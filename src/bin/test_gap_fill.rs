/// Programme de test pour démontrer le comblement de trous avec interpolation
use anyhow::Result;
use rusqlite::{Connection, params};
use std::path::Path;

fn main() -> Result<()> {
    let db_file = "test_gaps.db";

    // Supprimer l'ancienne base de test
    let _ = std::fs::remove_file(db_file);

    println!("=== TEST D'INTERPOLATION DE GAPS ===\n");

    // Créer la base de données
    let mut conn = setup_database(db_file)?;
    println!("✓ Base de données de test créée\n");

    // Insérer des données avec des trous intentionnels
    println!("Insertion de données avec trous intentionnels...");
    insert_test_data_with_gaps(&mut conn)?;
    println!("✓ Données insérées\n");

    // Afficher l'état avant interpolation
    println!("=== AVANT INTERPOLATION ===");
    show_data_stats(&conn)?;

    // Combler les trous
    println!("\n=== COMBLEMENT DES TROUS ===");
    let filled = fill_gaps(&mut conn)?;
    println!("✓ {} bougies interpolées\n", filled);

    // Afficher l'état après interpolation
    println!("=== APRÈS INTERPOLATION ===");
    show_data_stats(&conn)?;

    println!("\n✓ Test terminé! Base de données: {}", db_file);
    println!("  Vous pouvez inspecter la base avec: sqlite3 {}", db_file);

    Ok(())
}

fn setup_database(db_file: &str) -> Result<Connection> {
    let path = Path::new(db_file);
    let conn = Connection::open(path)?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS candlesticks (
            provider TEXT NOT NULL,
            symbol TEXT NOT NULL,
            timeframe TEXT NOT NULL,
            open_time INTEGER NOT NULL,
            open REAL NOT NULL,
            high REAL NOT NULL,
            low REAL NOT NULL,
            close REAL NOT NULL,
            volume REAL NOT NULL,
            close_time INTEGER NOT NULL,
            quote_asset_volume REAL NOT NULL,
            number_of_trades INTEGER NOT NULL,
            taker_buy_base_asset_volume REAL NOT NULL,
            taker_buy_quote_asset_volume REAL NOT NULL,
            interpolated INTEGER NOT NULL DEFAULT 0,
            UNIQUE(provider, symbol, timeframe, open_time)
        )",
        [],
    )?;

    Ok(conn)
}

fn insert_test_data_with_gaps(conn: &mut Connection) -> Result<()> {
    let base_time = 1700000000000i64; // Timestamp de référence
    let interval = 300_000i64; // 5 minutes

    // Insérer des bougies avec des gaps intentionnels
    let candles = vec![
        // Groupe 1: bougies 0-4 (continues)
        (0, 100.0, 105.0, 95.0, 102.0, 1000.0),
        (1, 102.0, 107.0, 97.0, 104.0, 1100.0),
        (2, 104.0, 109.0, 99.0, 106.0, 1200.0),
        (3, 106.0, 111.0, 101.0, 108.0, 1300.0),
        (4, 108.0, 113.0, 103.0, 110.0, 1400.0),
        // GAP de 5 bougies ici (indices 5-9 manquants)
        // Groupe 2: bougies 10-12 (continues)
        (10, 130.0, 135.0, 125.0, 132.0, 2000.0),
        (11, 132.0, 137.0, 127.0, 134.0, 2100.0),
        (12, 134.0, 139.0, 129.0, 136.0, 2200.0),
        // GAP de 3 bougies ici (indices 13-15 manquants)
        // Groupe 3: bougies 16-18 (continues)
        (16, 150.0, 155.0, 145.0, 152.0, 2600.0),
        (17, 152.0, 157.0, 147.0, 154.0, 2700.0),
        (18, 154.0, 159.0, 149.0, 156.0, 2800.0),
    ];

    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO candlesticks (
                provider, symbol, timeframe, open_time, open, high, low, close, volume,
                close_time, quote_asset_volume, number_of_trades,
                taker_buy_base_asset_volume, taker_buy_quote_asset_volume, interpolated
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        )?;

        for (index, open, high, low, close, volume) in candles {
            let open_time = base_time + (index * interval);
            let close_time = open_time + interval - 1;

            stmt.execute(params![
                "test_provider",
                "TEST",
                "5m",
                open_time,
                open,
                high,
                low,
                close,
                volume,
                close_time,
                volume * 100.0,         // quote_asset_volume
                (volume / 10.0) as i64, // number_of_trades
                volume * 0.4,           // taker_buy_base
                volume * 40.0,          // taker_buy_quote
                0,                      // interpolated = 0 (données réelles)
            ])?;
        }
    }
    tx.commit()?;

    Ok(())
}

