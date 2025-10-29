/// Serveur web pour visualiser les données de candlesticks
///
/// ARCHITECTURE:
/// - API REST avec actix-web
/// - Sert les fichiers statiques (HTML/CSS/JS)
/// - Endpoints:
///   - GET /api/pairs → liste des paires disponibles
///   - GET /api/candles?symbol=X&timeframe=5m&limit=1000&offset=0
///   - GET /api/realtime/candles?symbol=X&timeframes=5m,15m,1h → bougies partielles temps réel
use actix::{Actor, ActorContext, AsyncContext, Handler, Message, StreamHandler};
use actix_cors::Cors;
use actix_files::Files;
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, Responder, get, web};
use actix_web_actors::ws;
use binance::api::*;
use binance::market::*;
use moka::future::Cache;
use rusqlite::{Connection, params};
use rust_candles_retriever::backfill::{BackfillOptions, run_backfill};
use rust_candles_retriever::realtime::RealtimeManager;
use rust_candles_retriever::retriever::CandleRetriever;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Clé de cache pour les requêtes de candles
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct CacheKey {
    symbol: String,
    timeframe: String,
    start: Option<i64>,
    end: Option<i64>,
    limit: usize,
    offset: usize,
}

/// État partagé de l'application
struct AppState {
    db_dir: String,
    realtime: Arc<RealtimeManager>,
    candles_cache: Cache<CacheKey, Arc<Vec<Candle>>>,
}

/// Représentation d'une bougie pour l'API
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Candle {
    time: i64, // timestamp en secondes (pour Lightweight Charts)
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}

/// Paire de trading disponible
#[derive(Debug, Serialize)]
struct TradingPair {
    symbol: String,
    timeframes: Vec<String>,
}

/// Paramètres de requête pour les candles
#[derive(Debug, Deserialize)]
struct CandlesQuery {
    symbol: String,
    timeframe: String,
    limit: Option<usize>,
    offset: Option<usize>,
    start: Option<i64>, // Timestamp de début en secondes
    end: Option<i64>,   // Timestamp de fin en secondes
}

/// GET /api/pairs - Récupère toutes les paires disponibles en scannant les fichiers .db
#[get("/api/pairs")]
async fn get_pairs(data: web::Data<Mutex<AppState>>) -> impl Responder {
    let db_dir = {
        let state = data.lock().unwrap();
        state.db_dir.clone()
    };

    // Déplacer toutes les opérations DB dans web::block
    let result = web::block(move || {
        let db_path = std::path::Path::new(&db_dir);
        let entries = std::fs::read_dir(db_path)
            .map_err(|e| format!("Failed to read db directory: {}", e))?;

        let mut pairs_map: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();

        // Pour chaque fichier .db
        for entry in entries.flatten() {
            let path = entry.path();

            // Vérifier que c'est un fichier .db
            if !path.is_file() {
                continue;
            }

            let file_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };

            if !file_name.ends_with(".db") {
                continue;
            }

            // Extraire le symbole du nom de fichier (ex: BTCUSDT.db -> BTCUSDT)
            let symbol = file_name.trim_end_matches(".db").to_string();

            // Ouvrir la base de données pour récupérer les timeframes
            let conn = match Connection::open(&path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Failed to open {}: {}", file_name, e);
                    continue;
                }
            };

            // Récupérer les timeframes pour ce symbole
            let mut stmt = match conn.prepare(
                "SELECT DISTINCT timeframe
                 FROM candlesticks
                 WHERE provider = 'binance'
                 ORDER BY timeframe",
            ) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to query timeframes for {}: {}", symbol, e);
                    continue;
                }
            };

            let timeframes: Vec<String> = match stmt.query_map([], |row| row.get(0)) {
                Ok(rows) => rows.filter_map(Result::ok).collect(),
                Err(e) => {
                    eprintln!("Failed to map timeframes for {}: {}", symbol, e);
                    continue;
                }
            };

            pairs_map.insert(symbol, timeframes);
        }

        let pairs: Vec<TradingPair> = pairs_map
            .into_iter()
            .map(|(symbol, timeframes)| TradingPair { symbol, timeframes })
            .collect();

        Ok::<Vec<TradingPair>, String>(pairs)
    })
    .await;

    match result {
        Ok(Ok(pairs)) => HttpResponse::Ok().json(pairs),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": e
        })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Blocking error: {}", e)
        })),
    }
}

