/// Serveur web pour visualiser les données de candlesticks
///
/// ARCHITECTURE:
/// - API REST avec actix-web
/// - Sert les fichiers statiques (HTML/CSS/JS)
/// - Endpoints:
///   - GET /api/pairs → liste des paires disponibles
///   - GET /api/candles?symbol=X&timeframe=5m&limit=1000&offset=0
use actix_cors::Cors;
use actix_files::Files;
use actix_web::{App, HttpResponse, HttpServer, Responder, get, web};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

/// État partagé de l'application
struct AppState {
    db_dir: String,
}

/// Représentation d'une bougie pour l'API
#[derive(Debug, Serialize, Deserialize)]
struct Candle {
    time: i64, // timestamp en secondes (pour Lightweight Charts)
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}

/// Paire de trading disponible
#[derive(Debug, Serialize)]
struct TradingPair {
    symbol: String,
    timeframes: Vec<String>,
}

/// Paramètres de requête pour les candles
#[derive(Debug, Deserialize)]
struct CandlesQuery {
    symbol: String,
    timeframe: String,
    limit: Option<usize>,
    offset: Option<usize>,
    start: Option<i64>, // Timestamp de début en secondes
    end: Option<i64>,   // Timestamp de fin en secondes
}

/// GET /api/pairs - Récupère toutes les paires disponibles en scannant les fichiers .db
#[get("/api/pairs")]
async fn get_pairs(data: web::Data<Mutex<AppState>>) -> impl Responder {
    let state = data.lock().unwrap();

    // Scanner tous les fichiers .db dans le répertoire
    let db_dir = std::path::Path::new(&state.db_dir);
    let entries = match std::fs::read_dir(db_dir) {
        Ok(e) => e,
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Failed to read db directory: {}", e)
            }));
        }
    };

    let mut pairs_map: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();

    // Pour chaque fichier .db
    for entry in entries.flatten() {
        let path = entry.path();

        // Vérifier que c'est un fichier .db
        if !path.is_file() {
            continue;
        }

        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };

        if !file_name.ends_with(".db") {
            continue;
        }

        // Extraire le symbole du nom de fichier (ex: BTCUSDT.db -> BTCUSDT)
        let symbol = file_name.trim_end_matches(".db").to_string();

        // Ouvrir la base de données pour récupérer les timeframes
        let conn = match Connection::open(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to open {}: {}", file_name, e);
                continue;
            }
        };

        // Récupérer les timeframes pour ce symbole
        let mut stmt = match conn.prepare(
            "SELECT DISTINCT timeframe
             FROM candlesticks
             WHERE provider = 'binance'
             ORDER BY timeframe",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to query timeframes for {}: {}", symbol, e);
                continue;
            }
        };

        let timeframes: Vec<String> = match stmt.query_map([], |row| row.get(0)) {
            Ok(rows) => rows.filter_map(Result::ok).collect(),
            Err(e) => {
                eprintln!("Failed to map timeframes for {}: {}", symbol, e);
                continue;
            }
        };

        pairs_map.insert(symbol, timeframes);
    }

    let pairs: Vec<TradingPair> = pairs_map
        .into_iter()
        .map(|(symbol, timeframes)| TradingPair { symbol, timeframes })
        .collect();

    HttpResponse::Ok().json(pairs)
}

