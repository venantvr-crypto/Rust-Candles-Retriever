/// Module de gestion du statut de complétion des timeframes
///
/// Ce module fournit des fonctionnalités pour tracker quels timeframes
/// ont été entièrement récupérés (jusqu'à la limite historique ou date limite)
use anyhow::Result;
use rusqlite::{Connection, params};
use std::time::{SystemTime, UNIX_EPOCH};

/// Gestionnaire du statut des timeframes
///
/// ARCHITECTURE:
/// Fournit des méthodes statiques (associées) pour interroger et mettre à jour
/// le statut de complétion des timeframes dans la base de données
pub struct TimeframeStatus;

impl TimeframeStatus {
    /// Vérifie si un timeframe est marqué comme complet
    ///
    /// RETOUR:
    /// - true: timeframe complet (déjà traité jusqu'à la limite)
    /// - false: timeframe incomplet ou jamais traité
    ///
    /// SUBTILITÉ RUST: Fonction associée (pas de &self)
    /// Similaire à une méthode statique en POO classique
    pub fn is_complete(conn: &Connection, provider: &str, symbol: &str, timeframe: &str) -> bool {
        conn.query_row(
            "SELECT is_complete FROM timeframe_status
             WHERE provider = ?1 AND symbol = ?2 AND timeframe = ?3",
            params![provider, symbol, timeframe],
            |row| row.get(0),
        )
        .unwrap_or(0)
            == 1
    }

    /// Marque un timeframe comme complet
    ///
    /// ALGORITHME:
    /// Appelé dans deux situations:
    /// 1. L'API retourne 0 bougies (limite historique atteinte)
    /// 2. La date --start-date est atteinte
    ///
    /// PARAMÈTRES:
    /// - oldest_candle_time: timestamp de la plus ancienne bougie récupérée
    pub fn mark_complete(
        conn: &Connection,
        provider: &str,
        symbol: &str,
        timeframe: &str,
        oldest_candle_time: Option<i64>,
    ) -> Result<()> {
        let now = Self::current_timestamp_ms()?;

        conn.execute(
            "INSERT OR REPLACE INTO timeframe_status
             (provider, symbol, timeframe, oldest_candle_time, is_complete, last_updated)
             VALUES (?1, ?2, ?3, ?4, 1, ?5)",
            params![provider, symbol, timeframe, oldest_candle_time, now],
        )?;

        Ok(())
    }

    /// Met à jour la progression d'un timeframe (sans le marquer comme complet)
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
             (provider, symbol, timeframe, oldest_candle_time, is_complete, last_updated)
             VALUES (?1, ?2, ?3, ?4, 0, ?5)",
            params![provider, symbol, timeframe, oldest_candle_time, now],
        )?;

        Ok(())
    }

    /// Récupère le timestamp actuel en millisecondes
    ///
    /// SUBTILITÉ RUST: Méthode helper privée
    /// Utilise Self::method() pour appeler une autre méthode associée
    fn current_timestamp_ms() -> Result<i64> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as i64;
        Ok(timestamp)
    }

    /// Récupère le timestamp de la dernière bougie stockée
    ///
    /// ALGORITHME:
    /// Requête MAX(open_time) pour un (provider, symbol, timeframe)
    /// Utilisé pour le mode de reprise intelligent
    pub fn get_last_candle_time(
        conn: &Connection,
        provider: &str,
        symbol: &str,
        timeframe: &str,
    ) -> Option<i64> {
        conn.query_row(
            "SELECT MAX(open_time) FROM candlesticks
             WHERE provider = ?1 AND symbol = ?2 AND timeframe = ?3",
            params![provider, symbol, timeframe],
            |row| row.get(0),
        )
        .unwrap_or(None)
    }
}
