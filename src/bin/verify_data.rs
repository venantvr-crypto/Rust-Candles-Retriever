use anyhow::Result;
use clap::Parser;
use rusqlite::Connection;
use std::path::Path;

// Réutiliser le module verify
mod verify {
    include!("../../src/verify.rs");
}

#[derive(Parser, Debug)]
#[command(author, version, about = "Vérifier l'espacement des données de chandelier", long_about = None)]
struct Args {
    /// Le symbole/paire de trading à vérifier (ex: BTCUSDT)
    #[arg(short, long)]
    symbol: String,

    /// Le provider (par défaut: binance)
    #[arg(short, long, default_value = "binance")]
    provider: String,

    /// Les timeframes à vérifier (par défaut: tous)
    #[arg(short, long, value_delimiter = ',')]
    timeframes: Option<Vec<String>>,

    /// Fichier de base de données
    #[arg(short = 'f', long, default_value = "candlesticks.db")]
    db_file: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let path = Path::new(&args.db_file);
    if !path.exists() {
        eprintln!(
            "Erreur: Le fichier de base de données '{}' n'existe pas",
            args.db_file
        );
        std::process::exit(1);
    }

    let conn = Connection::open(path)?;

    let timeframes = args.timeframes.unwrap_or_else(|| {
        vec![
            "5m".to_string(),
            "15m".to_string(),
            "30m".to_string(),
            "1h".to_string(),
        ]
    });

    println!("========================================");
    println!("VÉRIFICATION DE L'ESPACEMENT DES DONNÉES");
    println!("========================================");
    println!("Provider: {}", args.provider);
    println!("Symbol: {}", args.symbol);
    println!("Timeframes: {:?}", timeframes);
    println!();

    for tf in &timeframes {
        if let Err(e) = verify::verify_data_spacing(&conn, &args.provider, &args.symbol, tf) {
            eprintln!("Erreur lors de la vérification pour {}: {}", tf, e);
        }
    }

    Ok(())
}
