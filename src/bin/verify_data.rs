// ============================================================================
// BINAIRE STANDALONE DE VÉRIFICATION DES DONNÉES
// ============================================================================
//
// Programme CLI indépendant pour vérifier l'intégrité des données stockées
// Peut être exécuté séparément du programme principal de récupération
//
// SUBTILITÉ RUST #22: Structure des binaires
// Les fichiers dans src/bin/ sont des binaires indépendants
// Chacun a son propre main() et peut avoir ses propres dépendances
// Compilé séparément: cargo build --bin verify_data

use anyhow::Result;
use clap::Parser;
use rusqlite::Connection;
use std::path::Path;

// SUBTILITÉ RUST #23: include! macro
// include!() copie-colle le contenu d'un fichier à la compilation
// Ici utilisé pour réutiliser verify.rs sans le publier comme bibliothèque
// Alternative plus propre: créer une lib.rs et utiliser `use crate::verify;`
mod verify {
    include!("../../src/verify.rs");
}

/// Arguments CLI pour le programme de vérification
///
/// SUBTILITÉ RUST #24: Valeurs par défaut avec clap
/// default_value = valeur utilisée si l'argument n'est pas fourni
/// value_delimiter = ',' permet de passer plusieurs valeurs: --timeframes 5m,15m,1h
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

/// Point d'entrée du binaire de vérification
///
/// ALGORITHME:
/// 1. Parse les arguments CLI
/// 2. Vérifie que le fichier DB existe
/// 3. Ouvre la connexion DB
/// 4. Pour chaque timeframe demandé, lance verify_data_spacing()
fn main() -> Result<()> {
    let args = Args::parse();

    // Validation: le fichier DB doit exister
    let path = Path::new(&args.db_file);
    if !path.exists() {
        eprintln!(
            "Erreur: Le fichier de base de données '{}' n'existe pas",
            args.db_file
        );
        // SUBTILITÉ RUST #25: std::process::exit()
        // exit(1) termine immédiatement le programme avec code d'erreur 1
        // Alternative: return Err(...) mais exit() est plus explicite pour erreurs CLI
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
