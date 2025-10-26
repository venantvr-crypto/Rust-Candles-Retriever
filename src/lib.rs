/// Bibliothèque principale du projet Rust Candles Retriever
///
/// Cette bibliothèque expose tous les modules nécessaires pour récupérer,
/// stocker et interpoler des données de chandeliers depuis Binance
// Déclaration des modules publics
pub mod database;
pub mod gap_filler;
pub mod retriever;
pub mod timeframe_status;
pub mod utils;
pub mod verify;
