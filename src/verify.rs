use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};

/// Vérifie que les dates dans la base de données sont espacées de façon homogène
/// Retourne un rapport avec les statistiques et les anomalies trouvées
pub fn verify_data_spacing(
    conn: &Connection,
    provider: &str,
    symbol: &str,
    timeframe: &str,
) -> Result<()> {
    // Déterminer l'intervalle attendu en millisecondes selon le timeframe
    let expected_interval_ms = match timeframe {
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
        "1M" => 2_592_000_000, // 30 jours approximatif
        _ => {
            eprintln!("Timeframe inconnu: {}", timeframe);
            return Ok(());
        }
    };

    println!(
        "\n=== Vérification de l'espacement pour {}/{}/{} ===",
        provider, symbol, timeframe
    );
    println!(
        "Intervalle attendu: {} ms ({} minutes)",
        expected_interval_ms,
        expected_interval_ms / 60_000
    );

    // Récupérer toutes les bougies triées par date
    let mut stmt = conn.prepare(
        "SELECT open_time FROM candlesticks
         WHERE provider = ?1 AND symbol = ?2 AND timeframe = ?3
         ORDER BY open_time ASC",
    )?;

    let mut rows = stmt.query(params![provider, symbol, timeframe])?;

    let mut previous_time: Option<i64> = None;
    let mut gaps: Vec<(i64, i64, i64)> = Vec::new(); // (timestamp, interval, expected)
    let mut overlaps: Vec<(i64, i64)> = Vec::new(); // (timestamp, interval)
    let mut total_count = 0;
    let mut first_timestamp: Option<i64> = None;
    let mut last_timestamp: Option<i64> = None;

    while let Some(row) = rows.next()? {
        let current_time: i64 = row.get(0)?;

        if first_timestamp.is_none() {
            first_timestamp = Some(current_time);
        }
        last_timestamp = Some(current_time);

        if let Some(prev) = previous_time {
            let interval = current_time - prev;

            // Vérifier si l'intervalle est différent de l'attendu
            if interval != expected_interval_ms {
                if interval > expected_interval_ms {
                    // Gap (trou dans les données)
                    gaps.push((prev, interval, expected_interval_ms));
                } else if interval < expected_interval_ms {
                    // Overlap ou duplication
                    overlaps.push((prev, interval));
                }
            }
        }

        previous_time = Some(current_time);
        total_count += 1;
    }

    // Afficher les résultats
    println!("\n--- Statistiques ---");
    println!("Nombre total de bougies: {}", total_count);

    if let (Some(first), Some(last)) = (first_timestamp, last_timestamp) {
        println!("Première bougie: {}", format_timestamp_ms(first));
        println!("Dernière bougie: {}", format_timestamp_ms(last));

        let duration_ms = last - first;
        let expected_count = (duration_ms / expected_interval_ms) + 1;
        println!("Nombre de bougies attendu: {}", expected_count);
        println!("Différence: {}", total_count as i64 - expected_count);
    }

    // Afficher les gaps (trous)
    if !gaps.is_empty() {
        println!("\n--- GAPS DÉTECTÉS ({} gaps) ---", gaps.len());
        for (i, (timestamp, interval, expected)) in gaps.iter().enumerate() {
            if i < 10 {
                // Limiter l'affichage aux 10 premiers
                let missing_candles = (interval / expected) - 1;
                println!(
                    "  Gap à {}: intervalle de {} ms ({} bougies manquantes)",
                    format_timestamp_ms(*timestamp),
                    interval,
                    missing_candles
                );
            }
        }
        if gaps.len() > 10 {
            println!("  ... et {} autres gaps", gaps.len() - 10);
        }
    } else {
        println!("\n✓ Aucun gap détecté - les données sont continues!");
    }

    // Afficher les overlaps (chevauchements)
    if !overlaps.is_empty() {
        println!("\n--- OVERLAPS DÉTECTÉS ({} overlaps) ---", overlaps.len());
        for (i, (timestamp, interval)) in overlaps.iter().enumerate() {
            if i < 10 {
                println!(
                    "  Overlap à {}: intervalle de {} ms (attendu {} ms)",
                    format_timestamp_ms(*timestamp),
                    interval,
                    expected_interval_ms
                );
            }
        }
        if overlaps.len() > 10 {
            println!("  ... et {} autres overlaps", overlaps.len() - 10);
        }
    } else {
        println!("✓ Aucun overlap détecté - les espacements sont corrects!");
    }

    println!("\n{:=<60}\n", "");

    Ok(())
}

/// Fonction utilitaire pour afficher les timestamps
fn format_timestamp_ms(timestamp_ms: i64) -> String {
    // Crée un DateTime à partir du timestamp Unix en millisecondes
    if let Some(datetime_utc) = DateTime::<Utc>::from_timestamp_millis(timestamp_ms) {
        // Formate la date et l'heure
        datetime_utc.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        "Invalid timestamp".to_string()
    }
}
