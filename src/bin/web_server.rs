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
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

/// État partagé de l'application
struct AppState {
    db_path: String,
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

/// GET /api/pairs - Récupère toutes les paires disponibles
#[get("/api/pairs")]
async fn get_pairs(data: web::Data<Mutex<AppState>>) -> impl Responder {
    let state = data.lock().unwrap();
    let conn = match Connection::open(&state.db_path) {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Database error: {}", e)
            }));
        }
    };

    // Récupérer toutes les paires distinctes avec leurs timeframes
    let mut stmt = match conn.prepare(
        "SELECT DISTINCT symbol, timeframe
         FROM candlesticks
         WHERE provider = 'binance'
         ORDER BY symbol, timeframe",
    ) {
        Ok(s) => s,
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Query error: {}", e)
            }));
        }
    };

    let rows = match stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    }) {
        Ok(r) => r,
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Query mapping error: {}", e)
            }));
        }
    };

    // Grouper par symbole
    let mut pairs_map: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();

    for row in rows {
        if let Ok((symbol, timeframe)) = row {
            pairs_map
                .entry(symbol)
                .or_insert_with(Vec::new)
                .push(timeframe);
        }
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
    let conn = match Connection::open(&state.db_path) {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Database error: {}", e)
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

    HttpResponse::Ok().json(candles)
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
    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| "candlesticks.db".to_string());
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .unwrap_or(8080);

    println!("🚀 Démarrage du serveur web sur http://127.0.0.1:{}", port);
    println!("📊 Base de données: {}", db_path);
    println!("📁 Fichiers statiques: ./web");

    let app_state = web::Data::new(Mutex::new(AppState { db_path }));

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