/// GET /api/candles - Récupère les candles pour une paire/timeframe
#[get("/api/candles")]
async fn get_candles(
    data: web::Data<Mutex<AppState>>,
    query: web::Query<CandlesQuery>,
) -> impl Responder {
    let symbol = query.symbol.clone();
    let timeframe = query.timeframe.clone();
    let start = query.start;
    let end = query.end;
    let limit = query.limit.unwrap_or(2000);
    let offset = query.offset.unwrap_or(0);

    // Construire la clé de cache
    let cache_key = CacheKey {
        symbol: symbol.clone(),
        timeframe: timeframe.clone(),
        start,
        end,
        limit,
        offset,
    };

    // Extraire db_dir et cache en dehors du closure
    let (db_dir, cache) = {
        let state = data.lock().unwrap();
        (state.db_dir.clone(), state.candles_cache.clone())
    };

    // Vérifier le cache d'abord
    if let Some(cached_candles) = cache.get(&cache_key).await {
        return HttpResponse::Ok()
            .insert_header(("X-Cache", "HIT"))
            .json(cached_candles.as_ref());
    }

    // Cache miss - exécuter la requête DB
    let result = web::block(move || {
        let db_path = format!("{}/{}.db", db_dir, symbol);

        let conn = Connection::open(&db_path)
            .map_err(|e| format!("Database error for {}: {}", symbol, e))?;

        // Construire la requête SQL selon les paramètres
        let mut sql = String::from(
            "SELECT open_time, open, high, low, close, volume
             FROM candlesticks
             WHERE provider = 'binance'
               AND symbol = ?1
               AND timeframe = ?2",
        );

        let mut param_index = 3;

        // Ajouter filtre sur start (timestamp en secondes -> convertir en ms pour la DB)
        if start.is_some() {
            sql.push_str(&format!(" AND open_time >= ?{}", param_index));
            param_index += 1;
        }

        // Ajouter filtre sur end
        if end.is_some() {
            sql.push_str(&format!(" AND open_time <= ?{}", param_index));
            param_index += 1;
        }

        sql.push_str(" ORDER BY open_time ASC"); // ASC pour avoir l'ordre chronologique direct

        // Ajouter LIMIT et OFFSET
        sql.push_str(&format!(" LIMIT ?{}", param_index));
        param_index += 1;
        sql.push_str(&format!(" OFFSET ?{}", param_index));

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("Query error: {}", e))?;

        // Construire les paramètres dynamiquement
        let mut query_params: Vec<Box<dyn rusqlite::ToSql>> =
            vec![Box::new(symbol.clone()), Box::new(timeframe.clone())];

        if let Some(s) = start {
            query_params.push(Box::new(s * 1000)); // Convertir secondes en ms
        }

        if let Some(e) = end {
            query_params.push(Box::new(e * 1000)); // Convertir secondes en ms
        }

        query_params.push(Box::new(limit));
        query_params.push(Box::new(offset));

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            query_params.iter().map(|p| p.as_ref()).collect();

        let candles_iter = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(Candle {
                    time: row.get::<_, i64>(0)? / 1000, // Convertir ms en secondes
                    open: row.get(1)?,
                    high: row.get(2)?,
                    low: row.get(3)?,
                    close: row.get(4)?,
                    volume: row.get(5)?,
                })
            })
            .map_err(|e| format!("Query mapping error: {}", e))?;

        let mut candles: Vec<Candle> = Vec::new();
        for candle_result in candles_iter {
            if let Ok(candle) = candle_result {
                candles.push(candle);
            }
        }

        // Si aucune donnée, essayer le rééchantillonnage depuis une TF inférieure
        if candles.is_empty() {
            if let Some(smaller_tf) = find_smaller_timeframe(&conn, &symbol, &timeframe) {
                println!(
                    "⚠️ Pas de données pour {} {}, rééchantillonnage depuis {}",
                    symbol, timeframe, smaller_tf
                );

                candles =
                    resample_candles(&conn, &symbol, &smaller_tf, &timeframe, start, end, limit);
            }
        }

        Ok::<Vec<Candle>, String>(candles)
    })
    .await;

    match result {
        Ok(Ok(candles)) => {
            // Stocker dans le cache (TTL configuré au niveau du builder)
            let candles_arc = Arc::new(candles);
            cache.insert(cache_key, candles_arc.clone()).await;

            HttpResponse::Ok()
                .insert_header(("X-Cache", "MISS"))
                .json(candles_arc.as_ref())
        }
        Ok(Err(e)) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": e
        })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Blocking error: {}", e)
        })),
    }
}

