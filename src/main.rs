use anyhow::Result;
use binance::api::*;
use binance::market::*;
use binance::model::KlineSummaries;
use chrono::{DateTime, NaiveDateTime, Utc};
use clap::Parser;
use rusqlite::{Connection, Result as SqlResult, params};
use std::path::Path;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH}; // Pour obtenir l'heure actuelle

mod verify;

const DB_FILE: &str = "candlesticks.db";
const BATCH_SIZE: usize = 1000;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Le symbole/paire de trading à récupérer (ex: BTCUSDT)
    #[arg(short, long)]
    symbol: String,

    /// Optionnel: Date de début au format YYYY-MM-DD (remonte jusqu'à cette date)
    #[arg(short = 'd', long)]
    start_date: Option<String>,

    /// Vérifier l'espacement des données après la récupération
    #[arg(short = 'v', long)]
    verify: bool,
}

// Structure pour correspondre aux Klines de l'API REST
// Note: les champs peuvent varier légèrement de KlineEvent
// type Kline = (i64, String, String, String, String, String, i64, String, i64, String, String, String);

fn main() -> Result<()> {
    // Utilisation de anyhow::Result
    let args = Args::parse();
    let symbol = args.symbol.to_uppercase();
    println!(
        "Démarrage de la récupération pour le symbole: {}",
        symbol.clone()
    );

    let mut conn = setup_database()?;
    println!("Base de données initialisée.");

    let timeframes = vec!["5m", "15m", "30m", "1h"];
    let market: Market = Binance::new(None, None); // Pas besoin de clés API pour les données publiques

    // Convertir la date de début optionnelle en timestamp
    let start_timestamp_ms: Option<i64> = match args.start_date {
        Some(date_str) => {
            let naive_date =
                NaiveDateTime::parse_from_str(&(date_str + " 00:00:00"), "%Y-%m-%d %H:%M:%S")?;
            // Convertir NaiveDateTime en DateTime<Utc>
            let datetime_utc = DateTime::<Utc>::from_naive_utc_and_offset(naive_date, Utc);

            Some(datetime_utc.timestamp_millis())
        }
        None => None,
    };

    for tf in &timeframes {
        println!("Récupération pour le timeframe: {}...", tf);
        match fetch_and_store_klines(&market, &mut conn, &symbol, tf, start_timestamp_ms) {
            Ok(count) => println!("Terminé pour {}. {} nouvelles bougies insérées.", tf, count),
            Err(e) => eprintln!("Erreur lors de la récupération pour {}: {}", tf, e),
        }
    }

    println!("Toutes les opérations sont terminées.");

    // Vérifier l'espacement des données si demandé
    if args.verify {
        println!("\n========================================");
        println!("VÉRIFICATION DE L'ESPACEMENT DES DONNÉES");
        println!("========================================");

        for tf in &timeframes {
            if let Err(e) = verify::verify_data_spacing(&conn, "binance", &symbol, tf) {
                eprintln!("Erreur lors de la vérification pour {}: {}", tf, e);
            }
        }
    }

    Ok(())
}

fn setup_database() -> SqlResult<Connection> {
    let path = Path::new(DB_FILE);
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
            UNIQUE(provider, symbol, timeframe, open_time)
        )",
        [],
    )?;
    Ok(conn)
}

