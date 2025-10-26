/// Module de récupération des bougies depuis l'API Binance
///
/// Ce module gère le téléchargement des données historiques,
/// le mode de reprise intelligent, et la détection de complétion
use crate::gap_filler::GapFiller;
use crate::timeframe_status::TimeframeStatus;
use crate::utils;
use anyhow::Result;
use binance::market::*;
use binance::model::KlineSummaries;
use rusqlite::{Connection, params};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const BATCH_SIZE: usize = 1000;
const PROVIDER: &str = "binance";

/// Récupérateur de bougies depuis Binance
///
/// ARCHITECTURE:
/// Encapsule la logique de récupération par batch avec mode de reprise
pub struct CandleRetriever<'a> {
    market: &'a Market,
    conn: &'a mut Connection,
    symbol: &'a str,
    timeframe: &'a str,
    start_timestamp_ms: Option<i64>,
}

impl<'a> CandleRetriever<'a> {
    /// Crée un nouveau récupérateur
    ///
    /// SUBTILITÉ RUST: Lifetime 'a
    /// Toutes les références doivent vivre au moins aussi longtemps que le CandleRetriever
    pub fn new(
        market: &'a Market,
        conn: &'a mut Connection,
        symbol: &'a str,
        timeframe: &'a str,
        start_timestamp_ms: Option<i64>,
    ) -> Self {
        CandleRetriever {
            market,
            conn,
            symbol,
            timeframe,
            start_timestamp_ms,
        }
    }

    /// Lance la récupération des bougies
    ///
    /// ALGORITHME:
    /// 1. Détermine le point de départ (mode reprise ou première exécution)
    /// 2. Boucle de récupération par batch de 1000 bougies
    /// 3. Insère les bougies en DB avec transactions
    /// 4. Comble les gaps avec interpolation
    /// 5. Met à jour la progression
    /// 6. Détecte la complétion (limite historique ou date limite)
    ///
    /// RETOUR: Nombre total de bougies insérées (réelles + interpolées)
    pub fn fetch_and_store(&mut self) -> Result<i64> {
        let mut total_inserted = 0i64;

        // Déterminer le point de départ
        let mut end_time_ms = self.determine_start_point()?;

        // Boucle principale de récupération
        loop {
            println!(
                "Fetching {} klines ending before {}",
                BATCH_SIZE,
                utils::format_timestamp_ms(end_time_ms)
            );

            // Récupérer le batch depuis l'API
            let klines = match self.fetch_batch(end_time_ms) {
                Ok(k) => k,
                Err(e) => {
                    eprintln!("Erreur API Binance: {}", e);
                    thread::sleep(Duration::from_secs(5));
                    continue;
                }
            };

            // Vérifier si on a atteint la limite historique
            if klines.is_empty() {
                self.handle_historical_limit_reached()?;
                break;
            }

            let oldest_kline_time = klines[0].open_time;

            // Insérer le batch
            let inserted = self.insert_batch(&klines)?;
            total_inserted += inserted;

            println!(
                "Batch traité pour {}/{}. {} nouvelles bougies insérées. Bougie la plus ancienne: {}",
                self.symbol,
                self.timeframe,
                inserted,
                utils::format_timestamp_ms(oldest_kline_time)
            );

            // Mettre à jour la progression
            TimeframeStatus::update_progress(
                self.conn,
                PROVIDER,
                self.symbol,
                self.timeframe,
                oldest_kline_time,
            )?;

            // Combler les gaps
            let filled = GapFiller::fill_gaps_in_range(
                self.conn,
                PROVIDER,
                self.symbol,
                self.timeframe,
                oldest_kline_time,
                end_time_ms,
            )?;

            if filled > 0 {
                println!("  → {} bougies interpolées pour combler les trous", filled);
                total_inserted += filled;
            }

            // Préparer pour le prochain batch
            end_time_ms = oldest_kline_time;

            // Vérifier si on a atteint la date limite utilisateur
            if self.check_date_limit_reached(oldest_kline_time)? {
                break;
            }

            // Pause pour respecter les rate limits
            thread::sleep(Duration::from_millis(500));
        }

        Ok(total_inserted)
    }

    /// Détermine le point de départ (mode reprise ou première exécution)
    ///
    /// ALGORITHME:
    /// 1. Vérifie si des données existent déjà
    /// 2. Si OUI → MODE REPRISE depuis la dernière bougie
    /// 3. Si NON → MODE PREMIÈRE EXÉCUTION depuis maintenant
    fn determine_start_point(&self) -> Result<i64> {
        let last_stored =
            TimeframeStatus::get_last_candle_time(self.conn, PROVIDER, self.symbol, self.timeframe);

        let end_time_ms = match last_stored {
            Some(last_time) => {
                println!("╔════════════════════════════════════════════════════════════");
                println!("║ MODE REPRISE ACTIVÉ");
                println!("╠════════════════════════════════════════════════════════════");
                println!("║ Provider: {}", PROVIDER);
                println!("║ Symbole: {}", self.symbol);
                println!("║ Timeframe: {}", self.timeframe);
                println!(
                    "║ Dernière bougie en base: {}",
                    utils::format_timestamp_ms(last_time)
                );
                println!("║ Récupération: depuis cette date vers le passé");

                if let Some(start_ts) = self.start_timestamp_ms {
                    println!(
                        "║ Limite de récupération: {}",
                        utils::format_timestamp_ms(start_ts)
                    );
                } else {
                    println!("║ Limite de récupération: toutes les données disponibles");
                }
                println!("╚════════════════════════════════════════════════════════════\n");

                last_time
            }
            None => {
                let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as i64;

                println!("╔════════════════════════════════════════════════════════════");
                println!("║ MODE PREMIÈRE EXÉCUTION");
                println!("╠════════════════════════════════════════════════════════════");
                println!("║ Provider: {}", PROVIDER);
                println!("║ Symbole: {}", self.symbol);
                println!("║ Timeframe: {}", self.timeframe);
                println!("║ Aucune donnée existante pour cette combinaison");
                println!("║ Démarrage: {}", utils::format_timestamp_ms(now));

                if let Some(start_ts) = self.start_timestamp_ms {
                    println!(
                        "║ Récupération jusqu'à: {}",
                        utils::format_timestamp_ms(start_ts)
                    );
                } else {
                    println!("║ Récupération: toutes les données historiques disponibles");
                }
                println!("╚════════════════════════════════════════════════════════════\n");

                now
            }
        };

        Ok(end_time_ms)
    }

