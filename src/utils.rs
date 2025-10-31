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

/// Convertit un timeframe en intervalle en millisecondes
///
/// USAGE: Fonction centralisée pour éviter duplication
///
/// EXEMPLES:
/// - "1m" → 60_000 (1 minute)
/// - "15m" → 900_000 (15 minutes)
/// - "1h" → 3_600_000 (1 heure)
/// - "1d" → 86_400_000 (1 jour)
pub fn timeframe_to_interval(timeframe: &str) -> i64 {
    match timeframe {
        "1m" => 60_000,
        "3m" => 180_000,
        "5m" => 300_000,
        "15m" => 900_000,
        "30m" => 1_800_000,
        "1h" => 3_600_000,
        "2h" => 7_200_000,
        "4h" => 14_400_000,
        "6h" => 21_600_000,
        "8h" => 28_800_000,
        "12h" => 43_200_000,
        "1d" => 86_400_000,
        "3d" => 259_200_000,
        "1w" => 604_800_000,
        "1M" => 2_592_000_000,
        _ => 300_000, // Défaut: 5m
    }
}