/// Paramètres de requête pour les bougies temps réel
#[derive(Debug, Deserialize)]
struct RealtimeCandlesQuery {
    symbol: String,
    timeframes: String, // Format: "5m,15m,1h"
}

/// GET /api/realtime/candles - Récupère les bougies partielles temps réel
#[get("/api/realtime/candles")]
async fn get_realtime_candles(
    data: web::Data<Mutex<AppState>>,
    query: web::Query<RealtimeCandlesQuery>,
) -> impl Responder {
    let state = data.lock().unwrap();

    // Parser les timeframes
    let timeframes: Vec<String> = query
        .timeframes
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    // Récupérer les bougies partielles (sans re-souscrire)
    let candles = state.realtime.get_candles(&query.symbol, &timeframes);

    HttpResponse::Ok().json(candles)
}

/// Paramètres pour souscription manuelle
#[derive(Debug, Deserialize)]
struct SubscribeQuery {
    symbol: String,
    timeframes: String,
}

/// POST /api/realtime/subscribe - Souscrit à des streams
#[actix_web::post("/api/realtime/subscribe")]
async fn subscribe_realtime(
    data: web::Data<Mutex<AppState>>,
    query: web::Query<SubscribeQuery>,
) -> impl Responder {
    let state = data.lock().unwrap();

    let timeframes: Vec<String> = query
        .timeframes
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    for tf in &timeframes {
        state.realtime.subscribe(query.symbol.clone(), tf.clone());
    }

    HttpResponse::Ok().json(serde_json::json!({
        "status": "subscribed",
        "symbol": query.symbol,
        "timeframes": timeframes
    }))
}

/// Paramètres pour fetch dynamique
#[derive(Debug, Deserialize)]
struct FetchQuery {
    symbol: String,
    timeframe: String,
}

