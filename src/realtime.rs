use futures_util::StreamExt;
/// Module de gestion des bougies temps réel via WebSocket Binance
///
/// Architecture:
/// - Thread dédié qui maintient des connexions WebSocket à Binance
/// - Cache en mémoire des dernières bougies partielles (HashMap)
/// - API pour souscrire/désouscrire à des (symbol, timeframe)
///
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

/// Bougie partielle temps réel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealtimeCandle {
    pub time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub is_closed: bool,
}

/// Message Binance Kline
#[derive(Debug, Deserialize)]
struct BinanceKlineEvent {
    #[serde(rename = "e")]
    event_type: String,
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "k")]
    kline: BinanceKline,
}

#[derive(Debug, Deserialize)]
struct BinanceKline {
    #[serde(rename = "t")]
    start_time: i64,
    #[serde(rename = "o")]
    open: String,
    #[serde(rename = "h")]
    high: String,
    #[serde(rename = "l")]
    low: String,
    #[serde(rename = "c")]
    close: String,
    #[serde(rename = "v")]
    volume: String,
    #[serde(rename = "x")]
    is_closed: bool,
}

/// Clé unique pour identifier une bougie (symbol, timeframe)
type StreamKey = (String, String);

/// Événement de mise à jour de bougie pour broadcast
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandleUpdate {
    pub symbol: String,
    pub timeframe: String,
    pub candle: RealtimeCandle,
}

/// Gestionnaire de connexions WebSocket temps réel
pub struct RealtimeManager {
    /// Cache des dernières bougies partielles: (symbol, tf) -> candle
    cache: Arc<RwLock<HashMap<StreamKey, RealtimeCandle>>>,
    /// Canal pour envoyer des commandes au thread de gestion
    command_tx: mpsc::UnboundedSender<Command>,
    /// Canal de broadcast pour les mises à jour de bougies
    broadcast_tx: tokio::sync::broadcast::Sender<CandleUpdate>,
}

/// Commandes pour le gestionnaire
enum Command {
    Subscribe { symbol: String, timeframe: String },
    Unsubscribe { symbol: String, timeframe: String },
    Shutdown,
}

impl RealtimeManager {
    /// Crée un nouveau gestionnaire et lance le thread de gestion
    pub fn new() -> Self {
        let cache = Arc::new(RwLock::new(HashMap::new()));
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (broadcast_tx, _) = tokio::sync::broadcast::channel(1000);

        let manager_cache = Arc::clone(&cache);
        let manager_broadcast = broadcast_tx.clone();

        // Lancer le thread de gestion en arrière-plan
        tokio::spawn(async move {
            Self::run_manager(manager_cache, command_rx, manager_broadcast).await;
        });

        Self {
            cache,
            command_tx,
            broadcast_tx,
        }
    }

    /// S'abonne au canal de broadcast pour recevoir les mises à jour
    pub fn subscribe_updates(&self) -> tokio::sync::broadcast::Receiver<CandleUpdate> {
        self.broadcast_tx.subscribe()
    }

    /// Souscrit à un stream (symbol, timeframe)
    pub fn subscribe(&self, symbol: String, timeframe: String) {
        let _ = self
            .command_tx
            .send(Command::Subscribe { symbol, timeframe });
    }

    /// Se désabonne d'un stream
    pub fn unsubscribe(&self, symbol: String, timeframe: String) {
        let _ = self
            .command_tx
            .send(Command::Unsubscribe { symbol, timeframe });
    }

    /// Récupère les bougies partielles pour des (symbol, timeframes)
    pub fn get_candles(
        &self,
        symbol: &str,
        timeframes: &[String],
    ) -> HashMap<String, Option<RealtimeCandle>> {
        let cache = self.cache.read().unwrap();
        let mut result = HashMap::new();

        for tf in timeframes {
            let key = (symbol.to_string(), tf.clone());
            result.insert(tf.clone(), cache.get(&key).cloned());
        }

        result
    }

