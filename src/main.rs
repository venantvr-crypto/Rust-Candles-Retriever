/// Programme principal de récupération des chandeliers Binance
///
/// ARCHITECTURE REFACTORÉE:
/// Ce programme utilise une architecture modulaire avec séparation des responsabilités:
/// - database: Gestion de la connexion et du schéma SQLite
/// - timeframe_status: Gestion du statut de complétion des timeframes
/// - retriever: Récupération des bougies depuis l'API Binance
/// - gap_filler: Interpolation linéaire des trous
/// - verify: Vérification de l'intégrité des données
use anyhow::Result;
use binance::api::*;
use binance::market::*;
use chrono::{DateTime, NaiveDateTime, Utc};
use clap::Parser;
use rust_candles_retriever::{
    database::DatabaseManager, retriever::CandleRetriever, timeframe_status::TimeframeStatus,
    verify,
};

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

    /// Vérifier l'espacement des données après récupération
    #[arg(short = 'v', long)]
    verify: bool,

    /// Forcer le retraitement des timeframes complets
    #[arg(short = 'f', long)]
    force: bool,

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
    println!("Base de données initialisée.");

    // Timeframes supportés
    let timeframes = vec!["5m", "15m", "30m", "1h"];

    // Initialiser le client Binance
    let market: Market = Binance::new(None, None);

    // Parser la date de début si fournie
    let start_timestamp_ms = parse_start_date(args.start_date.as_deref())?;

    // Traiter chaque timeframe
    for tf in &timeframes {
        // Vérifier si le timeframe est déjà complet (sauf si --force)
        if !args.force && TimeframeStatus::is_complete(db.connection(), "binance", &symbol, tf) {
            println!(
                "⏭️  Timeframe {} déjà complet pour {}. Passage au suivant.",
                tf, symbol
            );
            println!("   (Utilisez --force pour forcer le retraitement)");
            continue;
        }

        if args.force && TimeframeStatus::is_complete(db.connection(), "binance", &symbol, tf) {
            println!(
                "🔄 Mode --force activé: retraitement du timeframe {} pour {}",
                tf, symbol
            );
        }

        println!("Récupération pour le timeframe: {}...", tf);

        // Créer le récupérateur et lancer la récupération
        let mut retriever = CandleRetriever::new(
            &market,
            db.connection_mut(),
            &symbol,
            tf,
            start_timestamp_ms,
        );

        match retriever.fetch_and_store() {
            Ok(count) => println!("Terminé pour {}. {} nouvelles bougies insérées.", tf, count),
            Err(e) => eprintln!("Erreur lors de la récupération pour {}: {}", tf, e),
        }
    }

    println!("Toutes les opérations sont terminées.");

    // Vérification optionnelle de l'intégrité
    if args.verify {
        println!("\n========================================");
        println!("VÉRIFICATION DE L'ESPACEMENT DES DONNÉES");
        println!("========================================");

        for tf in &timeframes {
            if let Err(e) = verify::verify_data_spacing(db.connection(), "binance", &symbol, tf) {
                eprintln!("Erreur lors de la vérification pour {}: {}", tf, e);
            }
        }
    }

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
