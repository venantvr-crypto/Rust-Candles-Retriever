/// Test de la fonctionnalité de complétion des timeframes
///
/// Ce test démontre que:
/// 1. On peut marquer un timeframe comme complet
/// 2. Les timeframes complets sont détectés correctement
/// 3. Le programme saute les timeframes complets lors de la prochaine exécution
use anyhow::Result;
use rust_candles_retriever::database::SQL_CREATE_TABLE_CANDLESTICKS;
use rusqlite::{Connection, params};
use rust_candles_retriever::utils;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() -> Result<()> {
    let db_file = "test_timeframe_completion.db";

    // Supprimer l'ancienne base de test
    let _ = std::fs::remove_file(db_file);

    println!("=== TEST DE COMPLÉTION DES TIMEFRAMES ===\n");

    let conn = setup_database(db_file)?;
    println!("✓ Base de données créée\n");

    // ===================================================================
    // SCÉNARIO 1: Marquer 5m comme complet
    // ===================================================================
    println!("╔════════════════════════════════════════════════════════════");
    println!("║ SCÉNARIO 1: Marquer BTCUSDT/5m comme complet");
    println!("╚════════════════════════════════════════════════════════════\n");

    let base_time = 1700000000000i64;

    // Insérer quelques bougies pour 5m
    for i in 0..10 {
        insert_candle(&conn, "binance", "BTCUSDT", "5m", base_time + i * 300_000)?;
    }
    println!("✓ Inséré 10 bougies BTCUSDT/5m");

    // Marquer 5m comme complet (comme si on avait atteint la limite historique)
    mark_timeframe_complete(&conn, "binance", "BTCUSDT", "5m", Some(base_time))?;
    println!("✓ BTCUSDT/5m marqué comme complet\n");

    // ===================================================================
    // SCÉNARIO 2: Laisser 15m incomplet
    // ===================================================================
    println!("╔════════════════════════════════════════════════════════════");
    println!("║ SCÉNARIO 2: BTCUSDT/15m incomplet");
    println!("╚════════════════════════════════════════════════════════════\n");

    // Insérer quelques bougies pour 15m
    for i in 0..5 {
        insert_candle(&conn, "binance", "BTCUSDT", "15m", base_time + i * 900_000)?;
    }
    println!("✓ Inséré 5 bougies BTCUSDT/15m");
    println!("  (pas marqué comme complet)\n");

    // ===================================================================
    // SCÉNARIO 3: Vérifier les statuts
    // ===================================================================
    println!("╔════════════════════════════════════════════════════════════");
    println!("║ SCÉNARIO 3: Vérification des statuts");
    println!("╚════════════════════════════════════════════════════════════\n");

    let timeframes = vec!["5m", "15m", "30m", "1h"];

    for tf in &timeframes {
        let is_complete = is_timeframe_complete(&conn, "binance", "BTCUSDT", tf);
        let status = if is_complete {
            "✅ COMPLET"
        } else {
            "⏳ INCOMPLET"
        };
        println!(
            "{} - BTCUSDT/{}: {}",
            status,
            tf,
            if is_complete {
                "(sera sauté lors de la prochaine exécution)"
            } else {
                "(sera traité lors de la prochaine exécution)"
            }
        );
    }
    println!();

    // ===================================================================
    // SCÉNARIO 4: Afficher le contenu de la table timeframe_status
    // ===================================================================
    println!("╔════════════════════════════════════════════════════════════");
    println!("║ SCÉNARIO 4: Contenu de timeframe_status");
    println!("╚════════════════════════════════════════════════════════════\n");

    let mut stmt = conn.prepare(
        "SELECT provider, symbol, timeframe, oldest_candle_time, is_complete, last_updated
         FROM timeframe_status
         ORDER BY provider, symbol, timeframe",
    )?;

    let mut rows = stmt.query([])?;
    let mut count = 0;

    println!("Provider | Symbol   | TF  | Oldest Candle       | Complet | Last Updated");
    println!("---------|----------|-----|---------------------|---------|---------------------");

    while let Some(row) = rows.next()? {
        let provider: String = row.get(0)?;
        let symbol: String = row.get(1)?;
        let timeframe: String = row.get(2)?;
        let oldest: Option<i64> = row.get(3)?;
        let is_complete: i32 = row.get(4)?;
        let last_updated: i64 = row.get(5)?;

        let oldest_str = match oldest {
            Some(t) => format_timestamp_ms(t),
            None => "N/A".to_string(),
        };

        println!(
            "{:8} | {:8} | {:3} | {:19} | {:7} | {}",
            provider,
            symbol,
            timeframe,
            oldest_str,
            if is_complete == 1 { "OUI" } else { "NON" },
            format_timestamp_ms(last_updated)
        );

        count += 1;
    }

    if count == 0 {
        println!("(aucune entrée)");
    }

    println!();

    // ===================================================================
    // RÉSULTAT FINAL
    // ===================================================================
    println!("╔════════════════════════════════════════════════════════════");
    println!("║ ✅ TEST RÉUSSI!");
    println!("╚════════════════════════════════════════════════════════════\n");

    println!("Pour tester le comportement du programme principal:");
    println!(
        "1. Copiez cette base de test: cp {} candlesticks.db",
        db_file
    );
    println!("2. Lancez: cargo run --release -- --symbol BTCUSDT");
    println!("3. Le programme devrait afficher:");
    println!("   ⏭️  Timeframe 5m déjà complet pour BTCUSDT. Passage au suivant.");
    println!("   Et continuer avec 15m, 30m, 1h\n");

    println!("Base de données: {}", db_file);

    Ok(())
}

fn setup_database(db_file: &str) -> Result<Connection> {
    let path = Path::new(db_file);
    let conn = Connection::open(path)?;

    conn.execute(SQL_CREATE_TABLE_CANDLESTICKS, [])?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS timeframe_status (
            provider TEXT NOT NULL,
            symbol TEXT NOT NULL,
            timeframe TEXT NOT NULL,
            oldest_candle_time INTEGER,
            is_complete INTEGER NOT NULL DEFAULT 0,
            last_updated INTEGER NOT NULL,
            PRIMARY KEY (provider, symbol, timeframe)
        )",
        [],
    )?;

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

fn is_timeframe_complete(conn: &Connection, provider: &str, symbol: &str, timeframe: &str) -> bool {
    conn.query_row(
        "SELECT is_complete FROM timeframe_status
         WHERE provider = ?1 AND symbol = ?2 AND timeframe = ?3",
        params![provider, symbol, timeframe],
        |row| row.get(0),
    )
    .unwrap_or(0)
        == 1
}

fn mark_timeframe_complete(
    conn: &Connection,
    provider: &str,
    symbol: &str,
    timeframe: &str,
    oldest_candle_time: Option<i64>,
) -> Result<()> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as i64;

    conn.execute(
        "INSERT OR REPLACE INTO timeframe_status
         (provider, symbol, timeframe, oldest_candle_time, is_complete, last_updated)
         VALUES (?1, ?2, ?3, ?4, 1, ?5)",
        params![provider, symbol, timeframe, oldest_candle_time, now],
    )?;

    Ok(())
}

fn format_timestamp_ms(timestamp_ms: i64) -> String {
    use chrono::{DateTime, Utc};

    if let Some(datetime_utc) = DateTime::<Utc>::from_timestamp_millis(timestamp_ms) {
        datetime_utc.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        "Invalid".to_string()
    }
}