    /// Récupère un batch de bougies depuis l'API Binance
    fn fetch_batch(&self, end_time_ms: i64) -> Result<Vec<binance::model::KlineSummary>> {
        let klines_data = self
            .market
            .get_klines(
                self.symbol,
                self.timeframe,
                Some(BATCH_SIZE as u16),
                None,
                Some(end_time_ms as u64),
            )
            .map_err(|e| anyhow::anyhow!("Erreur API Binance: {:?}", e))?;

        let klines = match klines_data {
            KlineSummaries::AllKlineSummaries(vec) => vec,
        };

        Ok(klines)
    }

    /// Insère un batch de bougies dans la base de données
    ///
    /// SUBTILITÉ RUST: Utilise une transaction pour l'atomicité
    fn insert_batch(&mut self, klines: &[binance::model::KlineSummary]) -> Result<i64> {
        let tx = self.conn.transaction()?;
        let mut inserted = 0i64;

        {
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO candlesticks (
                    provider, symbol, timeframe, open_time, open, high, low, close, volume,
                    close_time, quote_asset_volume, number_of_trades,
                    taker_buy_base_asset_volume, taker_buy_quote_asset_volume, interpolated
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            )?;

            for kline in klines {
                let changes = stmt.execute(params![
                    PROVIDER,
                    self.symbol,
                    self.timeframe,
                    kline.open_time,
                    kline.open.parse::<f64>().unwrap_or(0.0),
                    kline.high.parse::<f64>().unwrap_or(0.0),
                    kline.low.parse::<f64>().unwrap_or(0.0),
                    kline.close.parse::<f64>().unwrap_or(0.0),
                    kline.volume.parse::<f64>().unwrap_or(0.0),
                    kline.close_time,
                    kline.quote_asset_volume.parse::<f64>().unwrap_or(0.0),
                    kline.number_of_trades,
                    kline
                        .taker_buy_base_asset_volume
                        .parse::<f64>()
                        .unwrap_or(0.0),
                    kline
                        .taker_buy_quote_asset_volume
                        .parse::<f64>()
                        .unwrap_or(0.0),
                    0, // interpolated = 0 (données réelles)
                ])?;

                if changes > 0 {
                    inserted += 1;
                }
            }
        }

        tx.commit()?;
        Ok(inserted)
    }

    /// Gère le cas où la limite historique de l'API est atteinte
    fn handle_historical_limit_reached(&mut self) -> Result<()> {
        println!(
            "Aucune bougie supplémentaire retournée par l'API. Arrêt pour {}/{}.",
            self.symbol, self.timeframe
        );

        let oldest_time =
            TimeframeStatus::get_last_candle_time(self.conn, PROVIDER, self.symbol, self.timeframe);

        TimeframeStatus::mark_complete(
            self.conn,
            PROVIDER,
            self.symbol,
            self.timeframe,
            oldest_time,
        )?;

        println!(
            "✅ Timeframe {}/{} marqué comme complet (limite historique atteinte)",
            self.symbol, self.timeframe
        );

        Ok(())
    }

    /// Vérifie si la date limite utilisateur est atteinte
    ///
    /// RETOUR: true si la date limite est atteinte (arrêt de la boucle)
    fn check_date_limit_reached(&mut self, oldest_kline_time: i64) -> Result<bool> {
        if let Some(start_ts) = self.start_timestamp_ms {
            if oldest_kline_time <= start_ts {
                println!(
                    "Date de début ({}) atteinte ou dépassée. Arrêt pour {}/{}.",
                    utils::format_timestamp_ms(start_ts),
                    self.symbol,
                    self.timeframe
                );

                let oldest_time = TimeframeStatus::get_last_candle_time(
                    self.conn,
                    PROVIDER,
                    self.symbol,
                    self.timeframe,
                );

                TimeframeStatus::mark_complete(
                    self.conn,
                    PROVIDER,
                    self.symbol,
                    self.timeframe,
                    oldest_time,
                )?;

                println!(
                    "✅ Timeframe {}/{} marqué comme complet (date limite atteinte)",
                    self.symbol, self.timeframe
                );

                return Ok(true);
            }
        }

        Ok(false)
    }
}
