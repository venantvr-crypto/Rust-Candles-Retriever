/// Programme principal de récupération des chandeliers Binance
///
/// ARCHITECTURE SIMPLIFIÉE:
/// - Récupère 1000 bougies à la fois depuis maintenant (ou dernière bougie)
/// - Parcourt tous les timeframes simultanément
/// - Retire dynamiquement les timeframes qui n'insèrent plus rien
/// - Arrêt automatique quand tous les timeframes sont épuisés ou date limite atteinte
use anyhow::Result;
use binance::api::*;
use binance::market::*;
use chrono::{DateTime, NaiveDateTime, Utc};
use clap::Parser;
use rust_candles_retriever::{database::DatabaseManager, retriever::CandleRetriever};

/// Arguments CLI du programme
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Le symbole/paire de trading à récupérer (ex: BTCUSDT)
    #[arg(short, long)]
    symbol: String,

    /// Date de début au format YYYY-MM-DD
    #[arg(short = 'd', long)]
    start_date: Option<String>,

    /// Fichier de base de données
    #[arg(long, default_value = "candlesticks.db")]
    db_file: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let symbol = args.symbol.to_uppercase();

    println!("Démarrage de la récupération pour le symbole: {}", symbol);

    // Initialiser la base de données
    let mut db = DatabaseManager::new(&args.db_file)?;
    println!("Base de données initialisée.\n");

    // Timeframes supportés - liste dynamique
    let mut active_timeframes: Vec<&str> =
        vec!["5m", "15m", "30m", "1h", "2h", "4h", "6h", "12h", "1d"];

    // Initialiser le client Binance
    let market: Market = Binance::new(None, None);

    // Parser la date de début si fournie
    let start_timestamp_ms = parse_start_date(args.start_date.as_deref())?;

    // Boucle principale: traiter tous les timeframes simultanément
    let mut iteration = 0;
    loop {
        iteration += 1;
        println!("═══ Itération #{} ═══", iteration);
        println!("Timeframes actifs: {:?}\n", active_timeframes);

        if active_timeframes.is_empty() {
            println!("✅ Tous les timeframes ont été traités complètement!");
            break;
        }

        let mut exhausted_timeframes = Vec::new();

        // Traiter chaque timeframe actif
        for tf in &active_timeframes {
            println!("→ Traitement du timeframe {}...", tf);

            let mut retriever = CandleRetriever::new(
                &market,
                db.connection_mut(),
                &symbol,
                tf,
                start_timestamp_ms,
            );

            match retriever.fetch_one_batch() {
                Ok((inserted, is_exhausted)) => {
                    if inserted > 0 {
                        println!("  ✓ {} nouvelles bougies insérées", inserted);
                    }

                    // Retirer du pool si: date limite atteinte OU plus d'insertions
                    if is_exhausted || inserted == 0 {
                        if is_exhausted {
                            println!("  🏁 Timeframe {} épuisé (date limite atteinte)", tf);
                        } else {
                            println!("  🏁 Timeframe {} épuisé (plus de nouvelles données)", tf);
                        }
                        exhausted_timeframes.push(*tf);
                    }
                }
                Err(e) => {
                    eprintln!("  ⚠  Erreur: {}", e);
                }
            }
        }

        // Retirer les timeframes épuisés du pool actif
        active_timeframes.retain(|tf| !exhausted_timeframes.contains(tf));

        if !exhausted_timeframes.is_empty() {
            println!(
                "\n🗑  Timeframes retirés du pool: {:?}",
                exhausted_timeframes
            );
        }

        println!();

        // Pause pour respecter les rate limits
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    println!("Toutes les opérations sont terminées.");
    Ok(())
}

/// Parse une date au format YYYY-MM-DD en timestamp millisecondes
fn parse_start_date(date_str: Option<&str>) -> Result<Option<i64>> {
    match date_str {
        Some(date) => {
            let naive_date = NaiveDateTime::parse_from_str(
                &(date.to_string() + " 00:00:00"),
                "%Y-%m-%d %H:%M:%S",
            )?;
            let datetime_utc = DateTime::<Utc>::from_naive_utc_and_offset(naive_date, Utc);
            Ok(Some(datetime_utc.timestamp_millis()))
        }
        None => Ok(None),
    }
}
