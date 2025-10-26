// ============================================================================
// IMPORTS ET CONFIGURATION
// ============================================================================

// anyhow::Result - Gestion d'erreurs ergonomique en Rust
// Permet de propager les erreurs avec `?` sans définir un type d'erreur explicite
// Alternative à Result<T, E> quand on n'a pas besoin d'un type d'erreur spécifique
use anyhow::Result;

// Imports de la bibliothèque binance-rs pour interagir avec l'API Binance
use binance::api::*; // Trait API pour les opérations génériques
use binance::market::*; // Module Market pour les données de marché
use binance::model::KlineSummaries; // Enum qui encapsule les résultats de klines

// chrono - Bibliothèque de manipulation de dates/temps en Rust
use chrono::{DateTime, NaiveDateTime, Utc};

// clap - Bibliothèque de parsing d'arguments CLI (Command Line Interface Parser)
// Utilise les macros dérivées pour générer le code de parsing automatiquement
use clap::Parser;

// rusqlite - Wrapper Rust pour SQLite
// Result as SqlResult: renommage pour éviter le conflit avec std::Result
// params!: macro pour passer des paramètres SQL de manière type-safe
use rusqlite::{Connection, Result as SqlResult, params};

use std::path::Path;
use std::thread; // Pour thread::sleep() (mode synchrone, pas tokio)
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// Module de vérification défini dans src/verify.rs
mod verify;

// Constantes globales - En Rust, les const sont évaluées à la compilation
// et peuvent être utilisées partout sans overhead runtime
const DB_FILE: &str = "candlesticks.db";
const BATCH_SIZE: usize = 1000; // usize = taille naturelle du CPU (32/64 bits)

// ============================================================================
// STRUCTURES DE DONNÉES
// ============================================================================

/// Structure des arguments CLI générée automatiquement par clap
///
/// SUBTILITÉ RUST #1: Derive macros
/// #[derive(Parser)] génère automatiquement le code de parsing des arguments
/// C'est un exemple de "programmation générative" à la compilation
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Le symbole/paire de trading à récupérer (ex: BTCUSDT)
    ///
    /// SUBTILITÉ RUST #2: String vs &str
    /// String = owned (allouée sur le heap, mutable)
    /// &str = borrowed (référence, immutable)
    /// Ici on utilise String car on possède la valeur (pas juste une référence)
    #[arg(short, long)]
    symbol: String,

    /// Optionnel: Date de début au format YYYY-MM-DD
    ///
    /// SUBTILITÉ RUST #3: Option<T>
    /// Option est un enum avec deux variantes: Some(T) ou None
    /// Remplace les NULL/nil d'autres langages de manière type-safe
    /// Le compilateur force à gérer les deux cas (pas de NullPointerException!)
    #[arg(short = 'd', long)]
    start_date: Option<String>,

    /// Vérifier l'espacement des données après la récupération
    #[arg(short = 'v', long)]
    verify: bool,
}

// ============================================================================
// FONCTION PRINCIPALE
// ============================================================================

