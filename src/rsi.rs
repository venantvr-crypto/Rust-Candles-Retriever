/// Module de calcul RSI (Relative Strength Index)
///
/// Fournit des fonctions pour calculer et stocker les valeurs RSI en base de données

use anyhow::Result;
use rusqlite::{Connection, params};

/// Calcule le RSI pour une série de prix
///
/// ALGORITHME:
/// - Premier RSI: moyenne simple sur `period` valeurs
/// - RSI suivants: moyenne mobile exponentielle (EMA)
///
/// RETOUR: Vec<Option<f64>> avec None pour les valeurs avant `period`
pub fn calculate_rsi(closes: &[f64], period: usize) -> Vec<Option<f64>> {
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

/// Recalcule le RSI pour un symbole/timeframe/période donnés sur un intervalle de temps
///
/// USAGE: Appelé après insertion de nouvelles bougies pour mettre à jour le RSI
///
/// ALGORITHME:
/// 1. Charge toutes les bougies dans [start_time, end_time]
/// 2. Calcule le RSI pour cette plage
/// 3. INSERT OR REPLACE dans rsi_values
///
/// RETOUR: Nombre de valeurs RSI insérées
pub fn recalculate_rsi_for_range(
    conn: &mut Connection,
    provider: &str,
    symbol: &str,
    timeframe: &str,
    period: i64,
    start_time: i64,
    end_time: i64,
) -> Result<i64> {
    // Charger les bougies pour la plage donnée
    let (times, closes) = {
        let mut stmt = conn.prepare(
            "SELECT open_time, close FROM candlesticks
             WHERE provider = ?1 AND symbol = ?2 AND timeframe = ?3
             AND open_time >= ?4 AND open_time <= ?5
             ORDER BY open_time ASC"
        )?;

        let mut times = Vec::new();
        let mut closes = Vec::new();

        let rows = stmt.query_map(
            params![provider, symbol, timeframe, start_time, end_time],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)?))
        )?;

        for row_result in rows {
            let (time, close) = row_result?;
            times.push(time);
            closes.push(close);
        }

        (times, closes)
    };

    if closes.len() < period as usize + 1 {
        println!("   ⚠️  Not enough data for RSI: {} candles (need > {})", closes.len(), period);
        return Ok(0);
    }

    // Calculer RSI
    let rsi_values = calculate_rsi(&closes, period as usize);

    // Insérer dans la BDD
    let tx = conn.transaction()?;
    let mut count = 0i64;

    {
        let mut insert_stmt = tx.prepare(
            "INSERT OR REPLACE INTO rsi_values
             (provider, symbol, timeframe, period, open_time, rsi_value)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
        )?;

        for (i, rsi) in rsi_values.iter().enumerate() {
            if let Some(rsi_val) = rsi {
                insert_stmt.execute(params![
                    provider,
                    symbol,
                    timeframe,
                    period,
                    times[i],
                    rsi_val
                ])?;
                count += 1;
            }
        }
    }

    tx.commit()?;
    println!("   ✅ RSI recalculated: {} values for {}/{} {} (period {})",
             count, provider, symbol, timeframe, period);

    Ok(count)
}

/// Recalcule le RSI pour tous les timeframes d'un symbole sur un intervalle
///
/// USAGE: Appelé après fetch pour mettre à jour tous les RSI
pub fn recalculate_all_timeframes(
    conn: &mut Connection,
    provider: &str,
    symbol: &str,
    timeframe: &str,
    period: i64,
    start_time: i64,
    end_time: i64,
) -> Result<()> {
    println!("🔄 Recalculating RSI for {}/{} {} in range...", provider, symbol, timeframe);

    recalculate_rsi_for_range(
        conn,
        provider,
        symbol,
        timeframe,
        period,
        start_time,
        end_time,
    )?;

    Ok(())
}
