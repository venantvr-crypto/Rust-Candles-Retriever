/// Module d'interpolation linéaire pour combler les trous dans les données
///
/// Ce module détecte les gaps (intervalles manquants) et génère des bougies
/// interpolées pour maintenir la continuité de la série temporelle
use anyhow::Result;
use rusqlite::{Connection, params};
use chrono::TimeZone;

/// Structure pour stocker temporairement une bougie
///
/// DESIGN: Struct simple sans méthodes, utilisée pour charger les données
/// depuis la DB avant de calculer les interpolations
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

/// Gestionnaire d'interpolation des gaps
///
/// ARCHITECTURE:
/// Fournit des méthodes pour détecter et combler les trous dans les données
pub struct GapFiller;

impl GapFiller {
    /// Comble les gaps dans une plage de temps donnée
    ///
    /// ALGORITHME D'INTERPOLATION:
    /// 1. Récupère toutes les bougies dans [start_time, end_time]
    /// 2. Parcourt paire par paire (fenêtre glissante)
    /// 3. Si intervalle > intervalle_attendu → GAP détecté
    /// 4. Calcule nombre de bougies manquantes
    /// 5. Pour chaque bougie manquante:
    ///    - Calcule ratio de position: i / (n+1)
    ///    - Interpole linéairement tous les champs
    /// 6. Insère avec INSERT OR IGNORE
    ///
    /// FORMULE: valeur = A + (B-A) × ratio
    ///
    /// RETOUR: Nombre de bougies interpolées

    /// Compte le nombre de gaps dans une plage sans les remplir
    ///
    /// ALGORITHME OPTIMISÉ:
    /// 1. Calcule le nombre théorique de bougies sur [start_time, end_time]
    /// 2. Compte le nombre réel de bougies en BDD
    /// 3. Différence = nombre de gaps
    ///
    /// RETOUR: Nombre de bougies manquantes (gaps)
    pub fn count_gaps_in_range(
        conn: &Connection,
        provider: &str,
        symbol: &str,
        timeframe: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<i64> {
        // Debug: afficher les timestamps en Europe/Paris
        let paris_tz = chrono_tz::Europe::Paris;
        let start_dt = paris_tz.timestamp_millis_opt(start_time).unwrap();
        let end_dt = paris_tz.timestamp_millis_opt(end_time).unwrap();
        println!("🔍 count_gaps_in_range: {}/{} {} → start_time={} ({}) end_time={} ({})",
                 provider, symbol, timeframe,
                 start_time, start_dt.format("%Y-%m-%d %H:%M:%S %Z"),
                 end_time, end_dt.format("%Y-%m-%d %H:%M:%S %Z"));

        let interval = Self::timeframe_to_interval(timeframe);

        // Calcul théorique: combien de bougies devrait-il y avoir ?
        // +1 car on inclut à la fois start_time et end_time
        let expected_count = ((end_time - start_time) / interval) + 1;

        // Compter les bougies réelles en BDD
        let actual_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM candlesticks
             WHERE provider = ?1 AND symbol = ?2 AND timeframe = ?3
             AND open_time >= ?4 AND open_time <= ?5",
            params![provider, symbol, timeframe, start_time, end_time],
            |row| row.get(0),
        )?;

        let gaps = expected_count - actual_count;

        println!("   → expected={} actual={} gaps={}", expected_count, actual_count, gaps);

        // Pas de gaps négatifs (si on a plus de bougies que prévu, c'est suspect mais pas un gap)
        Ok(gaps.max(0))
    }

