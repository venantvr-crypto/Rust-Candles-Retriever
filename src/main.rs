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
use futures_util::future;
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

    /// Répertoire des bases de données (une par paire)
    #[arg(long, default_value = ".")]
    db_dir: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let symbol = args.symbol.to_uppercase();

    println!("Démarrage de la récupération pour le symbole: {}", symbol);

    // Créer le nom de fichier basé sur le symbole (ex: BTCUSDT.db)
    let db_file = format!("{}/{}.db", args.db_dir, symbol);
    println!("Fichier de base de données: {}", db_file);

    // Initialiser la base de données (sera créée si elle n'existe pas)
    let db = DatabaseManager::new(&db_file)?;
    println!("Base de données initialisée.\n");
    drop(db); // Fermer la connexion initiale

    // Timeframes supportés - liste dynamique
    let mut active_timeframes: Vec<String> = vec![
        "3m", "5m", "15m", "30m", "1h", "2h", "4h", "6h", "8h", "12h", "1d", "3d",
    ]
    .into_iter()
    .map(|s| s.to_string())
    .collect();

    // Parser la date de début si fournie
    let start_timestamp_ms = parse_start_date(args.start_date.as_deref())?;

    // Boucle principale: traiter tous les timeframes en parallèle
    let mut iteration = 0;
    loop {
        iteration += 1;
        println!("═══ Itération #{} ═══", iteration);
        println!("Timeframes actifs: {:?}\n", active_timeframes);

        if active_timeframes.is_empty() {
            println!("✅ Tous les timeframes ont été traités complètement!");
            break;
        }

        // Créer une tâche pour chaque timeframe
        let mut tasks = Vec::new();

        for tf in active_timeframes.clone() {
            let symbol_clone = symbol.clone();
            let db_file_clone = db_file.clone();

            // Spawner une tâche bloquante pour chaque timeframe
            let task = tokio::task::spawn_blocking(move || {
                // Créer une connexion DB par tâche (SQLite ne supporte pas bien la concurrence)
                let mut db = match DatabaseManager::new(&db_file_clone) {
                    Ok(db) => db,
                    Err(e) => return (tf.clone(), Err(anyhow::anyhow!("DB error: {}", e))),
                };

                // Initialiser le client Binance
                let market: Market = Binance::new(None, None);

                let mut retriever = CandleRetriever::new(
                    &market,
                    db.connection_mut(),
                    &symbol_clone,
                    &tf,
                    start_timestamp_ms,
                );

                let result = retriever.fetch_one_batch();
                (tf, result)
            });

            tasks.push(task);
        }

        // Attendre que toutes les tâches se terminent
        let results = future::join_all(tasks).await;

        let mut exhausted_timeframes = Vec::new();

        // Traiter les résultats
        for result in results {
            match result {
                Ok((tf, fetch_result)) => {
                    match fetch_result {
                        Ok((inserted, is_exhausted)) => {
                            if inserted > 0 {
                                println!("  ✓ {} : {} nouvelles bougies insérées", tf, inserted);
                            }

                            // Retirer du pool si: date limite atteinte OU plus d'insertions
                            if is_exhausted || inserted == 0 {
                                if is_exhausted {
                                    println!("  🏁 {} épuisé (date limite atteinte)", tf);
                                } else {
                                    println!("  🏁 {} épuisé (plus de nouvelles données)", tf);
                                }
                                exhausted_timeframes.push(tf);
                            }
                        }
                        Err(e) => {
                            eprintln!("  ⚠  {} : Erreur: {}", tf, e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("  ⚠  Erreur de tâche: {}", e);
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

        // Pause pour respecter les rate limits de l'API
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
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