/// Point d'entrée du programme
///
/// SUBTILITÉ RUST #4: Result<()>
/// () = unit type (équivalent à void)
/// Result<()> signifie "retourne Ok(()) en cas de succès ou Err(e) en cas d'erreur"
/// L'opérateur ? propage automatiquement les erreurs vers le haut
fn main() -> Result<()> {
    // Parse les arguments CLI - panique si les arguments sont invalides
    let args = Args::parse();

    // SUBTILITÉ RUST #5: Ownership et clone
    // to_uppercase() consomme args.symbol (ownership move)
    // Mais on a besoin de symbol plusieurs fois, donc on le stocke
    let symbol = args.symbol.to_uppercase();

    // clone() crée une copie profonde - nécessaire car println! emprunte temporairement
    // Alternative: utiliser &symbol (mais ici clone est plus lisible)
    println!(
        "Démarrage de la récupération pour le symbole: {}",
        symbol.clone()
    );

    // SUBTILITÉ RUST #6: mut (mutabilité)
    // Par défaut, tout est immutable en Rust
    // mut permet de modifier la variable (ici, la connexion DB)
    // ? = opérateur de propagation d'erreur (équivalent à un early return si Err)
    let mut conn = setup_database()?;
    println!("Base de données initialisée.");

    // vec! = macro pour créer un Vec<T> (vecteur dynamique sur le heap)
    // &str = string slice (référence immutable vers une chaîne)
    let timeframes = vec!["5m", "15m", "30m", "1h"];

    // Binance::new(None, None) = pas de clés API (données publiques seulement)
    // None = variant de Option<T> qui représente l'absence de valeur
    let market: Market = Binance::new(None, None);

    // SUBTILITÉ RUST #7: Pattern matching avec match
    // match est exhaustif: le compilateur vérifie que tous les cas sont couverts
    // Plus sûr que les if/else car impossible d'oublier un cas
    let start_timestamp_ms: Option<i64> = match args.start_date {
        Some(date_str) => {
            // ALGORITHME: Parser la date YYYY-MM-DD et la convertir en timestamp UTC
            // + " 00:00:00" car parse_from_str attend une heure complète
            let naive_date =
                NaiveDateTime::parse_from_str(&(date_str + " 00:00:00"), "%Y-%m-%d %H:%M:%S")?;

            // NaiveDateTime = date sans timezone
            // DateTime<Utc> = date avec timezone UTC
            // Cette conversion est nécessaire pour timestamp_millis()
            let datetime_utc = DateTime::<Utc>::from_naive_utc_and_offset(naive_date, Utc);

            // Convertir en timestamp milliseconde (format Binance API)
            Some(datetime_utc.timestamp_millis())
        }
        None => None, // Pas de date de début = récupérer toutes les données
    };

    // SUBTILITÉ RUST #8: Itération avec référence (&)
    // &timeframes = itère sur des références (&str) au lieu de consommer le vecteur
    // Sinon, timeframes serait déplacé (moved) et on ne pourrait plus l'utiliser après
    for tf in &timeframes {
        // ALGORITHME: Vérifier si le timeframe est déjà complet
        // Si is_complete=1, on saute ce timeframe (déjà traité jusqu'à la limite historique)
        if is_timeframe_complete(&conn, "binance", &symbol, tf) {
            println!(
                "⏭️  Timeframe {} déjà complet pour {}. Passage au suivant.",
                tf, symbol
            );
            continue;
        }

        println!("Récupération pour le timeframe: {}...", tf);

        // SUBTILITÉ RUST #9: Emprunt mutable (&mut)
        // &mut conn = emprunte mutably la connexion (permet de la modifier)
        // &symbol = emprunte immutably le symbol (lecture seule)
        // Rust garantit qu'il n'y a qu'un seul emprunt mutable à la fois (data race impossible)
        match fetch_and_store_klines(&market, &mut conn, &symbol, tf, start_timestamp_ms) {
            Ok(count) => println!("Terminé pour {}. {} nouvelles bougies insérées.", tf, count),
            Err(e) => eprintln!("Erreur lors de la récupération pour {}: {}", tf, e),
        }
    }

    println!("Toutes les opérations sont terminées.");

    // ALGORITHME: Vérification optionnelle de l'intégrité des données
    if args.verify {
        println!("\n========================================");
        println!("VÉRIFICATION DE L'ESPACEMENT DES DONNÉES");
        println!("========================================");

        // SUBTILITÉ RUST #10: if let - pattern matching simplifié
        // Équivalent à: match result { Err(e) => ..., Ok(_) => {} }
        for tf in &timeframes {
            if let Err(e) = verify::verify_data_spacing(&conn, "binance", &symbol, tf) {
                eprintln!("Erreur lors de la vérification pour {}: {}", tf, e);
            }
        }
    }

    // Ok(()) = succès sans valeur de retour
    // En Rust, la dernière expression sans ; est le return implicite
    Ok(())
}

// ============================================================================
// UTILITAIRES DE REPRISE
// ============================================================================

/// Récupère le timestamp de la dernière bougie stockée pour un (provider, symbol, timeframe)
///
/// ALGORITHME DE REPRISE:
/// Cette fonction est cruciale pour le mode de reprise. Elle permet de:
/// 1. Éviter de re-télécharger des données déjà présentes en base
/// 2. Reprendre exactement là où on s'est arrêté en cas d'interruption
/// 3. Optimiser la bande passante en ne récupérant que les nouvelles données
///
/// SUBTILITÉ RUST #13a: Gestion des erreurs SQL avec unwrap_or
/// query_row() peut échouer si la table est vide ou n'existe pas
/// unwrap_or(None) transforme une erreur en None (approche pragmatique)
fn get_last_candle_time(
    conn: &Connection,
    provider: &str,
    symbol: &str,
    timeframe: &str,
) -> Option<i64> {
    conn.query_row(
        "SELECT MAX(open_time) FROM candlesticks
         WHERE provider = ?1 AND symbol = ?2 AND timeframe = ?3",
        params![provider, symbol, timeframe],
        |row| row.get(0),
    )
    .unwrap_or(None)
}

