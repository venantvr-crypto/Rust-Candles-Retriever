# Rust Candles Retriever

Récupère les données de chandeliers depuis Binance avec interpolation automatique des trous.

## Utilisation

```bash
# Récupérer toutes les données
cargo run --bin rust_candles_retriever -- --symbol BTCUSDT

# Depuis une date spécifique
cargo run --bin rust_candles_retriever -- --symbol BTCUSDT --start-date 2024-01-01

# Vérifier les données
cargo run --bin verify_data -- --symbol BTCUSDT
```

## Fonctionnalités

- ✅ Récupération par batch de 1000 bougies
- ✅ **Interpolation automatique des trous** avec interpolation linéaire
- ✅ Vérification de l'espacement des données
- ✅ Support multi-timeframes (5m, 15m, 30m, 1h)
- ✅ Stockage SQLite avec provider/symbol/timeframe
