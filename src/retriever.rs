/// Module de récupération des bougies depuis l'API Binance
///
/// ARCHITECTURE SIMPLIFIÉE:
/// - Récupère UN batch à la fois
/// - Retourne le nombre d'insertions réelles et si le timeframe est épuisé
/// - Pas de boucle interne, la boucle est dans main.rs
use crate::gap_filler::GapFiller;
use crate::timeframe_status::TimeframeStatus;
use anyhow::Result;
use binance::market::*;
use binance::model::KlineSummaries;
use rusqlite::{Connection, params};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const BATCH_SIZE: usize = 1000;
const PROVIDER: &str = "binance";

/// Récupérateur de bougies depuis Binance
pub struct CandleRetriever<'a> {
    market: &'a Market,
    conn: &'a mut Connection,
    symbol: &'a str,
    timeframe: &'a str,
    start_timestamp_ms: Option<i64>,
}

impl<'a> CandleRetriever<'a> {
    /// Crée un nouveau récupérateur
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

    /// Récupère et insère UN batch de bougies
    ///
    /// RETOUR: (nombre_insertions_reelles, is_exhausted)
    /// - nombre_insertions_reelles: nouvelles bougies insérées (pas les doublons)
    /// - is_exhausted: true si le timeframe est épuisé (toutes les bougies déjà en base)
    pub fn fetch_one_batch(&mut self) -> Result<(i64, bool)> {
        // Déterminer le point de départ (dernière bougie stockée ou maintenant)
        let end_time_ms = self.determine_start_point()?;

        // Récupérer le batch depuis l'API (TOUJOURS en backward)
        let klines = match self.fetch_batch(end_time_ms) {
            Ok(k) => k,
            Err(e) => {
                thread::sleep(Duration::from_secs(5));
                return Err(e);
            }
        };

        // Vérifier si on a atteint la limite historique
        if klines.is_empty() {
            return Ok((0, true)); // Épuisé: API ne retourne plus rien
        }

        let oldest_kline_time = klines[0].open_time;
        let newest_kline_time = klines[klines.len() - 1].open_time;

        // Insérer le batch
        let inserted = self.insert_batch(&klines)?;

        // Mettre à jour la progression pour monitoring
        let _ = TimeframeStatus::update_progress(
            self.conn,
            PROVIDER,
            self.symbol,
            self.timeframe,
            oldest_kline_time,
        );

        // Combler les gaps
        let _ = GapFiller::fill_gaps_in_range(
            self.conn,
            PROVIDER,
            self.symbol,
            self.timeframe,
            oldest_kline_time,
            newest_kline_time,
        );

        // Épuisé si: aucune insertion (tout déjà en base) OU date limite atteinte
        let is_exhausted = inserted == 0 || self.is_date_limit_reached(oldest_kline_time);

        Ok((inserted, is_exhausted))
    }

    /// Détermine le point de départ (dernière bougie stockée ou maintenant)
    fn determine_start_point(&self) -> Result<i64> {
        let last_stored =
            TimeframeStatus::get_last_candle_time(self.conn, PROVIDER, self.symbol, self.timeframe);

        let end_time_ms = match last_stored {
            Some(last_time) => {
                // Mode reprise: continuer depuis la dernière bougie
                last_time
            }
            None => {
                // Mode première exécution: partir de maintenant
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as i64
            }
        };

        Ok(end_time_ms)
    }

    /// Récupère un batch de bougies depuis l'API Binance (TOUJOURS en backward)
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

        let mut klines = match klines_data {
            KlineSummaries::AllKlineSummaries(vec) => vec,
        };

        // IMPORTANT: Filtrer les bougies incomplètes (en cours de formation)
        // Une bougie est complète si son close_time est dans le passé
        let now_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as i64;

        klines.retain(|k| k.close_time < now_ms);

        Ok(klines)
    }

    /// Insère un batch de bougies dans la base de données
    ///
    /// RETOUR: Nombre de bougies réellement insérées (pas les doublons)
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

    /// Vérifie si la date limite utilisateur est atteinte
    fn is_date_limit_reached(&self, oldest_kline_time: i64) -> bool {
        if let Some(start_ts) = self.start_timestamp_ms {
            oldest_kline_time <= start_ts
        } else {
            false
        }
    }
}