// ============================================================================
// GESTION DU STATUT DES TIMEFRAMES
// ============================================================================

/// Vérifie si un timeframe est marqué comme complet
///
/// ALGORITHME:
/// Un timeframe est "complet" quand on a atteint la limite historique de l'API
/// (pas de nouvelles bougies retournées par l'API)
///
/// RETOUR:
/// - true: timeframe déjà complet, pas besoin de le re-traiter
/// - false: timeframe incomplet ou jamais traité
fn is_timeframe_complete(conn: &Connection, provider: &str, symbol: &str, timeframe: &str) -> bool {
    conn.query_row(
        "SELECT is_complete FROM timeframe_status
         WHERE provider = ?1 AND symbol = ?2 AND timeframe = ?3",
        params![provider, symbol, timeframe],
        |row| row.get(0),
    )
    .unwrap_or(0)
        == 1
}

/// Marque un timeframe comme complet
///
/// ALGORITHME:
/// Appelé quand l'API retourne 0 bougies (limite historique atteinte)
/// Permet au programme de sauter ce timeframe lors des prochaines exécutions
///
/// SUBTILITÉ RUST #26: INSERT OR REPLACE
/// Upsert SQLite: insère si n'existe pas, met à jour si existe
/// Plus simple que INSERT ... ON CONFLICT UPDATE
fn mark_timeframe_complete(
    conn: &Connection,
    provider: &str,
    symbol: &str,
    timeframe: &str,
    oldest_candle_time: Option<i64>,
) -> Result<()> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as i64;

    conn.execute(
        "INSERT OR REPLACE INTO timeframe_status
         (provider, symbol, timeframe, oldest_candle_time, is_complete, last_updated)
         VALUES (?1, ?2, ?3, ?4, 1, ?5)",
        params![provider, symbol, timeframe, oldest_candle_time, now],
    )?;

    Ok(())
}

/// Met à jour la progression d'un timeframe (sans le marquer comme complet)
///
/// ALGORITHME:
/// Appelé régulièrement pendant la récupération pour tracker la progression
/// Utile pour le monitoring et le debug
fn update_timeframe_progress(
    conn: &Connection,
    provider: &str,
    symbol: &str,
    timeframe: &str,
    oldest_candle_time: i64,
) -> Result<()> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as i64;

    conn.execute(
        "INSERT OR REPLACE INTO timeframe_status
         (provider, symbol, timeframe, oldest_candle_time, is_complete, last_updated)
         VALUES (?1, ?2, ?3, ?4, 0, ?5)",
        params![provider, symbol, timeframe, oldest_candle_time, now],
    )?;

    Ok(())
}

// ============================================================================
// CONFIGURATION BASE DE DONNÉES
// ============================================================================

/// Initialise la connexion à la base de données SQLite
///
/// SUBTILITÉ RUST #11: Type alias
/// SqlResult<T> est un alias pour rusqlite::Result<T>
/// Permet d'éviter la confusion avec std::Result ou anyhow::Result
fn setup_database() -> SqlResult<Connection> {
    // Path::new() crée une référence vers un chemin sans allocation
    // &'static str -> &Path (zero-cost abstraction)
    let path = Path::new(DB_FILE);

    // Connection::open() peut échouer (fichier verrouillé, permissions, etc.)
    // ? propage l'erreur vers le haut si échec
    let conn = Connection::open(path)?;

    // ALGORITHME: Schéma de la table avec contrainte d'unicité
    // UNIQUE(provider, symbol, timeframe, open_time) évite les doublons
    // Permet d'utiliser INSERT OR IGNORE pour l'idempotence
    //
    // CHOIX DE CONCEPTION:
    // - provider: permet de supporter plusieurs exchanges (Binance, Kraken, etc.)
    // - open_time: timestamp en millisecondes (format Binance)
    // - Tous les prix en REAL (f64) pour la précision
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
            interpolated INTEGER NOT NULL DEFAULT 0,
            UNIQUE(provider, symbol, timeframe, open_time)
        )",
        [], // Pas de paramètres pour cette requête
    )?;

    // ALGORITHME: Table de statut pour tracker la complétion des timeframes
    // Cette table résout le problème de boucle infinie quand on atteint la limite historique
    //
    // PROBLÈME RÉSOLU:
    // Sans cette table, quand l'API Binance retourne 0 bougies (limite historique atteinte),
    // le programme continue de boucler sur le même timeframe au lieu de passer au suivant
    //
    // SOLUTION:
    // - is_complete=1: le timeframe a été entièrement récupéré jusqu'à la limite historique
    // - oldest_candle_time: timestamp de la bougie la plus ancienne récupérée
    // - last_updated: timestamp de la dernière mise à jour (pour débug/monitoring)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS timeframe_status (
            provider TEXT NOT NULL,
            symbol TEXT NOT NULL,
            timeframe TEXT NOT NULL,
            oldest_candle_time INTEGER,
            is_complete INTEGER NOT NULL DEFAULT 0,
            last_updated INTEGER NOT NULL,
            PRIMARY KEY (provider, symbol, timeframe)
        )",
        [],
    )?;

    // SUBTILITÉ RUST #12: Move sémantic
    // conn est "déplacé" (moved) dans le Ok()
    // Après cette ligne, conn n'est plus accessible dans cette fonction
    // C'est le transfert d'ownership vers l'appelant
    Ok(conn)
}