/// POST /api/fetch - Comble les gaps dynamiquement (boucle jusqu'à complet)
#[actix_web::post("/api/fetch")]
async fn fetch_gaps(
    data: web::Data<Mutex<AppState>>,
    query: web::Query<FetchQuery>,
) -> impl Responder {
    let db_dir = data.lock().unwrap().db_dir.clone();
    let symbol = query.symbol.clone();
    let timeframe = query.timeframe.clone();

    // Exécuter le fetch dans un thread bloquant avec son propre runtime
    let result = web::block(move || {
        let db_path = format!("{}/{}.db", db_dir, symbol);

        // Vérifier que la base existe
        if !std::path::Path::new(&db_path).exists() {
            return Err("Database not found for symbol".to_string());
        }

        // Ouvrir connexion
        let mut conn = match Connection::open(&db_path) {
            Ok(c) => c,
            Err(e) => return Err(format!("Failed to open database: {}", e)),
        };

        // Créer un nouveau client Binance dans ce thread (évite problème runtime Tokio)
        let market: Market = Binance::new(None, None);

        let mut total_inserted = 0i64;
        let mut iterations = 0;
        const MAX_ITERATIONS: i32 = 10; // Limite pour éviter boucle infinie

        // Boucler jusqu'à combler le gap ou atteindre limite
        loop {
            iterations += 1;
            if iterations > MAX_ITERATIONS {
                println!("⚠️ Max iterations atteintes pour {}/{}", symbol, timeframe);
                break;
            }

            // Créer retriever et fetch un batch
            let mut retriever = CandleRetriever::new(
                &market, &mut conn, &symbol, &timeframe, None, // Pas de date limite
            );

            match retriever.fetch_one_batch() {
                Ok((inserted, is_exhausted)) => {
                    total_inserted += inserted;
                    println!(
                        "📦 Batch {}: {} bougies insérées pour {}/{}",
                        iterations, inserted, symbol, timeframe
                    );

                    // Arrêter si: aucune insertion OU épuisé
                    if inserted == 0 || is_exhausted {
                        break;
                    }
                }
                Err(e) => {
                    return Err(format!("Fetch failed at iteration {}: {}", iterations, e));
                }
            }

            // Pause courte entre batches
            std::thread::sleep(std::time::Duration::from_millis(200));
        }

        Ok((
            symbol.clone(),
            timeframe.clone(),
            total_inserted,
            iterations,
        ))
    })
    .await;

    match result {
        Ok(Ok((sym, tf, inserted, iters))) => HttpResponse::Ok().json(serde_json::json!({
            "status": "success",
            "symbol": sym,
            "timeframe": tf,
            "inserted": inserted,
            "iterations": iters
        })),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": e
        })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Thread error: {}", e)
        })),
    }
}

/// Trouve une timeframe plus petite disponible
fn find_smaller_timeframe(conn: &Connection, symbol: &str, target_tf: &str) -> Option<String> {
    let timeframes = vec![
        "5m", "15m", "30m", "1h", "2h", "4h", "6h", "8h", "12h", "1d", "3d",
    ];
    let target_seconds = parse_timeframe_seconds(target_tf);

    // Chercher la plus grande TF qui est plus petite que target
    for tf in timeframes.iter().rev() {
        let tf_seconds = parse_timeframe_seconds(tf);
        if tf_seconds < target_seconds {
            // Vérifier si cette TF a des données
            let count: Result<i64, _> = conn.query_row(
                "SELECT COUNT(*) FROM candlesticks WHERE provider = 'binance' AND symbol = ?1 AND timeframe = ?2",
                params![symbol, tf],
                |row| row.get(0),
            );

            if let Ok(n) = count {
                if n > 0 {
                    return Some(tf.to_string());
                }
            }
        }
    }

    None
}

/// Parse une timeframe en secondes
fn parse_timeframe_seconds(tf: &str) -> i64 {
    if let Some(stripped) = tf.strip_suffix('m') {
        stripped.parse::<i64>().unwrap_or(0) * 60
    } else if let Some(stripped) = tf.strip_suffix('h') {
        stripped.parse::<i64>().unwrap_or(0) * 3600
    } else if let Some(stripped) = tf.strip_suffix('d') {
        stripped.parse::<i64>().unwrap_or(0) * 86400
    } else {
        0
    }
}

