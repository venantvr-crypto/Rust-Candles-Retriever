/// Module de gestion de la base de données SQLite
///
/// Ce module fournit une structure DatabaseManager pour encapsuler
/// toutes les opérations liées à la base de données
use anyhow::Result;
use rusqlite::{Connection, Result as SqlResult};
use std::path::Path;

/// Schéma SQL pour la table candlesticks
///
/// Centralisé pour éviter la duplication dans tous les tests et binaires
pub const SQL_CREATE_TABLE_CANDLESTICKS: &str =
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
    )";

/// Index pour requêtes sur candlesticks
pub const SQL_CREATE_INDEX_CANDLESTICKS: &str =
    "CREATE INDEX IF NOT EXISTS idx_candles_query
        ON candlesticks (provider, symbol, timeframe, open_time)";

/// Schéma SQL pour la table rsi_values
pub const SQL_CREATE_TABLE_RSI: &str =
    "CREATE TABLE IF NOT EXISTS rsi_values (
        provider TEXT NOT NULL,
        symbol TEXT NOT NULL,
        timeframe TEXT NOT NULL,
        period INTEGER NOT NULL,
        open_time INTEGER NOT NULL,
        rsi_value REAL NOT NULL,
        UNIQUE(provider, symbol, timeframe, period, open_time)
    )";

/// Index pour requêtes sur rsi_values
pub const SQL_CREATE_INDEX_RSI: &str =
    "CREATE INDEX IF NOT EXISTS idx_rsi_query
        ON rsi_values (provider, symbol, timeframe, period, open_time)";

/// Gestionnaire de la base de données SQLite
///
/// ARCHITECTURE:
/// Cette structure encapsule la connexion SQLite et fournit des méthodes
/// pour initialiser le schéma et gérer la connexion
pub struct DatabaseManager {
    conn: Connection,
}

impl DatabaseManager {
    /// Crée et initialise une nouvelle connexion à la base de données
    ///
    /// ALGORITHME:
    /// 1. Ouvre la connexion SQLite
    /// 2. Crée la table candlesticks si elle n'existe pas
    /// 3. Crée la table timeframe_status si elle n'existe pas
    ///
    /// SUBTILITÉ RUST: Pattern builder avec Self
    /// Self est un alias pour DatabaseManager dans ce contexte
    pub fn new(db_file: &str) -> Result<Self> {
        let path = Path::new(db_file);
        let conn = Connection::open(path)?;

        // Initialiser le schéma
        Self::init_schema(&conn)?;

        Ok(DatabaseManager { conn })
    }

    /// Initialise le schéma de la base de données
    ///
    /// DESIGN: Méthode privée, appelée uniquement depuis new()
    fn init_schema(conn: &Connection) -> SqlResult<()> {
        // Table principale des bougies
        conn.execute(SQL_CREATE_TABLE_CANDLESTICKS, [])?;
        conn.execute(SQL_CREATE_INDEX_CANDLESTICKS, [])?;

        // Table de statut des timeframes (pour monitoring uniquement)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS timeframe_status (
                provider TEXT NOT NULL,
                symbol TEXT NOT NULL,
                timeframe TEXT NOT NULL,
                oldest_candle_time INTEGER,
                last_updated INTEGER NOT NULL,
                PRIMARY KEY (provider, symbol, timeframe)
            )",
            [],
        )?;

        // Table des indicateurs RSI pré-calculés
        conn.execute(SQL_CREATE_TABLE_RSI, [])?;
        conn.execute(SQL_CREATE_INDEX_RSI, [])?;

        Ok(())
    }

    /// Retourne une référence à la connexion SQLite
    ///
    /// SUBTILITÉ RUST: Retourne une référence (&) pour permettre
    /// l'emprunt de la connexion sans transférer l'ownership
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Retourne une référence mutable à la connexion SQLite
    ///
    /// SUBTILITÉ RUST: &mut permet de modifier la connexion
    /// (nécessaire pour les transactions, par exemple)
    pub fn connection_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}