// ============================================================================
// RÉCUPÉRATION ET STOCKAGE DES DONNÉES (CŒUR DE L'ALGORITHME)
// ============================================================================

/// Récupère et stocke les bougies (candlesticks) depuis l'API Binance
///
/// ALGORITHME: Récupération par batch en remontant dans le temps
/// 1. Démarre à now() et récupère 1000 bougies vers le passé
/// 2. Identifie la bougie la plus ancienne du batch
/// 3. Répète en utilisant cette bougie comme nouvelle fin
/// 4. Continue jusqu'à atteindre start_date ou fin des données
/// 5. Comble automatiquement les trous avec interpolation linéaire
///
/// SUBTILITÉ RUST #13: Signatures de fonction avec lifetime implicites
/// &Market, &str = emprunts immutables (lecture seule)
/// &mut Connection = emprunt mutable (permet modification)
/// Les lifetimes sont inférés automatiquement par le compilateur
fn fetch_and_store_klines(
    market: &Market,
    conn: &mut Connection,
    symbol: &str,
    timeframe: &str,
    start_timestamp_ms: Option<i64>,
) -> Result<i64> {
    // Compteur de bougies insérées (incluant les interpolées)
    // i64 car peut être très grand pour des récupérations longues
    let mut total_inserted = 0i64;

    // ALGORITHME DE REPRISE: Détermination du point de départ intelligent
    //
    // Stratégie en 3 étapes:
    // 1. Vérifier si des données existent déjà pour ce (provider, symbol, timeframe)
    // 2. Si OUI → reprendre depuis la dernière bougie stockée (mode REPRISE)
    // 3. Si NON → partir de maintenant et remonter (mode PREMIÈRE EXÉCUTION)
    //
    // AVANTAGES:
    // - Évite de re-télécharger des données déjà présentes
    // - Permet de reprendre après une interruption (panne, Ctrl+C, etc.)
    // - Économise la bande passante et respecte les rate limits de l'API
    //
    // SUBTILITÉ RUST #14: Utilisation de la fonction dédiée
    let last_stored_open_time = get_last_candle_time(conn, "binance", symbol, timeframe);

    let mut end_time_ms: i64;

    match last_stored_open_time {
        Some(last_time) => {
            // MODE REPRISE: on a déjà des données
            end_time_ms = last_time;

            println!("╔════════════════════════════════════════════════════════════");
            println!("║ MODE REPRISE ACTIVÉ");
            println!("╠════════════════════════════════════════════════════════════");
            println!("║ Provider: binance");
            println!("║ Symbole: {}", symbol);
            println!("║ Timeframe: {}", timeframe);
            println!(
                "║ Dernière bougie en base: {}",
                format_timestamp_ms(last_time)
            );
            println!("║ Récupération: depuis cette date vers le passé");

            if let Some(start_ts) = start_timestamp_ms {
                println!(
                    "║ Limite de récupération: {}",
                    format_timestamp_ms(start_ts)
                );
            } else {
                println!("║ Limite de récupération: toutes les données disponibles");
            }
            println!("╚════════════════════════════════════════════════════════════\n");
        }
        None => {
            // MODE PREMIÈRE EXÉCUTION: aucune donnée existante
            end_time_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis() as i64;

            println!("╔════════════════════════════════════════════════════════════");
            println!("║ MODE PREMIÈRE EXÉCUTION");
            println!("╠════════════════════════════════════════════════════════════");
            println!("║ Provider: binance");
            println!("║ Symbole: {}", symbol);
            println!("║ Timeframe: {}", timeframe);
            println!("║ Aucune donnée existante pour cette combinaison");
            println!("║ Démarrage: {}", format_timestamp_ms(end_time_ms));

            if let Some(start_ts) = start_timestamp_ms {
                println!("║ Récupération jusqu'à: {}", format_timestamp_ms(start_ts));
            } else {
                println!("║ Récupération: toutes les données historiques disponibles");
            }
            println!("╚════════════════════════════════════════════════════════════\n");
        }
    }

    // ALGORITHME: Boucle principale de récupération
    // Continue jusqu'à ce qu'on atteigne:
    // - La date de début demandée (start_date)
    // - La fin des données disponibles sur Binance
    // - Une erreur non-récupérable
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

            // ALGORITHME: Marquer le timeframe comme complet
            // On a atteint la limite historique de l'API Binance
            // Lors de la prochaine exécution, ce timeframe sera sauté
            let oldest_time = get_last_candle_time(conn, "binance", symbol, timeframe);
            if let Err(e) = mark_timeframe_complete(conn, "binance", symbol, timeframe, oldest_time)
            {
                eprintln!(
                    "⚠️  Erreur lors du marquage du timeframe comme complet: {}",
                    e
                );
            } else {
                println!(
                    "✅ Timeframe {}/{} marqué comme complet (limite historique atteinte)",
                    symbol, timeframe
                );
            }

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
                    taker_buy_base_asset_volume, taker_buy_quote_asset_volume, interpolated
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            )?;

            for kline in &klines {
                // Kline est maintenant une structure, accédons aux champs
                // interpolated = 0 car ce sont des données réelles de l'API
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
                    0, // interpolated = 0 (données réelles)
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

        // ALGORITHME: Mettre à jour la progression du timeframe
        // Permet de tracker où on en est dans la récupération historique
        if let Err(e) =
            update_timeframe_progress(conn, "binance", symbol, timeframe, oldest_kline_time)
        {
            eprintln!("⚠️  Erreur lors de la mise à jour de la progression: {}", e);
        }

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

// ============================================================================
// INTERPOLATION LINÉAIRE DES TROUS
// ============================================================================

/// Comble les trous dans une plage de données avec interpolation linéaire
///
/// ALGORITHME D'INTERPOLATION:
/// 1. Récupère toutes les bougies existantes dans la plage [start_time, end_time]
/// 2. Parcourt les bougies paire par paire (fenêtre glissante)
/// 3. Si intervalle > intervalle_attendu → GAP détecté
/// 4. Calcule le nombre de bougies manquantes: (gap / intervalle) - 1
/// 5. Pour chaque bougie manquante:
///    - Calcule un ratio de position: i / (n+1)
///    - Interpole linéairement tous les champs: valeur = A + (B-A) × ratio
/// 6. Insère les bougies interpolées avec INSERT OR IGNORE
///
/// EXEMPLE: Gap entre t=0 (close=100) et t=1500 (close=150) avec timeframe=5m (300ms)
/// → 4 bougies manquantes (t=300, 600, 900, 1200)
/// → Bougie #1 (ratio=1/5=0.2): close = 100 + (150-100)×0.2 = 110
/// → Bougie #2 (ratio=2/5=0.4): close = 100 + (150-100)×0.4 = 120
/// → etc.
///
/// JUSTIFICATION: Pourquoi interpolation linéaire?
/// - Simple et rapide à calculer
/// - Acceptable pour petits gaps (quelques bougies)
/// - Meilleure que laisser des trous (évite erreurs dans analyses temporelles)
/// - Les données interpolées sont continues et monotones
fn fill_gaps_in_range(
    conn: &mut Connection,
    provider: &str,
    symbol: &str,
    timeframe: &str,
    start_time: i64,
    end_time: i64,
) -> Result<i64> {
    // SUBTILITÉ RUST #15: Match exhaustif avec pattern guard
    // Le _ (underscore) = catch-all pattern pour tous les autres cas
    // Ici utilisé pour les timeframes non reconnus
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

    // SUBTILITÉ RUST #16: Struct locale pour typage fort
    // On définit une struct locale plutôt que d'utiliser des tuples
    // Avantages: nommage des champs, auto-documentation, type-safety
    //
    // #[derive(Debug)] = génère automatiquement l'implémentation de Debug
    // #[allow(dead_code)] = désactive le warning pour close_time (lu mais non utilisé directement)
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
                taker_buy_base_asset_volume, taker_buy_quote_asset_volume, interpolated
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
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

                    // MARQUAGE: interpolated = 1 pour identifier les données générées
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
                        1, // interpolated = 1 (données interpolées)
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
