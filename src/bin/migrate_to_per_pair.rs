/// Script de migration: candlesticks.db → une BDD par paire
///
/// Usage: cargo run --bin migrate_to_per_pair -- --source candlesticks.db --dest-dir .
use anyhow::Result;
use clap::Parser;
use rusqlite::{Connection, params};
use std::collections::HashSet;
use std::path::Path;

#[derive(Parser, Debug)]
struct Args {
    /// Base de données source (ex: candlesticks.db)
    #[arg(short, long)]
    source: String,

    /// Répertoire de destination pour les BDD par paire
    #[arg(short, long, default_value = ".")]
    dest_dir: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

    println!("🔄 Migration des données vers bases par paire");
    println!("📁 Source: {}", args.source);
    println!("📁 Destination: {}\n", args.dest_dir);

    // Vérifier que la source existe
    if !Path::new(&args.source).exists() {
        anyhow::bail!("❌ Fichier source introuvable: {}", args.source);
    }

    // Ouvrir la base source
    let source_conn = Connection::open(&args.source)?;
    println!("✓ Base source ouverte");

    // Récupérer tous les symboles distincts
    let mut stmt = source_conn.prepare(
        "SELECT DISTINCT symbol FROM candlesticks WHERE provider = 'binance' ORDER BY symbol",
    )?;

    let symbols: HashSet<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(Result::ok)
        .collect();

    println!("✓ {} symboles trouvés: {:?}\n", symbols.len(), symbols);

    // Migrer chaque symbole
    for symbol in symbols {
        migrate_symbol(&source_conn, &symbol, &args.dest_dir)?;
    }

    println!("\n✅ Migration terminée!");
    println!("💡 Pensez à mettre à jour DB_PATH → DB_DIR dans votre configuration.");

    Ok(())
}

fn migrate_symbol(source: &Connection, symbol: &str, dest_dir: &str) -> Result<()> {
    println!("→ Migration de {}...", symbol);

    // Créer le chemin de destination
    let dest_path = format!("{}/{}.db", dest_dir, symbol);

    // Vérifier si le fichier existe déjà
    if Path::new(&dest_path).exists() {
        println!("  ⚠️ {} existe déjà, ignoré", dest_path);
        return Ok(());
    }

    // Créer la base de destination avec le même schéma
    let dest = Connection::open(&dest_path)?;

    // Créer les tables (même schéma que la base source)
    dest.execute(
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

    dest.execute(
        "CREATE INDEX IF NOT EXISTS idx_candles_query
         ON candlesticks(provider, symbol, timeframe, open_time)",
        [],
    )?;

    dest.execute(
        "CREATE TABLE IF NOT EXISTS timeframe_status (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            provider TEXT NOT NULL,
            symbol TEXT NOT NULL,
            timeframe TEXT NOT NULL,
            oldest_time INTEGER,
            newest_time INTEGER,
            UNIQUE(provider, symbol, timeframe)
        )",
        [],
    )?;

    // Copier les candlesticks
    let mut select_stmt = source.prepare(
        "SELECT provider, symbol, timeframe, open_time, open, high, low, close,
                volume, close_time, quote_asset_volume, number_of_trades,
                taker_buy_base_asset_volume, taker_buy_quote_asset_volume, interpolated
         FROM candlesticks
         WHERE symbol = ?1",
    )?;

    let mut insert_stmt = dest.prepare(
        "INSERT INTO candlesticks (
            provider, symbol, timeframe, open_time, open, high, low, close,
            volume, close_time, quote_asset_volume, number_of_trades,
            taker_buy_base_asset_volume, taker_buy_quote_asset_volume, interpolated
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
    )?;

    let rows = select_stmt.query_map(params![symbol], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, f64>(4)?,
            row.get::<_, f64>(5)?,
            row.get::<_, f64>(6)?,
            row.get::<_, f64>(7)?,
            row.get::<_, f64>(8)?,
            row.get::<_, i64>(9)?,
            row.get::<_, f64>(10)?,
            row.get::<_, i64>(11)?,
            row.get::<_, f64>(12)?,
            row.get::<_, f64>(13)?,
            row.get::<_, i64>(14)?,
        ))
    })?;

    let mut count = 0;
    for row_result in rows {
        let row = row_result?;
        insert_stmt.execute(params![
            row.0, row.1, row.2, row.3, row.4, row.5, row.6, row.7, row.8, row.9, row.10, row.11,
            row.12, row.13, row.14
        ])?;
        count += 1;
    }

    // Copier les status
    let mut select_status = source.prepare(
        "SELECT provider, symbol, timeframe, oldest_time, newest_time
         FROM timeframe_status
         WHERE symbol = ?1",
    )?;

    let mut insert_status = dest.prepare(
        "INSERT INTO timeframe_status (provider, symbol, timeframe, oldest_time, newest_time)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;

    let status_rows = select_status.query_map(params![symbol], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<i64>>(3)?,
            row.get::<_, Option<i64>>(4)?,
        ))
    })?;

    let mut status_count = 0;
    for row_result in status_rows {
        let row = row_result?;
        insert_status.execute(params![row.0, row.1, row.2, row.3, row.4])?;
        status_count += 1;
    }

    println!(
        "  ✓ {} créé: {} candles, {} status",
        dest_path, count, status_count
    );

    Ok(())
}