fn fetch_and_store_klines(
    market: &Market,
    conn: &mut Connection,
    symbol: &str,
    timeframe: &str,
    start_timestamp_ms: Option<i64>,
) -> Result<i64> {
    // Utilisation de anyhow::Result
    let mut total_inserted = 0i64;
    let mut end_time_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as i64;

    // Vérifier la dernière bougie existante pour éviter de tout re-télécharger
    let last_stored_open_time: Option<i64> = conn
        .query_row(
            "SELECT MAX(open_time) FROM candlesticks WHERE provider = ?1 AND symbol = ?2 AND timeframe = ?3",
            params!["binance", symbol, timeframe],
            |row| row.get(0),
        )
        .unwrap_or(None);

    // Si on a déjà des données, on commence juste après la dernière
    if let Some(last_time) = last_stored_open_time {
        println!(
            "Reprise après la dernière bougie enregistrée: {}",
            format_timestamp_ms(last_time)
        );
        // On ajuste end_time_ms pour la requête initiale afin de récupérer les données *après* la dernière enregistrée
        // Binance utilise endTime comme borne supérieure *exclusive* dans certains cas,
        // mais pour les klines, c'est inclusif. On va quand même chercher un peu plus large pour être sûr
        // et laisser la contrainte UNIQUE faire le travail.
        // On ne modifie pas end_time_ms initialement, on le laisse à maintenant.
        // La boucle va naturellement chercher les données manquantes.
    } else {
        println!(
            "Aucune donnée existante trouvée pour {}/{}. Récupération depuis le début (ou --start-date).",
            symbol, timeframe
        );
    }

    loop {
        println!(
            "Fetching {} klines ending before {}",
            BATCH_SIZE,
            format_timestamp_ms(end_time_ms)
        );

        // Utiliser l'appel get_klines qui prend un endTime optionnel
        let klines_data = match market
            .get_klines(symbol, timeframe, Some(BATCH_SIZE as u16), None, Some(end_time_ms as u64)) // startTime=None, endTime=Some(end_time_ms)
        {
            Ok(klines) => klines,
            Err(e) => {
                eprintln!("Erreur API Binance: {}", e);
                // Attendre avant de réessayer ?
                thread::sleep(Duration::from_secs(5));
                continue; // Tente de refaire la même requête après une pause
            }
        };

        // Extract the actual Vec from the enum
        let klines = match klines_data {
            KlineSummaries::AllKlineSummaries(vec) => vec,
        };

        if klines.len() == 0 {
            println!(
                "Aucune bougie supplémentaire retournée par l'API. Arrêt pour {}/{}.",
                symbol, timeframe
            );
            break;
        }

        let oldest_kline_time = klines[0].open_time;

        let tx = conn.transaction()?;
        let mut inserted_in_batch = 0;
        {
            // Préparer le statement une fois pour le batch
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO candlesticks (
                    provider, symbol, timeframe, open_time, open, high, low, close, volume,
                    close_time, quote_asset_volume, number_of_trades,
                    taker_buy_base_asset_volume, taker_buy_quote_asset_volume
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            )?;

            for kline in &klines {
                // Kline est maintenant une structure, accédons aux champs
                let changes = stmt.execute(params![
                    "binance",
                    symbol,
                    timeframe,
                    kline.open_time,
                    kline.open.parse::<f64>().unwrap_or(0.0), // Parser les String en f64
                    kline.high.parse::<f64>().unwrap_or(0.0),
                    kline.low.parse::<f64>().unwrap_or(0.0),
                    kline.close.parse::<f64>().unwrap_or(0.0),
                    kline.volume.parse::<f64>().unwrap_or(0.0),
                    kline.close_time,
                    kline.quote_asset_volume.parse::<f64>().unwrap_or(0.0),
                    kline.number_of_trades,
                    kline
                        .taker_buy_base_asset_volume
                        .parse::<f64>()
                        .unwrap_or(0.0),
                    kline
                        .taker_buy_quote_asset_volume
                        .parse::<f64>()
                        .unwrap_or(0.0),
                ])?;
                if changes > 0 {
                    inserted_in_batch += 1;
                }
            }
        } // stmt est libéré ici
        tx.commit()?;

        total_inserted += inserted_in_batch;
        println!(
            "Batch traité pour {}/{}. {} nouvelles bougies insérées. Bougie la plus ancienne: {}",
            symbol,
            timeframe,
            inserted_in_batch,
            format_timestamp_ms(oldest_kline_time)
        );

        // Combler les trous dans le batch qui vient d'être inséré
        let filled = fill_gaps_in_range(
            conn,
            "binance",
            symbol,
            timeframe,
            oldest_kline_time,
            end_time_ms,
        )?;
        if filled > 0 {
            println!("  → {} bougies interpolées pour combler les trous", filled);
            total_inserted += filled;
        }

        // Préparer pour le prochain batch en remontant le temps
        // On met endTime juste avant l'ouverture de la bougie la plus ancienne de ce batch
        end_time_ms = oldest_kline_time; // Utiliser directement open_time pour la prochaine requête

        // Vérifier si on a atteint ou dépassé la date de début demandée
        if let Some(start_ts) = start_timestamp_ms {
            if oldest_kline_time <= start_ts {
                println!(
                    "Date de début ({}) atteinte ou dépassée. Arrêt pour {}/{}.",
                    format_timestamp_ms(start_ts),
                    symbol,
                    timeframe
                );
                break;
            }
        }

        // Petite pause pour respecter les limites de l'API Binance
        thread::sleep(Duration::from_millis(10 * 500));
    }

    Ok(total_inserted)
}

