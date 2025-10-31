// ============================================================================
// MODULE DE VÉRIFICATION DE L'INTÉGRITÉ DES DONNÉES
// ============================================================================
//
// Ce module vérifie que les données stockées sont continues et correctement espacées
// Il détecte:
// - Les GAPS (trous): intervalles trop grands entre les bougies
// - Les OVERLAPS (chevauchements): intervalles trop petits ou négatifs
// - Les statistiques globales: nombre total, plage temporelle, etc.

use super::utils;
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};

/// Vérifie que les dates dans la base de données sont espacées de façon homogène
///
/// ALGORITHME DE VÉRIFICATION:
/// 1. Détermine l'intervalle attendu selon le timeframe
/// 2. Parcourt toutes les bougies séquentiellement
/// 3. Compare chaque intervalle avec l'intervalle attendu
/// 4. Classe les anomalies: gaps (intervalle trop grand) ou overlaps (trop petit)
/// 5. Calcule des statistiques: nombre de bougies, période couverte, etc.
/// 6. Affiche un rapport détaillé des anomalies trouvées
///
/// SUBTILITÉ RUST #17: pub fn
/// pub = fonction publique, accessible depuis d'autres modules
/// Sans pub, la fonction serait privée au module (visibility par défaut)
pub fn verify_data_spacing(
    conn: &Connection,
    provider: &str,
    symbol: &str,
    timeframe: &str,
) -> Result<()> {
    // Déterminer l'intervalle attendu en millisecondes selon le timeframe
    let expected_interval_ms = utils::timeframe_to_interval(timeframe);

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

    // SUBTILITÉ RUST #18: Accumulateurs mutables
    // Toutes ces variables sont déclarées mut car modifiées dans la boucle
    let mut previous_time: Option<i64> = None;

    // SUBTILITÉ RUST #19: Vec avec types tuples
    // Vec<(i64, i64, i64)> = vecteur de tuples à 3 éléments
    // Plus simple qu'une struct quand on n'a besoin que de stocker temporairement
    let mut gaps: Vec<(i64, i64, i64)> = Vec::new(); // (timestamp, interval, expected)
    let mut overlaps: Vec<(i64, i64)> = Vec::new(); // (timestamp, interval)
    let mut total_count = 0;
    let mut first_timestamp: Option<i64> = None;
    let mut last_timestamp: Option<i64> = None;

    // SUBTILITÉ RUST #20: while let - pattern matching dans une boucle
    // Équivalent à: loop { match rows.next()? { Some(row) => ..., None => break } }
    // Plus idiomatique et concis que la version avec loop/match
    while let Some(row) = rows.next()? {
        let current_time: i64 = row.get(0)?;

        // SUBTILITÉ RUST #21: Option::is_none()
        // Méthode helper pour tester si Option == None
        // Alternative: match first_timestamp { None => ..., Some(_) => ... }
        if first_timestamp.is_none() {
            first_timestamp = Some(current_time);
        }
        last_timestamp = Some(current_time);

        // ALGORITHME: Détection des anomalies par comparaison d'intervalles
        if let Some(prev) = previous_time {
            let interval = current_time - prev;

            // Trois cas possibles:
            // 1. interval == expected: OK
            // 2. interval > expected: GAP (données manquantes)
            // 3. interval < expected: OVERLAP (duplication ou erreur)
            if interval != expected_interval_ms {
                if interval > expected_interval_ms {
                    // Gap détecté - stocker pour rapport
                    gaps.push((prev, interval, expected_interval_ms));
                } else if interval < expected_interval_ms {
                    // Overlap détecté - stocker pour rapport
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
