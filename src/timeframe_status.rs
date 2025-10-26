/// Module de monitoring de la progression des timeframes
///
/// Ce module track la progression de chaque timeframe pour monitoring uniquement
use anyhow::Result;
use rusqlite::{Connection, params};
use std::time::{SystemTime, UNIX_EPOCH};

/// Gestionnaire du statut des timeframes
pub struct TimeframeStatus;

impl TimeframeStatus {
    /// Met à jour la progression d'un timeframe
    ///
    /// ALGORITHME:
    /// Appelé après chaque batch pour tracker la progression
    /// Utile pour monitoring et debug
    pub fn update_progress(
        conn: &Connection,
        provider: &str,
        symbol: &str,
        timeframe: &str,
        oldest_candle_time: i64,
    ) -> Result<()> {
        let now = Self::current_timestamp_ms()?;

        conn.execute(
            "INSERT OR REPLACE INTO timeframe_status
             (provider, symbol, timeframe, oldest_candle_time, last_updated)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![provider, symbol, timeframe, oldest_candle_time, now],
        )?;

        Ok(())
    }

    /// Récupère le timestamp actuel en millisecondes
    fn current_timestamp_ms() -> Result<i64> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as i64;
        Ok(timestamp)
    }

    /// Récupère le timestamp de la dernière bougie stockée
    ///
    /// ALGORITHME:
    /// Requête oldest_candle_time depuis timeframe_status
    /// Utilisé pour le mode de reprise intelligent (on remonte dans le temps)
    /// Si aucune entrée n'existe, retourne None (premier lancement)
    pub fn get_last_candle_time(
        conn: &Connection,
        provider: &str,
        symbol: &str,
        timeframe: &str,
    ) -> Option<i64> {
        conn.query_row(
            "SELECT oldest_candle_time FROM timeframe_status
             WHERE provider = ?1 AND symbol = ?2 AND timeframe = ?3",
            params![provider, symbol, timeframe],
            |row| row.get(0),
        )
        .unwrap_or(None)
    }
}
