#!/bin/bash

echo "=== Test d'interpolation ==="
echo "Suppression de la base de données..."
rm -f candlesticks.db

echo ""
echo "Récupération de données historiques avec des trous connus (février 2020)..."
timeout 60 cargo run --bin rust_candles_retriever -- --symbol BTCUSDT --start-date 2020-02-05 2>&1 | grep -E "(Récupération|Batch|interpolées|Terminé|Gap)"

echo ""
echo "=== Vérification des données ==="
cargo run --bin verify_data -- --symbol BTCUSDT --timeframes 5m 2>&1
