/// Module utilitaire pour les fonctions partagées
use chrono::{DateTime, Utc};

/// Formate un timestamp en millisecondes en format lisible
///
/// EXEMPLE:
/// 1700000000000 → "2023-11-14 22:13:20 UTC"
pub fn format_timestamp_ms(timestamp_ms: i64) -> String {
    if let Some(datetime_utc) = DateTime::<Utc>::from_timestamp_millis(timestamp_ms) {
        datetime_utc.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        "Invalid timestamp".to_string()
    }
}