/// Rééchantillonne des candles depuis une TF inférieure
fn resample_candles(
    conn: &Connection,
    symbol: &str,
    source_tf: &str,
    target_tf: &str,
    start: Option<i64>,
    end: Option<i64>,
    limit: usize,
) -> Vec<Candle> {
    // Récupérer toutes les candles source dans la plage
    let mut sql = String::from(
        "SELECT open_time, open, high, low, close, volume
         FROM candlesticks
         WHERE provider = 'binance'
           AND symbol = ?1
           AND timeframe = ?2",
    );

    let mut param_index = 3;

    if start.is_some() {
        sql.push_str(&format!(" AND open_time >= ?{}", param_index));
        param_index += 1;
    }

    if end.is_some() {
        sql.push_str(&format!(" AND open_time <= ?{}", param_index));
    }

    sql.push_str(" ORDER BY open_time ASC LIMIT 50000");

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let mut query_params: Vec<Box<dyn rusqlite::ToSql>> = vec![
        Box::new(symbol.to_string()),
        Box::new(source_tf.to_string()),
    ];

    if let Some(s) = start {
        query_params.push(Box::new(s * 1000));
    }
    if let Some(e) = end {
        query_params.push(Box::new(e * 1000));
    }

    let params_refs: Vec<&dyn rusqlite::ToSql> = query_params.iter().map(|p| p.as_ref()).collect();

    let candles_iter = match stmt.query_map(params_refs.as_slice(), |row| {
        Ok(Candle {
            time: row.get::<_, i64>(0)? / 1000,
            open: row.get(1)?,
            high: row.get(2)?,
            low: row.get(3)?,
            close: row.get(4)?,
            volume: row.get(5)?,
        })
    }) {
        Ok(iter) => iter,
        Err(_) => return vec![],
    };

    let source_candles: Vec<Candle> = candles_iter.filter_map(|r| r.ok()).collect();

    if source_candles.is_empty() {
        return vec![];
    }

    let target_seconds = parse_timeframe_seconds(target_tf);

    // Grouper par période target
    let mut resampled: Vec<Candle> = Vec::new();
    let mut current_group: Vec<&Candle> = Vec::new();
    let mut current_period_start = (source_candles[0].time / target_seconds) * target_seconds;

    for candle in &source_candles {
        let period_start = (candle.time / target_seconds) * target_seconds;

        if period_start != current_period_start {
            // Agréger le groupe précédent
            if !current_group.is_empty() {
                resampled.push(aggregate_candles(&current_group, current_period_start));
                current_group.clear();
            }
            current_period_start = period_start;
        }

        current_group.push(candle);
    }

    // Agréger le dernier groupe
    if !current_group.is_empty() {
        resampled.push(aggregate_candles(&current_group, current_period_start));
    }

    // Limiter le nombre de résultats
    resampled.truncate(limit);
    resampled
}

/// Agrège un groupe de candles en une seule
fn aggregate_candles(candles: &[&Candle], period_start: i64) -> Candle {
    let open = candles.first().unwrap().open;
    let close = candles.last().unwrap().close;
    let high = candles
        .iter()
        .map(|c| c.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let low = candles.iter().map(|c| c.low).fold(f64::INFINITY, f64::min);
    let volume = candles.iter().map(|c| c.volume).sum();

    Candle {
        time: period_start,
        open,
        high,
        low,
        close,
        volume,
    }
}

/// GET /health - Health check
/// ============================================================================
/// MODULE WEBSOCKET - Communication temps réel avec les clients frontend
/// ============================================================================

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

/// Message WebSocket du client
#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
enum ClientMessage {
    #[serde(rename = "subscribe")]
    Subscribe {
        symbol: String,
        timeframes: Vec<String>,
    },
    #[serde(rename = "unsubscribe")]
    Unsubscribe {
        symbol: String,
        timeframes: Vec<String>,
    },
    #[serde(rename = "ping")]
    Ping,
}

/// Message WebSocket vers le client
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ServerMessage {
    #[serde(rename = "candle_update")]
    CandleUpdate {
        symbol: String,
        timeframe: String,
        candle: rust_candles_retriever::realtime::RealtimeCandle,
    },
    #[serde(rename = "subscribed")]
    Subscribed {
        symbol: String,
        timeframes: Vec<String>,
    },
    #[serde(rename = "pong")]
    Pong,
    #[serde(rename = "error")]
    Error { message: String },
}

/// Message Actix pour envoyer des mises à jour au client WebSocket
#[derive(Message, Clone)]
#[rtype(result = "()")]
struct BroadcastUpdate(rust_candles_retriever::realtime::CandleUpdate);

/// Session WebSocket pour un client
struct WsSession {
    /// Timestamp du dernier heartbeat
    hb: Instant,
    /// Référence au RealtimeManager
    realtime: Arc<RealtimeManager>,
    /// Souscriptions actives du client: (symbol, timeframe)
    subscriptions: Vec<(String, String)>,
}

impl WsSession {
    fn new(realtime: Arc<RealtimeManager>) -> Self {
        Self {
            hb: Instant::now(),
            realtime,
            subscriptions: Vec::new(),
        }
    }

    /// Démarre le heartbeat
    fn start_heartbeat(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                println!("⚠️ Client heartbeat timeout, disconnecting");
                ctx.stop();
                return;
            }
            ctx.ping(b"");
        });
    }

    /// Démarre l'écoute du canal de broadcast
    fn start_broadcast_listener(&self, ctx: &mut ws::WebsocketContext<Self>) {
        let mut rx = self.realtime.subscribe_updates();
        let addr = ctx.address();

        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(update) => {
                        addr.do_send(BroadcastUpdate(update));
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        eprintln!("⚠️ Broadcast lagging, some messages dropped");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        });
    }
}

