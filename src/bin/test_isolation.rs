/// Test d'isolation du mode de reprise par (provider, symbol, timeframe)
///
/// Ce test démontre que chaque combinaison (provider, symbol, timeframe)
/// garde sa propre dernière bougie indépendamment des autres
use anyhow::Result;
use rusqlite::{Connection, params};
use rust_candles_retriever::database::SQL_CREATE_TABLE_CANDLESTICKS;
use rust_candles_retriever::utils;
use std::path::Path;

fn main() -> Result<()> {
    let db_file = "test_isolation.db";

    // Supprimer l'ancienne base de test
    let _ = std::fs::remove_file(db_file);

    println!("=== TEST D'ISOLATION PAR (PROVIDER, SYMBOL, TIMEFRAME) ===\n");

    let conn = setup_database(db_file)?;
    println!("✓ Base de données créée\n");

    // ===================================================================
    // Insérer des données pour différentes combinaisons
    // ===================================================================
    println!("Insertion de données pour différentes combinaisons...\n");

    let base_time = 1700000000000i64;
    let interval_5m = 300_000i64;
    let interval_15m = 900_000i64;

    // BTCUSDT 5m → dernière bougie à base_time + 10 * interval_5m
    for i in 0..=10 {
        insert_candle(
            &conn,
            "binance",
            "BTCUSDT",
            "5m",
            base_time + i * interval_5m,
        )?;
    }
    println!("✓ BTCUSDT/5m: 11 bougies insérées (0 à 10)");

    // BTCUSDT 15m → dernière bougie à base_time + 5 * interval_15m
    for i in 0..=5 {
        insert_candle(
            &conn,
            "binance",
            "BTCUSDT",
            "15m",
            base_time + i * interval_15m,
        )?;
    }
    println!("✓ BTCUSDT/15m: 6 bougies insérées (0 à 5)");

    // ETHUSDT 5m → dernière bougie à base_time + 7 * interval_5m
    for i in 0..=7 {
        insert_candle(
            &conn,
            "binance",
            "ETHUSDT",
            "5m",
            base_time + i * interval_5m,
        )?;
    }
    println!("✓ ETHUSDT/5m: 8 bougies insérées (0 à 7)");

    // ETHUSDT 15m → dernière bougie à base_time + 3 * interval_15m
    for i in 0..=3 {
        insert_candle(
            &conn,
            "binance",
            "ETHUSDT",
            "15m",
            base_time + i * interval_15m,
        )?;
    }
    println!("✓ ETHUSDT/15m: 4 bougies insérées (0 à 3)");

    println!();

    // ===================================================================
    // Vérifier l'isolation: chaque combinaison doit avoir sa propre dernière bougie
    // ===================================================================
    println!("╔════════════════════════════════════════════════════════════");
    println!("║ VÉRIFICATION DE L'ISOLATION");
    println!("╚════════════════════════════════════════════════════════════\n");

    let tests = vec![
        (
            "binance",
            "BTCUSDT",
            "5m",
            base_time + 10 * interval_5m,
            "10ème bougie",
        ),
        (
            "binance",
            "BTCUSDT",
            "15m",
            base_time + 5 * interval_15m,
            "5ème bougie",
        ),
        (
            "binance",
            "ETHUSDT",
            "5m",
            base_time + 7 * interval_5m,
            "7ème bougie",
        ),
        (
            "binance",
            "ETHUSDT",
            "15m",
            base_time + 3 * interval_15m,
            "3ème bougie",
        ),
        (
            "binance",
            "BTCUSDT",
            "1h",
            0,
            "Aucune donnée (None attendu)",
        ),
        (
            "binance",
            "SOLUSDT",
            "5m",
            0,
            "Aucune donnée (None attendu)",
        ),
    ];

    let mut all_passed = true;

    for (provider, symbol, timeframe, expected_time, description) in tests {
        let last_time = get_last_candle_time(&conn, provider, symbol, timeframe);

        let passed = if expected_time == 0 {
            // On attend None
            last_time.is_none()
        } else {
            // On attend Some(expected_time)
            last_time == Some(expected_time)
        };

        let status = if passed { "✓" } else { "✗" };

        println!("{} {}/{}/{}", status, provider, symbol, timeframe);

        match last_time {
            Some(t) => {
                println!("   Dernière bougie: {}", format_timestamp_ms(t));
                println!("   Timestamp: {} ms", t);
                if expected_time != 0 {
                    if t == expected_time {
                        println!("   ✓ Correspond à l'attendu ({})", description);
                    } else {
                        println!("   ✗ ERREUR: attendu {}, trouvé {}", expected_time, t);
                        all_passed = false;
                    }
                } else {
                    println!("   ✗ ERREUR: attendu None, trouvé Some({})", t);
                    all_passed = false;
                }
            }
            None => {
                println!("   Aucune donnée");
                if expected_time == 0 {
                    println!("   ✓ Correspond à l'attendu ({})", description);
                } else {
                    println!("   ✗ ERREUR: attendu Some({}), trouvé None", expected_time);
                    all_passed = false;
                }
            }
        }
        println!();
    }

    // ===================================================================
    // Résultat final
    // ===================================================================
    if all_passed {
        println!("╔════════════════════════════════════════════════════════════");
        println!("║ ✅ TOUS LES TESTS RÉUSSIS!");
        println!("╚════════════════════════════════════════════════════════════");
        println!();
        println!("✓ Chaque combinaison (provider, symbol, timeframe) est bien isolée");
        println!("✓ La fonction get_last_candle_time() respecte les 3 critères");
        println!("✓ Le mode de reprise fonctionne indépendamment pour chaque timeframe");
    } else {
        println!("╔════════════════════════════════════════════════════════════");
        println!("║ ✗ ÉCHEC: Certains tests ont échoué");
        println!("╚════════════════════════════════════════════════════════════");
    }

    println!("\nBase de données: {}", db_file);
    println!(
        "Inspecter avec: sqlite3 {} \"SELECT * FROM candlesticks ORDER BY provider, symbol, timeframe, open_time\"",
        db_file
    );

    Ok(())
}

fn setup_database(db_file: &str) -> Result<Connection> {
    let path = Path::new(db_file);
    let conn = Connection::open(path)?;

    conn.execute(SQL_CREATE_TABLE_CANDLESTICKS, [])?;

    Ok(conn)
}

fn insert_candle(
    conn: &Connection,
    provider: &str,
    symbol: &str,
    timeframe: &str,
    open_time: i64,
) -> Result<()> {
    let interval = utils::timeframe_to_interval(timeframe);

    conn.execute(
        "INSERT INTO candlesticks (
            provider, symbol, timeframe, open_time, open, high, low, close, volume,
            close_time, quote_asset_volume, number_of_trades,
            taker_buy_base_asset_volume, taker_buy_quote_asset_volume, interpolated
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            provider,
            symbol,
            timeframe,
            open_time,
            50000.0,
            50500.0,
            49500.0,
            50200.0,
            100.0,
            open_time + interval - 1,
            5000000.0,
            1000,
            50.0,
            2500000.0,
            0,
        ],
    )?;

    Ok(())
}

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