    /// Thread principal de gestion des WebSockets
    async fn run_manager(
        cache: Arc<RwLock<HashMap<StreamKey, RealtimeCandle>>>,
        mut command_rx: mpsc::UnboundedReceiver<Command>,
        broadcast_tx: tokio::sync::broadcast::Sender<CandleUpdate>,
    ) {
        let mut active_streams: HashMap<StreamKey, tokio::task::JoinHandle<()>> = HashMap::new();

        while let Some(cmd) = command_rx.recv().await {
            match cmd {
                Command::Subscribe { symbol, timeframe } => {
                    let key = (symbol.clone(), timeframe.clone());

                    if active_streams.contains_key(&key) {
                        eprintln!("⚠️ Already subscribed to {:?}", key);
                        continue;
                    }

                    println!("🔌 Subscribing to {:?}", key);

                    let stream_cache = Arc::clone(&cache);
                    let stream_symbol = symbol.clone();
                    let stream_tf = timeframe.clone();
                    let stream_broadcast = broadcast_tx.clone();

                    // Lancer une task pour ce stream
                    let handle = tokio::spawn(async move {
                        Self::handle_stream(
                            stream_cache,
                            stream_symbol,
                            stream_tf,
                            stream_broadcast,
                        )
                        .await;
                    });

                    active_streams.insert(key, handle);
                }

                Command::Unsubscribe { symbol, timeframe } => {
                    let key = (symbol.clone(), timeframe.clone());

                    if let Some(handle) = active_streams.remove(&key) {
                        println!("🛑 Unsubscribing from {:?}", key);
                        handle.abort();

                        // Nettoyer le cache
                        cache.write().unwrap().remove(&key);
                    }
                }

                Command::Shutdown => {
                    println!("🛑 Shutting down realtime manager");
                    for (_, handle) in active_streams.drain() {
                        handle.abort();
                    }
                    break;
                }
            }
        }
    }

    /// Gère un stream WebSocket Binance spécifique
    async fn handle_stream(
        cache: Arc<RwLock<HashMap<StreamKey, RealtimeCandle>>>,
        symbol: String,
        timeframe: String,
        broadcast_tx: tokio::sync::broadcast::Sender<CandleUpdate>,
    ) {
        let binance_interval = Self::to_binance_interval(&timeframe);
        let stream_name = format!("{}@kline_{}", symbol.to_lowercase(), binance_interval);
        let url = format!("wss://stream.binance.com:9443/ws/{}", stream_name);

        loop {
            println!("📡 Connecting to {}", url);

            match connect_async(&url).await {
                Ok((ws_stream, _)) => {
                    println!("✅ Connected to {}", stream_name);

                    let (mut _write, mut read) = ws_stream.split();

                    // Lire les messages
                    while let Some(msg) = read.next().await {
                        match msg {
                            Ok(Message::Text(text)) => {
                                if let Ok(event) = serde_json::from_str::<BinanceKlineEvent>(&text)
                                {
                                    if event.event_type == "kline" {
                                        let candle = RealtimeCandle {
                                            time: event.kline.start_time / 1000, // ms → s
                                            open: event.kline.open.parse().unwrap_or(0.0),
                                            high: event.kline.high.parse().unwrap_or(0.0),
                                            low: event.kline.low.parse().unwrap_or(0.0),
                                            close: event.kline.close.parse().unwrap_or(0.0),
                                            volume: event.kline.volume.parse().unwrap_or(0.0),
                                            is_closed: event.kline.is_closed,
                                        };

                                        // Mettre à jour le cache
                                        let key = (symbol.clone(), timeframe.clone());
                                        cache.write().unwrap().insert(key, candle.clone());

                                        // Broadcaster la mise à jour aux clients WebSocket
                                        let update = CandleUpdate {
                                            symbol: symbol.clone(),
                                            timeframe: timeframe.clone(),
                                            candle,
                                        };
                                        let _ = broadcast_tx.send(update);
                                    }
                                }
                            }
                            Ok(Message::Close(_)) => {
                                println!("🔌 Connection closed for {}", stream_name);
                                break;
                            }
                            Err(e) => {
                                eprintln!("❌ WebSocket error for {}: {}", stream_name, e);
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    eprintln!("❌ Failed to connect to {}: {}", stream_name, e);
                }
            }

            // Attendre avant de reconnecter
            println!("⏰ Reconnecting to {} in 5s...", stream_name);
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
    }

    /// Convertit une timeframe interne vers le format Binance
    fn to_binance_interval(tf: &str) -> String {
        // Notre format: "5m", "1h", "1d"
        // Binance: "5m", "1h", "1d" → identique
        tf.to_string()
    }
}