/// GET /api/candles - Récupère les candles pour une paire/timeframe
#[get("/api/candles")]
async fn get_candles(
    data: web::Data<Mutex<AppState>>,
    query: web::Query<CandlesQuery>,
) -> impl Responder {
    let state = data.lock().unwrap();

    // Construire le chemin vers la base de données de la paire
    let db_path = format!("{}/{}.db", state.db_dir, query.symbol);

    let conn = match Connection::open(&db_path) {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Database error for {}: {}", query.symbol, e)
            }));
        }
    };

    // Construire la requête SQL selon les paramètres
    let mut sql = String::from(
        "SELECT open_time, open, high, low, close, volume
         FROM candlesticks
         WHERE provider = 'binance'
           AND symbol = ?1
           AND timeframe = ?2",
    );

    let mut param_index = 3;

    // Ajouter filtre sur start (timestamp en secondes -> convertir en ms pour la DB)
    if query.start.is_some() {
        sql.push_str(&format!(" AND open_time >= ?{}", param_index));
        param_index += 1;
    }

    // Ajouter filtre sur end
    if query.end.is_some() {
        sql.push_str(&format!(" AND open_time <= ?{}", param_index));
        param_index += 1;
    }

    sql.push_str(" ORDER BY open_time ASC"); // ASC pour avoir l'ordre chronologique direct

    // Ajouter LIMIT et OFFSET
    sql.push_str(&format!(" LIMIT ?{}", param_index));
    param_index += 1;
    sql.push_str(&format!(" OFFSET ?{}", param_index));

    let limit = query.limit.unwrap_or(2000);
    let offset = query.offset.unwrap_or(0);

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Query error: {}", e)
            }));
        }
    };

    // Construire les paramètres dynamiquement
    let mut query_params: Vec<Box<dyn rusqlite::ToSql>> = vec![
        Box::new(query.symbol.clone()),
        Box::new(query.timeframe.clone()),
    ];

    if let Some(start) = query.start {
        query_params.push(Box::new(start * 1000)); // Convertir secondes en ms
    }

    if let Some(end) = query.end {
        query_params.push(Box::new(end * 1000)); // Convertir secondes en ms
    }

    query_params.push(Box::new(limit));
    query_params.push(Box::new(offset));

    let params_refs: Vec<&dyn rusqlite::ToSql> = query_params.iter().map(|p| p.as_ref()).collect();

    let candles_iter = match stmt.query_map(params_refs.as_slice(), |row| {
        Ok(Candle {
            time: row.get::<_, i64>(0)? / 1000, // Convertir ms en secondes
            open: row.get(1)?,
            high: row.get(2)?,
            low: row.get(3)?,
            close: row.get(4)?,
            volume: row.get(5)?,
        })
    }) {
        Ok(iter) => iter,
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Query mapping error: {}", e)
            }));
        }
    };

    let mut candles: Vec<Candle> = Vec::new();
    for candle_result in candles_iter {
        if let Ok(candle) = candle_result {
            candles.push(candle);
        }
    }

    // Si aucune donnée, essayer le rééchantillonnage depuis une TF inférieure
    if candles.is_empty() {
        if let Some(smaller_tf) = find_smaller_timeframe(&conn, &query.symbol, &query.timeframe) {
            println!(
                "⚠️ Pas de données pour {} {}, rééchantillonnage depuis {}",
                query.symbol, query.timeframe, smaller_tf
            );

            candles = resample_candles(
                &conn,
                &query.symbol,
                &smaller_tf,
                &query.timeframe,
                query.start,
                query.end,
                limit,
            );
        }
    }

    HttpResponse::Ok().json(candles)
}

/// Trouve une timeframe plus petite disponible
fn find_smaller_timeframe(conn: &Connection, symbol: &str, target_tf: &str) -> Option<String> {
    let timeframes = vec![
        "5m", "15m", "30m", "1h", "2h", "4h", "6h", "8h", "12h", "1d", "3d",
    ];
    let target_seconds = parse_timeframe_seconds(target_tf);

    // Chercher la plus grande TF qui est plus petite que target
    for tf in timeframes.iter().rev() {
        let tf_seconds = parse_timeframe_seconds(tf);
        if tf_seconds < target_seconds {
            // Vérifier si cette TF a des données
            let count: Result<i64, _> = conn.query_row(
                "SELECT COUNT(*) FROM candlesticks WHERE provider = 'binance' AND symbol = ?1 AND timeframe = ?2",
                params![symbol, tf],
                |row| row.get(0),
            );

            if let Ok(n) = count {
                if n > 0 {
                    return Some(tf.to_string());
                }
            }
        }
    }

    None
}

/// Parse une timeframe en secondes
fn parse_timeframe_seconds(tf: &str) -> i64 {
    if let Some(stripped) = tf.strip_suffix('m') {
        stripped.parse::<i64>().unwrap_or(0) * 60
    } else if let Some(stripped) = tf.strip_suffix('h') {
        stripped.parse::<i64>().unwrap_or(0) * 3600
    } else if let Some(stripped) = tf.strip_suffix('d') {
        stripped.parse::<i64>().unwrap_or(0) * 86400
    } else {
        0
    }
}