/// Comble les trous dans une plage de données avec interpolation linéaire
fn fill_gaps_in_range(
    conn: &mut Connection,
    provider: &str,
    symbol: &str,
    timeframe: &str,
    start_time: i64,
    end_time: i64,
) -> Result<i64> {
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
        "1M" => 2_592_000_000,
        _ => return Ok(0),
    };

    // Récupérer toutes les bougies dans la plage, triées par date
    let mut stmt = conn.prepare(
        "SELECT open_time, open, high, low, close, volume, close_time,
                quote_asset_volume, number_of_trades,
                taker_buy_base_asset_volume, taker_buy_quote_asset_volume
         FROM candlesticks
         WHERE provider = ?1 AND symbol = ?2 AND timeframe = ?3
           AND open_time >= ?4 AND open_time <= ?5
         ORDER BY open_time ASC",
    )?;

    let mut rows = stmt.query(params![provider, symbol, timeframe, start_time, end_time])?;

    #[derive(Debug)]
    #[allow(dead_code)]
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
        return Ok(0); // Pas assez de données pour interpoler
    }

    let mut total_filled = 0i64;
    let tx = conn.transaction()?;

    {
        let mut insert_stmt = tx.prepare(
            "INSERT OR IGNORE INTO candlesticks (
                provider, symbol, timeframe, open_time, open, high, low, close, volume,
                close_time, quote_asset_volume, number_of_trades,
                taker_buy_base_asset_volume, taker_buy_quote_asset_volume
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        )?;

        for i in 0..candles.len() - 1 {
            let current = &candles[i];
            let next = &candles[i + 1];

            let time_diff = next.open_time - current.open_time;

            // Si il y a un gap
            if time_diff > expected_interval_ms {
                let missing_candles = (time_diff / expected_interval_ms) - 1;

                // Interpoler linéairement pour chaque bougie manquante
                for j in 1..=missing_candles {
                    let ratio = j as f64 / (missing_candles + 1) as f64;
                    let interpolated_time = current.open_time + (j * expected_interval_ms);

                    // Interpolation linéaire pour tous les champs
                    let interpolated_open = current.open + (next.open - current.open) * ratio;
                    let interpolated_high = current.high + (next.high - current.high) * ratio;
                    let interpolated_low = current.low + (next.low - current.low) * ratio;
                    let interpolated_close = current.close + (next.close - current.close) * ratio;
                    let interpolated_volume =
                        current.volume + (next.volume - current.volume) * ratio;
                    let interpolated_close_time = interpolated_time + expected_interval_ms - 1;
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

                    let changes = insert_stmt.execute(params![
                        provider,
                        symbol,
                        timeframe,
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
                    ])?;

                    if changes > 0 {
                        total_filled += 1;
                    }
                }
            }
        }
    }

    tx.commit()?;
    Ok(total_filled)
}

// Fonction utilitaire pour afficher les timestamps
fn format_timestamp_ms(timestamp_ms: i64) -> String {
    // Crée un DateTime à partir du timestamp Unix en millisecondes
    if let Some(datetime_utc) = DateTime::<Utc>::from_timestamp_millis(timestamp_ms) {
        // Formate la date et l'heure
        datetime_utc.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        "Invalid timestamp".to_string()
    }
}
