/// Module de backfill automatique des chandelles
///
/// Contient la logique métier pour récupérer les chandelles manquantes
/// en remontant dans le temps jusqu'à combler tous les gaps
use anyhow::Result;
use binance::api::*;
use binance::market::*;
use chrono::{DateTime, NaiveDateTime, Utc};
use futures_util::future;

use crate::{database::DatabaseManager, retriever::CandleRetriever};

/// Options de configuration pour le backfill
#[derive(Debug, Clone)]
pub struct BackfillOptions {
    /// Le symbole/paire de trading à récupérer (ex: BTCUSDT)
    pub symbol: String,
    /// Date de début au format timestamp millisecondes (optionnel)
    pub start_timestamp_ms: Option<i64>,
    /// Répertoire des bases de données
    pub db_dir: String,
    /// Timeframes à récupérer (par défaut tous)
    pub timeframes: Option<Vec<String>>,
}

impl BackfillOptions {
    /// Crée des options de backfill avec les valeurs par défaut
    pub fn new(symbol: String, db_dir: String) -> Self {
        Self {
            symbol,
            start_timestamp_ms: None,
            db_dir,
            timeframes: None,
        }
    }

    /// Définit la date de début à partir d'une chaîne YYYY-MM-DD
    pub fn with_start_date(mut self, date_str: &str) -> Result<Self> {
        self.start_timestamp_ms = Some(parse_start_date(Some(date_str))?);
        Ok(self)
    }

    /// Définit le timestamp de début en millisecondes
    pub fn with_start_timestamp(mut self, timestamp_ms: i64) -> Self {
        self.start_timestamp_ms = Some(timestamp_ms);
        self
    }

    /// Définit les timeframes spécifiques à récupérer
    pub fn with_timeframes(mut self, timeframes: Vec<String>) -> Self {
        self.timeframes = Some(timeframes);
        self
    }
}

/// Exécute le backfill pour une paire de trading
///
/// Cette fonction récupère les chandelles manquantes en remontant dans le temps
/// pour tous les timeframes spécifiés jusqu'à ce que tous soient complets.
pub async fn run_backfill(options: BackfillOptions) -> Result<()> {
    let symbol = options.symbol.to_uppercase();
    println!("🔄 Démarrage backfill pour: {}", symbol);

    // Créer le nom de fichier basé sur le symbole
    let db_file = format!("{}/{}.db", options.db_dir, symbol);

    // Initialiser la base de données
    let db = DatabaseManager::new(&db_file)?;
    println!("  ✓ Base de données: {}", db_file);
    drop(db);

    // Timeframes à récupérer
    let mut active_timeframes: Vec<String> = options.timeframes.unwrap_or_else(|| {
        vec![
            "3m", "5m", "15m", "30m", "1h", "2h", "4h", "6h", "8h", "12h", "1d", "3d",
        ]
        .into_iter()
        .map(|s| s.to_string())
        .collect()
    });

    let start_timestamp_ms = options.start_timestamp_ms;

    // Boucle principale: traiter tous les timeframes en parallèle
    let mut iteration = 0;
    loop {
        iteration += 1;
        println!("  ═══ Itération #{} ═══", iteration);
        println!("  Timeframes actifs: {:?}", active_timeframes);

        if active_timeframes.is_empty() {
            println!("  ✅ Tous les timeframes traités pour {}!", symbol);
            break;
        }

        // Créer une tâche pour chaque timeframe
        let mut tasks = Vec::new();

        for tf in active_timeframes.clone() {
            let symbol_clone = symbol.clone();
            let db_file_clone = db_file.clone();

            let task = tokio::task::spawn_blocking(move || {
                let mut db = match DatabaseManager::new(&db_file_clone) {
                    Ok(db) => db,
                    Err(e) => return (tf.clone(), Err(anyhow::anyhow!("DB error: {}", e))),
                };

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
                Ok((tf, fetch_result)) => match fetch_result {
                    Ok((inserted, is_exhausted)) => {
                        if inserted > 0 {
                            println!("    ✓ {} : {} nouvelles bougies", tf, inserted);
                        }

                        if is_exhausted || inserted == 0 {
                            if is_exhausted {
                                println!("    🏁 {} épuisé (date limite)", tf);
                            } else {
                                println!("    🏁 {} épuisé (plus de données)", tf);
                            }
                            exhausted_timeframes.push(tf);
                        }
                    }
                    Err(e) => {
                        eprintln!("    ⚠ {} : Erreur: {}", tf, e);
                    }
                },
                Err(e) => {
                    eprintln!("    ⚠ Erreur de tâche: {}", e);
                }
            }
        }

        // Retirer les timeframes épuisés
        active_timeframes.retain(|tf| !exhausted_timeframes.contains(tf));

        // Pause pour respecter les rate limits
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }

    println!("✅ Backfill terminé pour {}", symbol);
    Ok(())
}

/// Parse une date au format YYYY-MM-DD en timestamp millisecondes
fn parse_start_date(date_str: Option<&str>) -> Result<i64> {
    match date_str {
        Some(date) => {
            let naive_date = NaiveDateTime::parse_from_str(
                &(date.to_string() + " 00:00:00"),
                "%Y-%m-%d %H:%M:%S",
            )?;
            let datetime_utc = DateTime::<Utc>::from_naive_utc_and_offset(naive_date, Utc);
            Ok(datetime_utc.timestamp_millis())
        }
        None => Err(anyhow::anyhow!("Date string is required")),
    }
}
