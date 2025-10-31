/// Script pour calculer et stocker les RSI dans la BDD
///
/// Usage: cargo run --bin calculate_rsi [--period 14]

use anyhow::Result;
use rusqlite::{Connection, params};

/// Calcule le RSI pour une série de prix
fn calculate_rsi(closes: &[f64], period: usize) -> Vec<Option<f64>> {
    if closes.len() < period + 1 {
        return vec![None; closes.len()];
    }

    let mut results = vec![None; closes.len()];

    // Calculer les changements de prix
    let mut gains = Vec::new();
    let mut losses = Vec::new();

    for i in 1..closes.len() {
        let change = closes[i] - closes[i - 1];
        if change > 0.0 {
            gains.push(change);
            losses.push(0.0);
        } else {
            gains.push(0.0);
            losses.push(change.abs());
        }
    }

    if gains.len() < period {
        return results;
    }

    // Premier RSI: moyenne simple
    let mut avg_gain: f64 = gains[..period].iter().sum::<f64>() / period as f64;
    let mut avg_loss: f64 = losses[..period].iter().sum::<f64>() / period as f64;

    let rs = if avg_loss == 0.0 { 100.0 } else { avg_gain / avg_loss };
    results[period] = Some(100.0 - (100.0 / (1.0 + rs)));

    // RSI suivants: moyenne mobile exponentielle
    for i in period..gains.len() {
        avg_gain = (avg_gain * (period - 1) as f64 + gains[i]) / period as f64;
        avg_loss = (avg_loss * (period - 1) as f64 + losses[i]) / period as f64;

        let rs = if avg_loss == 0.0 { 100.0 } else { avg_gain / avg_loss };
        results[i + 1] = Some(100.0 - (100.0 / (1.0 + rs)));
    }

    results
}

fn main() -> Result<()> {
    let period: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(14);

    println!("🧮 RSI Calculator - Period: {}", period);
    println!("📁 Scanning database directory...");

    let db_dir = std::env::var("DB_DIR").unwrap_or_else(|_| ".".to_string());
    let entries = std::fs::read_dir(&db_dir)?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || !path.extension().map_or(false, |e| e == "db") {
            continue;
        }

        let symbol = path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("UNKNOWN")
            .to_string();

        println!("\n📊 Processing {}...", symbol);

        let mut conn = Connection::open(&path)?;

        // Vérifier si la table rsi_values existe, sinon la créer
        conn.execute(
            "CREATE TABLE IF NOT EXISTS rsi_values (
                provider TEXT NOT NULL,
                symbol TEXT NOT NULL,
                timeframe TEXT NOT NULL,
                period INTEGER NOT NULL,
                open_time INTEGER NOT NULL,
                rsi_value REAL NOT NULL,
                UNIQUE(provider, symbol, timeframe, period, open_time)
            )",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_rsi_query
                ON rsi_values (
                provider, symbol, timeframe, period, open_time
            )",
            [],
        )?;

        // Récupérer les timeframes disponibles
        let timeframes: Vec<String> = {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT timeframe FROM candlesticks WHERE provider = 'binance' AND symbol = ?1"
            )?;

            stmt.query_map(params![&symbol], |row| row.get(0))?
                .filter_map(Result::ok)
                .collect()
        }; // stmt est drop ici

        for tf in &timeframes {
            println!("  📈 Calculating RSI for {} {}...", symbol, tf);

            // Charger toutes les candles pour cette TF
            let (times, closes) = {
                let mut candles_stmt = conn.prepare(
                    "SELECT open_time, close FROM candlesticks
                     WHERE provider = 'binance' AND symbol = ?1 AND timeframe = ?2
                     ORDER BY open_time ASC"
                )?;

                let mut times = Vec::new();
                let mut closes = Vec::new();

                let rows = candles_stmt.query_map(params![&symbol, tf], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)?))
                })?;

                for row_result in rows {
                    let (time, close) = row_result?;
                    times.push(time);
                    closes.push(close);
                }

                (times, closes)
            }; // candles_stmt est drop ici

            if closes.len() < period + 1 {
                println!("    ⚠️  Not enough data: {} candles (need > {})", closes.len(), period);
                continue;
            }

            // Calculer RSI
            let rsi_values = calculate_rsi(&closes, period);

            // Insérer dans la BDD
            let tx = conn.transaction()?;
            {
                let mut insert_stmt = tx.prepare(
                    "INSERT OR REPLACE INTO rsi_values
                     (provider, symbol, timeframe, period, open_time, rsi_value)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
                )?;

                let mut count = 0;
                for (i, rsi) in rsi_values.iter().enumerate() {
                    if let Some(rsi_val) = rsi {
                        insert_stmt.execute(params![
                            "binance",
                            &symbol,
                            tf,
                            period as i64,
                            times[i],
                            rsi_val
                        ])?;
                        count += 1;
                    }
                }

                println!("    ✅ Inserted {} RSI values", count);
            }
            tx.commit()?;
        }
    }

    println!("\n✅ RSI calculation complete!");
    Ok(())
}
