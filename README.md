# Rust Candles Retriever

Récupère les données de chandeliers depuis Binance avec interpolation automatique des trous.

## Utilisation

```bash
# Récupérer toutes les données historiques
cargo run --release -- --symbol BTCUSDT

# Depuis une date spécifique
cargo run --release -- --symbol BTCUSDT --start-date "2024-01-01"

# Forcer le retraitement d'un timeframe complet
cargo run --release -- --symbol BTCUSDT --force

# Vérifier les données
cargo run --bin verify_data -- --symbol BTCUSDT

# Avec vérification automatique après récupération
cargo run --release -- --symbol BTCUSDT --verify
```

## Fonctionnalités

- ✅ **Mode de reprise intelligent**: Reprend automatiquement après interruption
- ✅ **Complétion des timeframes**: Évite de re-télécharger les données déjà récupérées
- ✅ **Interpolation automatique des trous** avec interpolation linéaire
- ✅ Récupération par batch de 1000 bougies
- ✅ Support multi-timeframes (5m, 15m, 30m, 1h)
- ✅ Vérification de l'espacement des données
- ✅ Distinction données réelles vs interpolées (colonne `interpolated`)
- ✅ Stockage SQLite avec provider/symbol/timeframe
- ✅ Option `--force` pour forcer le retraitement

## Complétion des Timeframes

Le système marque automatiquement un timeframe comme "complet" dans deux cas:

1. **Limite historique atteinte**: L'API Binance ne retourne plus de données (ex: août 2017 pour BTCUSDT)
2. **Date limite atteinte**: La date spécifiée par `--start-date` est atteinte

Lors des exécutions suivantes, les timeframes complets sont automatiquement sautés pour optimiser le temps et la bande passante.

Voir [TIMEFRAME_COMPLETION.md](TIMEFRAME_COMPLETION.md) pour plus de détails.