/// Rééchantillonne des candles depuis une TF inférieure
fn resample_candles(
    conn: &Connection,
    symbol: &str,
    source_tf: &str,
    target_tf: &str,
    start: Option<i64>,
    end: Option<i64>,
    limit: usize,
) -> Vec<Candle> {
    // Récupérer toutes les candles source dans la plage
    let mut sql = String::from(
        "SELECT open_time, open, high, low, close, volume
         FROM candlesticks
         WHERE provider = 'binance'
           AND symbol = ?1
           AND timeframe = ?2",
    );

    let mut param_index = 3;

    if start.is_some() {
        sql.push_str(&format!(" AND open_time >= ?{}", param_index));
        param_index += 1;
    }

    if end.is_some() {
        sql.push_str(&format!(" AND open_time <= ?{}", param_index));
    }

    sql.push_str(" ORDER BY open_time ASC LIMIT 50000");

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let mut query_params: Vec<Box<dyn rusqlite::ToSql>> = vec![
        Box::new(symbol.to_string()),
        Box::new(source_tf.to_string()),
    ];

    if let Some(s) = start {
        query_params.push(Box::new(s * 1000));
    }
    if let Some(e) = end {
        query_params.push(Box::new(e * 1000));
    }

    let params_refs: Vec<&dyn rusqlite::ToSql> = query_params.iter().map(|p| p.as_ref()).collect();

    let candles_iter = match stmt.query_map(params_refs.as_slice(), |row| {
        Ok(Candle {
            time: row.get::<_, i64>(0)? / 1000,
            open: row.get(1)?,
            high: row.get(2)?,
            low: row.get(3)?,
            close: row.get(4)?,
            volume: row.get(5)?,
        })
    }) {
        Ok(iter) => iter,
        Err(_) => return vec![],
    };

    let source_candles: Vec<Candle> = candles_iter.filter_map(|r| r.ok()).collect();

    if source_candles.is_empty() {
        return vec![];
    }

    let target_seconds = parse_timeframe_seconds(target_tf);

    // Grouper par période target
    let mut resampled: Vec<Candle> = Vec::new();
    let mut current_group: Vec<&Candle> = Vec::new();
    let mut current_period_start = (source_candles[0].time / target_seconds) * target_seconds;

    for candle in &source_candles {
        let period_start = (candle.time / target_seconds) * target_seconds;

        if period_start != current_period_start {
            // Agréger le groupe précédent
            if !current_group.is_empty() {
                resampled.push(aggregate_candles(&current_group, current_period_start));
                current_group.clear();
            }
            current_period_start = period_start;
        }

        current_group.push(candle);
    }

    // Agréger le dernier groupe
    if !current_group.is_empty() {
        resampled.push(aggregate_candles(&current_group, current_period_start));
    }

    // Limiter le nombre de résultats
    resampled.truncate(limit);
    resampled
}

/// Agrège un groupe de candles en une seule
fn aggregate_candles(candles: &[&Candle], period_start: i64) -> Candle {
    let open = candles.first().unwrap().open;
    let close = candles.last().unwrap().close;
    let high = candles
        .iter()
        .map(|c| c.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let low = candles.iter().map(|c| c.low).fold(f64::INFINITY, f64::min);
    let volume = candles.iter().map(|c| c.volume).sum();

    Candle {
        time: period_start,
        open,
        high,
        low,
        close,
        volume,
    }
}

/// GET /health - Health check
#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let db_dir = std::env::var("DB_DIR").unwrap_or_else(|_| ".".to_string());
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .unwrap_or(8080);

    println!("🚀 Démarrage du serveur web sur http://127.0.0.1:{}", port);
    println!("📊 Répertoire bases de données: {}", db_dir);
    println!("📁 Fichiers statiques: ./web");

    let app_state = web::Data::new(Mutex::new(AppState { db_dir }));

    HttpServer::new(move || {
        let cors = Cors::permissive();

        App::new()
            .wrap(cors)
            .app_data(app_state.clone())
            .service(health)
            .service(get_pairs)
            .service(get_candles)
            .service(Files::new("/", "./web").index_file("index.html"))
    })
    .bind(("127.0.0.1", port))?
    .run()
    .await
}