impl Actor for WsSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        println!("🔌 New WebSocket client connected");
        self.start_heartbeat(ctx);
        self.start_broadcast_listener(ctx);
    }

    fn stopped(&mut self, _: &mut Self::Context) {
        println!("🔌 WebSocket client disconnected");
        // Désabonner de tous les streams
        for (symbol, timeframe) in &self.subscriptions {
            self.realtime.unsubscribe(symbol.clone(), timeframe.clone());
        }
    }
}

/// Handler pour les mises à jour de broadcast
impl Handler<BroadcastUpdate> for WsSession {
    type Result = ();

    fn handle(&mut self, msg: BroadcastUpdate, ctx: &mut Self::Context) {
        let update = msg.0;

        // Filtrer: envoyer seulement si le client est abonné à ce (symbol, timeframe)
        if self
            .subscriptions
            .contains(&(update.symbol.clone(), update.timeframe.clone()))
        {
            let server_msg = ServerMessage::CandleUpdate {
                symbol: update.symbol,
                timeframe: update.timeframe,
                candle: update.candle,
            };

            if let Ok(json) = serde_json::to_string(&server_msg) {
                ctx.text(json);
            }
        }
    }
}

/// Handler pour les messages WebSocket texte
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WsSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Text(text)) => {
                self.hb = Instant::now();

                // Parser le message JSON du client
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(ClientMessage::Subscribe { symbol, timeframes }) => {
                        println!("📥 Client subscribing to {} {:?}", symbol, timeframes);

                        // Souscrire aux streams Binance
                        for tf in &timeframes {
                            self.realtime.subscribe(symbol.clone(), tf.clone());
                            self.subscriptions.push((symbol.clone(), tf.clone()));
                        }

                        // Confirmer la souscription
                        let response = ServerMessage::Subscribed {
                            symbol: symbol.clone(),
                            timeframes: timeframes.clone(),
                        };
                        if let Ok(json) = serde_json::to_string(&response) {
                            ctx.text(json);
                        }
                    }
                    Ok(ClientMessage::Unsubscribe { symbol, timeframes }) => {
                        println!("📥 Client unsubscribing from {} {:?}", symbol, timeframes);

                        for tf in &timeframes {
                            self.realtime.unsubscribe(symbol.clone(), tf.clone());
                            self.subscriptions
                                .retain(|(s, t)| !(s == &symbol && t == tf));
                        }
                    }
                    Ok(ClientMessage::Ping) => {
                        let response = ServerMessage::Pong;
                        if let Ok(json) = serde_json::to_string(&response) {
                            ctx.text(json);
                        }
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to parse client message: {}", e);
                        let error = ServerMessage::Error {
                            message: format!("Invalid message format: {}", e),
                        };
                        if let Ok(json) = serde_json::to_string(&error) {
                            ctx.text(json);
                        }
                    }
                }
            }
            Ok(ws::Message::Ping(msg)) => {
                self.hb = Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                self.hb = Instant::now();
            }
            Ok(ws::Message::Close(reason)) => {
                println!("🔌 Client closing connection: {:?}", reason);
                ctx.close(reason);
                ctx.stop();
            }
            _ => {}
        }
    }
}