    pub fn fill_gaps_in_range(
        conn: &mut Connection,
        provider: &str,
        symbol: &str,
        timeframe: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<i64> {
        let interval = Self::timeframe_to_interval(timeframe);

        // Récupérer toutes les bougies existantes dans la plage
        let candles =
            Self::fetch_candles_in_range(conn, provider, symbol, timeframe, start_time, end_time)?;

        // Pas de gaps possibles si moins de 2 bougies (besoin de 2 pour comparer l'intervalle)
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

            // Fenêtre glissante: parcourir paires de bougies consécutives
            for i in 0..candles.len() - 1 {
                let current = &candles[i];
                let next = &candles[i + 1];

                let time_diff = next.open_time - current.open_time;

                if time_diff > interval {
                    let missing_candles = (time_diff / interval) - 1;

                    // Interpoler chaque bougie manquante
                    for j in 1..=missing_candles {
                        let ratio = j as f64 / (missing_candles + 1) as f64;
                        let interpolated = Self::interpolate_candle(current, next, ratio, interval);

                        insert_stmt.execute(params![
                            provider,
                            symbol,
                            timeframe,
                            interpolated.open_time,
                            interpolated.open,
                            interpolated.high,
                            interpolated.low,
                            interpolated.close,
                            interpolated.volume,
                            interpolated.close_time,
                            interpolated.quote_asset_volume,
                            interpolated.number_of_trades,
                            interpolated.taker_buy_base_asset_volume,
                            interpolated.taker_buy_quote_asset_volume,
                            1, // interpolated = 1 (données synthétiques)
                        ])?;

                        total_filled += 1;
                    }
                }
            }
        }

        tx.commit()?;
        Ok(total_filled)
    }

    /// Récupère les bougies dans une plage de temps
    ///
    /// SUBTILITÉ RUST: Retourne un Vec<Candle>
    /// Le Vec est alloué sur le heap et ownership est transféré à l'appelant
    fn fetch_candles_in_range(
        conn: &Connection,
        provider: &str,
        symbol: &str,
        timeframe: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<Candle>> {
        let mut stmt = conn.prepare(
            "SELECT open_time, open, high, low, close, volume, close_time,
                    quote_asset_volume, number_of_trades,
                    taker_buy_base_asset_volume, taker_buy_quote_asset_volume
             FROM candlesticks
             WHERE provider = ?1 AND symbol = ?2 AND timeframe = ?3
                   AND open_time >= ?4 AND open_time <= ?5
             ORDER BY open_time ASC",
        )?;

        let candles = stmt
            .query_map(
                params![provider, symbol, timeframe, start_time, end_time],
                |row| {
                    Ok(Candle {
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
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(candles)
    }

    /// Interpole une bougie entre deux bougies existantes
    ///
    /// ALGORITHME: Interpolation linéaire
    /// Pour chaque champ: valeur = A + (B-A) × ratio
    ///
    /// PARAMÈTRES:
    /// - current: bougie avant le gap
    /// - next: bougie après le gap
    /// - ratio: position relative (0.0 à 1.0)
    /// - interval: intervalle du timeframe en ms
    fn interpolate_candle(current: &Candle, next: &Candle, ratio: f64, interval: i64) -> Candle {
        let open_time =
            current.open_time + ((next.open_time - current.open_time) as f64 * ratio) as i64;

        Candle {
            open_time,
            open: current.open + (next.open - current.open) * ratio,
            high: current.high + (next.high - current.high) * ratio,
            low: current.low + (next.low - current.low) * ratio,
            close: current.close + (next.close - current.close) * ratio,
            volume: current.volume + (next.volume - current.volume) * ratio,
            close_time: open_time + interval - 1,
            quote_asset_volume: current.quote_asset_volume
                + (next.quote_asset_volume - current.quote_asset_volume) * ratio,
            number_of_trades: (current.number_of_trades as f64
                + (next.number_of_trades as f64 - current.number_of_trades as f64) * ratio)
                as i64,
            taker_buy_base_asset_volume: current.taker_buy_base_asset_volume
                + (next.taker_buy_base_asset_volume - current.taker_buy_base_asset_volume) * ratio,
            taker_buy_quote_asset_volume: current.taker_buy_quote_asset_volume
                + (next.taker_buy_quote_asset_volume - current.taker_buy_quote_asset_volume)
                    * ratio,
        }
    }

    /// Convertit un timeframe en intervalle en millisecondes
    ///
    /// DESIGN: Fonction helper pour éviter la duplication de code
    fn timeframe_to_interval(timeframe: &str) -> i64 {
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
            _ => 300_000, // Par défaut: 5m
        }
    }
}
