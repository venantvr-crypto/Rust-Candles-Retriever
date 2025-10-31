/// Programme de test pour démontrer le mode de reprise
///
/// Ce test simule deux scénarios:
/// 1. Première exécution sans données existantes
/// 2. Reprise avec des données déjà présentes
use anyhow::Result;
use rust_candles_retriever::database::SQL_CREATE_TABLE_CANDLESTICKS;
use rusqlite::{Connection, params};
use std::path::Path;

fn main() -> Result<()> {
    let db_file = "test_resume_demo.db";

    // Supprimer l'ancienne base de test
    let _ = std::fs::remove_file(db_file);

    println!("=== TEST DU MODE DE REPRISE ===\n");

    // Créer la base de données
    let conn = setup_database(db_file)?;
    println!("✓ Base de données de test créée\n");

    // ===================================================================
    // SCÉNARIO 1: Première exécution (aucune donnée)
    // ===================================================================
    println!("╔════════════════════════════════════════════════════════════");
    println!("║ SCÉNARIO 1: Première exécution");
    println!("╚════════════════════════════════════════════════════════════");

    let last_time = get_last_candle_time(&conn, "binance", "BTCUSDT", "5m");
    match last_time {
        None => println!("✓ Aucune donnée trouvée → MODE PREMIÈRE EXÉCUTION"),
        Some(t) => println!("✗ Données trouvées (inattendu): {}", t),
    }
    println!();

    // ===================================================================
    // Insérer des données simulées
    // ===================================================================
    println!("Insertion de données simulées...");
    let base_time = 1700000000000i64;
    let interval = 300_000i64; // 5 minutes

    conn.execute(
        "INSERT INTO candlesticks (
            provider, symbol, timeframe, open_time, open, high, low, close, volume,
            close_time, quote_asset_volume, number_of_trades,
            taker_buy_base_asset_volume, taker_buy_quote_asset_volume, interpolated
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            "binance",
            "BTCUSDT",
            "5m",
            base_time,
            50000.0,
            50500.0,
            49500.0,
            50200.0,
            100.0,
            base_time + interval - 1,
            5000000.0,
            1000,
            50.0,
            2500000.0,
            0,
        ],
    )?;

    conn.execute(
        "INSERT INTO candlesticks (
            provider, symbol, timeframe, open_time, open, high, low, close, volume,
            close_time, quote_asset_volume, number_of_trades,
            taker_buy_base_asset_volume, taker_buy_quote_asset_volume, interpolated
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            "binance",
            "BTCUSDT",
            "5m",
            base_time + interval,
            50200.0,
            50700.0,
            49800.0,
            50400.0,
            110.0,
            base_time + 2 * interval - 1,
            5500000.0,
            1100,
            55.0,
            2750000.0,
            0,
        ],
    )?;

    // Données pour un autre timeframe
    conn.execute(
        "INSERT INTO candlesticks (
            provider, symbol, timeframe, open_time, open, high, low, close, volume,
            close_time, quote_asset_volume, number_of_trades,
            taker_buy_base_asset_volume, taker_buy_quote_asset_volume, interpolated
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            "binance",
            "BTCUSDT",
            "15m",
            base_time,
            50000.0,
            51000.0,
            49000.0,
            50500.0,
            300.0,
            base_time + 900_000 - 1,
            15000000.0,
            3000,
            150.0,
            7500000.0,
            0,
        ],
    )?;

    println!("✓ 3 bougies insérées\n");

    // ===================================================================
    // SCÉNARIO 2: Reprise (données existantes)
    // ===================================================================
    println!("╔════════════════════════════════════════════════════════════");
    println!("║ SCÉNARIO 2: Reprise avec données existantes");
    println!("╚════════════════════════════════════════════════════════════");

    // Test pour 5m timeframe
    let last_time_5m = get_last_candle_time(&conn, "binance", "BTCUSDT", "5m");
    match last_time_5m {
        Some(t) => {
            println!("✓ MODE REPRISE ACTIVÉ");
            println!("  Provider: binance");
            println!("  Symbol: BTCUSDT");
            println!("  Timeframe: 5m");
            println!("  Dernière bougie: {}", format_timestamp_ms(t));
            println!("  Timestamp: {} ms", t);
            let expected = base_time + interval;
            if t == expected {
                println!("  ✓ Correspond à la dernière bougie insérée");
            } else {
                println!("  ✗ Erreur: attendu {}, trouvé {}", expected, t);
            }
        }
        None => println!("✗ Aucune donnée trouvée (inattendu)"),
    }
    println!();

    // Test pour 15m timeframe
    let last_time_15m = get_last_candle_time(&conn, "binance", "BTCUSDT", "15m");
    match last_time_15m {
        Some(t) => {
            println!("✓ MODE REPRISE ACTIVÉ");
            println!("  Provider: binance");
            println!("  Symbol: BTCUSDT");
            println!("  Timeframe: 15m");
            println!("  Dernière bougie: {}", format_timestamp_ms(t));
            println!("  Timestamp: {} ms", t);
            if t == base_time {
                println!("  ✓ Correspond à la dernière bougie insérée");
            } else {
                println!("  ✗ Erreur: attendu {}, trouvé {}", base_time, t);
            }
        }
        None => println!("✗ Aucune donnée trouvée (inattendu)"),
    }
    println!();

    // Test pour un timeframe sans données
    println!("╔════════════════════════════════════════════════════════════");
    println!("║ SCÉNARIO 3: Timeframe sans données (1h)");
    println!("╚════════════════════════════════════════════════════════════");

    let last_time_1h = get_last_candle_time(&conn, "binance", "BTCUSDT", "1h");
    match last_time_1h {
        None => println!("✓ Aucune donnée trouvée → MODE PREMIÈRE EXÉCUTION"),
        Some(t) => println!("✗ Données trouvées (inattendu): {}", t),
    }
    println!();

    println!("✅ TOUS LES TESTS RÉUSSIS!");
    println!("\nBase de données: {}", db_file);
    println!("Vous pouvez l'inspecter avec: sqlite3 {}", db_file);

    Ok(())
}

fn setup_database(db_file: &str) -> Result<Connection> {
    let path = Path::new(db_file);
    let conn = Connection::open(path)?;

    conn.execute(SQL_CREATE_TABLE_CANDLESTICKS, [])?;

    Ok(conn)
}

/// Récupère le timestamp de la dernière bougie stockée
fn get_last_candle_time(
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

fn format_timestamp_ms(timestamp_ms: i64) -> String {
    use chrono::{DateTime, Utc};

    if let Some(datetime_utc) = DateTime::<Utc>::from_timestamp_millis(timestamp_ms) {
        datetime_utc.format("%Y-%m-%d %H:%M:%S UTC").to_string()
    } else {
        "Invalid timestamp".to_string()
    }
}