fn show_data_stats(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT open_time, open, close, volume FROM candlesticks
         WHERE provider = 'test_provider' AND symbol = 'TEST' AND timeframe = '5m'
         ORDER BY open_time ASC",
    )?;

    let mut rows = stmt.query([])?;
    let mut count = 0;
    let mut prev_time: Option<i64> = None;
    let mut gaps = 0;

    println!("Timestamp           | Open    | Close   | Volume");
    println!("--------------------|---------|---------|--------");

    while let Some(row) = rows.next()? {
        let open_time: i64 = row.get(0)?;
        let open: f64 = row.get(1)?;
        let close: f64 = row.get(2)?;
        let volume: f64 = row.get(3)?;

        if let Some(prev) = prev_time {
            let diff = open_time - prev;
            if diff > 300_000 {
                gaps += 1;
                println!("       *** GAP ***       |         |         |");
            }
        }

        println!(
            "{:19} | {:7.1} | {:7.1} | {:6.0}",
            open_time, open, close, volume
        );

        prev_time = Some(open_time);
        count += 1;
    }

    println!("\nTotal: {} bougies, {} gaps détectés", count, gaps);

    Ok(())
}

fn fill_gaps(conn: &mut Connection) -> Result<i64> {
    let interval = 300_000i64; // 5 minutes

    // Récupérer toutes les bougies
    let mut stmt = conn.prepare(
        "SELECT open_time, open, high, low, close, volume, close_time,
                quote_asset_volume, number_of_trades,
                taker_buy_base_asset_volume, taker_buy_quote_asset_volume
         FROM candlesticks
         WHERE provider = 'test_provider' AND symbol = 'TEST' AND timeframe = '5m'
         ORDER BY open_time ASC",
    )?;

    #[derive(Debug)]
    struct Candle {
        open_time: i64,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
        close_time: i64,
        quote_asset_volume: f64,
        number_of_trades: i64,
        taker_buy_base_asset_volume: f64,
        taker_buy_quote_asset_volume: f64,
    }

    let mut candles: Vec<Candle> = Vec::new();
    let mut rows = stmt.query([])?;

    while let Some(row) = rows.next()? {
        candles.push(Candle {
            open_time: row.get(0)?,
            open: row.get(1)?,
            high: row.get(2)?,
            low: row.get(3)?,
            close: row.get(4)?,
            volume: row.get(5)?,
            close_time: row.get(6)?,
            quote_asset_volume: row.get(7)?,
            number_of_trades: row.get(8)?,
            taker_buy_base_asset_volume: row.get(9)?,
            taker_buy_quote_asset_volume: row.get(10)?,
        });
    }
    drop(rows);
    drop(stmt);

    if candles.len() < 2 {
        return Ok(0);
    }

    let mut total_filled = 0i64;
    let tx = conn.transaction()?;

    {
        let mut insert_stmt = tx.prepare(
            "INSERT OR IGNORE INTO candlesticks (
                provider, symbol, timeframe, open_time, open, high, low, close, volume,
                close_time, quote_asset_volume, number_of_trades,
                taker_buy_base_asset_volume, taker_buy_quote_asset_volume, interpolated
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        )?;

        for i in 0..candles.len() - 1 {
            let current = &candles[i];
            let next = &candles[i + 1];

            let time_diff = next.open_time - current.open_time;

            if time_diff > interval {
                let missing_candles = (time_diff / interval) - 1;

                println!(
                    "Gap détecté: {} -> {} ({} bougies manquantes)",
                    current.open_time, next.open_time, missing_candles
                );

                for j in 1..=missing_candles {
                    let ratio = j as f64 / (missing_candles + 1) as f64;
                    let interpolated_time = current.open_time + (j * interval);

                    let interpolated_open = current.open + (next.open - current.open) * ratio;
                    let interpolated_high = current.high + (next.high - current.high) * ratio;
                    let interpolated_low = current.low + (next.low - current.low) * ratio;
                    let interpolated_close = current.close + (next.close - current.close) * ratio;
                    let interpolated_volume =
                        current.volume + (next.volume - current.volume) * ratio;
                    let interpolated_close_time = interpolated_time + interval - 1;
                    let interpolated_quote_volume = current.quote_asset_volume
                        + (next.quote_asset_volume - current.quote_asset_volume) * ratio;
                    let interpolated_trades = (current.number_of_trades as f64
                        + (next.number_of_trades as f64 - current.number_of_trades as f64) * ratio)
                        as i64;
                    let interpolated_taker_base = current.taker_buy_base_asset_volume
                        + (next.taker_buy_base_asset_volume - current.taker_buy_base_asset_volume)
                            * ratio;
                    let interpolated_taker_quote = current.taker_buy_quote_asset_volume
                        + (next.taker_buy_quote_asset_volume
                            - current.taker_buy_quote_asset_volume)
                            * ratio;

                    insert_stmt.execute(params![
                        "test_provider",
                        "TEST",
                        "5m",
                        interpolated_time,
                        interpolated_open,
                        interpolated_high,
                        interpolated_low,
                        interpolated_close,
                        interpolated_volume,
                        interpolated_close_time,
                        interpolated_quote_volume,
                        interpolated_trades,
                        interpolated_taker_base,
                        interpolated_taker_quote,
                        1, // interpolated = 1 (données interpolées)
                    ])?;

                    println!(
                        "  Interpolée #{}: time={}, open={:.1}, close={:.1}, volume={:.0}",
                        j,
                        interpolated_time,
                        interpolated_open,
                        interpolated_close,
                        interpolated_volume
                    );

                    total_filled += 1;
                }
            }
        }
    }

    tx.commit()?;
    Ok(total_filled)
}