/// Endpoint WebSocket pour les mises à jour temps réel
async fn ws_realtime(
    req: HttpRequest,
    stream: web::Payload,
    data: web::Data<Mutex<AppState>>,
) -> Result<HttpResponse, actix_web::Error> {
    let realtime = {
        let state = data.lock().unwrap();
        Arc::clone(&state.realtime)
    };

    let session = WsSession::new(realtime);
    ws::start(session, &req, stream)
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let db_dir = std::env::var("DB_DIR").unwrap_or_else(|_| ".".to_string());
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .unwrap_or(8080);

    println!("🚀 Démarrage du serveur web sur http://127.0.0.1:{}", port);
    println!("📊 Répertoire bases de données: {}", db_dir);
    println!("📁 Fichiers statiques: ./web");

    // Lancer le backfill automatique pour toutes les paires existantes
    start_auto_backfill(db_dir.clone());

    // Initialiser le gestionnaire de bougies temps réel
    let realtime = Arc::new(RealtimeManager::new());
    println!("🔌 Gestionnaire WebSocket temps réel initialisé");

    // Initialiser le cache pour les requêtes de candles
    let candles_cache: Cache<CacheKey, Arc<Vec<Candle>>> = Cache::builder()
        .max_capacity(1000)
        .time_to_live(Duration::from_secs(60))
        .build();
    println!("💾 Cache de candles initialisé (max 1000 entrées, TTL 60s)");

    let app_state = web::Data::new(Mutex::new(AppState {
        db_dir,
        realtime,
        candles_cache,
    }));

    HttpServer::new(move || {
        let cors = Cors::permissive();

        App::new()
            .wrap(cors)
            .app_data(app_state.clone())
            .service(health)
            .service(get_pairs)
            .service(get_candles)
            .service(fetch_gaps)
            .route("/ws/realtime", web::get().to(ws_realtime))
            .service(Files::new("/", "./web").index_file("index.html"))
    })
    .bind(("127.0.0.1", port))?
    .run()
    .await
}

/// Lance le backfill automatique pour toutes les paires disponibles
fn start_auto_backfill(db_dir: String) {
    tokio::spawn(async move {
        println!("🔄 Démarrage du backfill automatique...");

        // Scanner le répertoire pour trouver toutes les paires
        let db_path = std::path::Path::new(&db_dir);
        let entries = match std::fs::read_dir(db_path) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("❌ Impossible de lire le répertoire DB: {}", e);
                return;
            }
        };

        let mut pairs = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    if file_name.ends_with(".db") {
                        let symbol = file_name.trim_end_matches(".db").to_string();
                        pairs.push(symbol);
                    }
                }
            }
        }

        println!("📋 Paires trouvées pour backfill: {:?}", pairs);

        // Lancer le backfill pour chaque paire en parallèle
        let mut tasks = Vec::new();
        for symbol in pairs {
            let db_dir_clone = db_dir.clone();
            let task = tokio::spawn(async move {
                let options = BackfillOptions::new(symbol.clone(), db_dir_clone);
                if let Err(e) = run_backfill(options).await {
                    eprintln!("❌ Erreur backfill pour {}: {}", symbol, e);
                }
            });
            tasks.push(task);
        }

        // Attendre que tous les backfills se terminent
        for task in tasks {
            let _ = task.await;
        }

        println!("✅ Backfill automatique terminé pour toutes les paires");
    });
}
