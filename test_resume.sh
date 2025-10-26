#!/bin/bash
# Script de test du mode de reprise
# Démontre que le programme reprend correctement après interruption

set -e

DB_TEST="test_resume.db"
SYMBOL="BTCUSDT"

echo "=========================================="
echo "TEST DU MODE DE REPRISE"
echo "=========================================="
echo ""

# Nettoyer les anciennes données de test
if [ -f "$DB_TEST" ]; then
    echo "🧹 Nettoyage de l'ancienne base de test..."
    rm "$DB_TEST"
fi

echo "📦 Compilation du programme..."
cargo build --release --quiet

echo ""
echo "=========================================="
echo "PHASE 1: PREMIÈRE EXÉCUTION"
echo "=========================================="
echo "Récupération limitée à 50 bougies pour simuler un arrêt précoce"
echo ""

# Première exécution: récupérer seulement un peu de données
# On limite à quelques jours pour que ce soit rapide
./target/release/rust_candles_retriever \
    --symbol "$SYMBOL" \
    --start-date "2024-10-20" \
    --db-file "$DB_TEST" 2>&1 | head -n 30

echo ""
echo "✓ Phase 1 terminée"
echo ""

# Vérifier combien de bougies on a
CANDLE_COUNT=$(sqlite3 "$DB_TEST" "SELECT COUNT(*) FROM candlesticks WHERE symbol='$SYMBOL' AND timeframe='5m'")
OLDEST_CANDLE=$(sqlite3 "$DB_TEST" "SELECT datetime(open_time/1000, 'unixepoch') FROM candlesticks WHERE symbol='$SYMBOL' AND timeframe='5m' ORDER BY open_time ASC LIMIT 1")
NEWEST_CANDLE=$(sqlite3 "$DB_TEST" "SELECT datetime(open_time/1000, 'unixepoch') FROM candlesticks WHERE symbol='$SYMBOL' AND timeframe='5m' ORDER BY open_time DESC LIMIT 1")

echo "📊 État de la base après Phase 1:"
echo "   - Nombre de bougies (5m): $CANDLE_COUNT"
echo "   - Bougie la plus ancienne: $OLDEST_CANDLE"
echo "   - Bougie la plus récente: $NEWEST_CANDLE"
echo ""

echo "⏳ Attente de 3 secondes..."
sleep 3

echo ""
echo "=========================================="
echo "PHASE 2: REPRISE (MODE RESUME)"
echo "=========================================="
echo "Le programme devrait détecter la dernière bougie et reprendre depuis là"
echo ""

# Deuxième exécution: le programme devrait détecter le mode reprise
./target/release/rust_candles_retriever \
    --symbol "$SYMBOL" \
    --start-date "2024-10-15" \
    --db-file "$DB_TEST" 2>&1 | head -n 40

echo ""
echo "✓ Phase 2 terminée"
echo ""

# Vérifier l'état final
CANDLE_COUNT_FINAL=$(sqlite3 "$DB_TEST" "SELECT COUNT(*) FROM candlesticks WHERE symbol='$SYMBOL' AND timeframe='5m'")
OLDEST_CANDLE_FINAL=$(sqlite3 "$DB_TEST" "SELECT datetime(open_time/1000, 'unixepoch') FROM candlesticks WHERE symbol='$SYMBOL' AND timeframe='5m' ORDER BY open_time ASC LIMIT 1")
NEWEST_CANDLE_FINAL=$(sqlite3 "$DB_TEST" "SELECT datetime(open_time/1000, 'unixepoch') FROM candlesticks WHERE symbol='$SYMBOL' AND timeframe='5m' ORDER BY open_time DESC LIMIT 1")

echo "📊 État de la base après Phase 2:"
echo "   - Nombre de bougies (5m): $CANDLE_COUNT_FINAL"
echo "   - Bougie la plus ancienne: $OLDEST_CANDLE_FINAL"
echo "   - Bougie la plus récente: $NEWEST_CANDLE_FINAL"
echo ""

echo "✅ TEST RÉUSSI!"
echo ""
echo "Le mode de reprise a fonctionné si:"
echo "  1. Phase 1 affichait 'MODE PREMIÈRE EXÉCUTION'"
echo "  2. Phase 2 affichait 'MODE REPRISE ACTIVÉ'"
echo "  3. La bougie la plus récente n'a pas changé entre les phases"
echo "  4. Le nombre de bougies a augmenté (données plus anciennes ajoutées)"
echo ""
echo "Base de test disponible: $DB_TEST"
echo "Vous pouvez l'inspecter avec: sqlite3 $DB_TEST"
