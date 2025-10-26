# 📊 Rust Candles Retriever

Une application complète de récupération et visualisation de données de candlesticks (bougies) depuis l'API Binance.

## 🚀 Fonctionnalités

### Récupérateur de données (CLI)

- ✅ Récupération automatique des bougies depuis Binance
- ✅ Support de multiples timeframes (5m, 15m, 30m, 1h, 2h, 4h, 6h, 12h, 1d)
- ✅ Mode de reprise intelligent (continue où vous vous êtes arrêté)
- ✅ Gestion dynamique des timeframes (retire automatiquement les timeframes épuisés)
- ✅ Interpolation automatique des gaps
- ✅ Stockage SQLite avec déduplication

### 📈 Visualiseur Web (NOUVEAU!)

- Interface web interactive avec [TradingView Lightweight Charts](https://www.tradingview.com/lightweight-charts/)
- 🔍 **Zoom dynamique intelligent** : change automatiquement de timeframe selon le niveau de zoom
- 📊 Support de toutes les paires récupérées
- 🎨 Interface moderne et responsive
- ⚡ API REST haute performance

## 🎯 Utilisation

### 1. Récupération des données (CLI)

```bash
# Via Cargo
cargo run --release -- --symbol BTCUSDT --start-date "2024-01-01"

# Via Makefile (plus pratique)
make run-btc                    # Récupère BTCUSDT
make run-ada                    # Récupère ADAUSDT
make run-sol                    # Récupère SOLUSDT
make run-bnb                    # Récupère BNBUSDT

# Avec date de début spécifique
make run-btc-from START_DATE=2024-01-01

# Vérifier les données
cargo run --bin verify_data -- --symbol BTCUSDT
```

### 2. Lancement du visualiseur web 🆕

```bash
# Via Makefile (recommandé)
make web

# Ou via Cargo
DB_PATH=candlesticks.db cargo run --bin web_server
```

**Ouvrez ensuite votre navigateur à : http://127.0.0.1:8080**

## 🖼️ Interface Web

### Fonctionnalités principales

1. **Sélection de paire** : Menu déroulant avec toutes les paires disponibles
2. **Zoom intelligent** :
    - Utilisez la molette de la souris pour zoomer
    - Le timeframe s'adapte automatiquement :
        - Zoom arrière → timeframes plus larges (1h, 4h, 1d)
        - Zoom avant → timeframes plus fins (5m, 15m, 30m)
3. **Affichage temps réel** : Indicateur du timeframe actuel
4. **Compteur de bougies** : Nombre de données chargées

### API REST

L'application expose une API REST pour accéder aux données :

#### `GET /api/pairs`

Retourne toutes les paires disponibles avec leurs timeframes.

```json
[
  {
    "symbol": "BTCUSDT",
    "timeframes": ["5m", "15m", "30m", "1h", "2h", "4h", "6h", "12h", "1d"]
  }
]
```

#### `GET /api/candles?symbol=BTCUSDT&timeframe=5m&limit=1000`

Retourne les données de candlesticks.

**Paramètres:**

- `symbol` : Paire de trading (requis)
- `timeframe` : Timeframe souhaité (requis)
- `limit` : Nombre max de bougies (défaut: 1000)
- `offset` : Décalage pour pagination (défaut: 0)

```json
[
  {
    "time": 1761485700,
    "open": 113606.53,
    "high": 113639.99,
    "low": 113533.29,
    "close": 113639.98,
    "volume": 27.56421
  }
]
```

#### `GET /health`

Health check de l'API.

```json
{
  "status": "ok",
  "version": "0.1.0"
}
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

# Documentation des Commentaires Rust

Ce document liste toutes les subtilités Rust expliquées dans les commentaires du code.

## Subtilités Rust Couvertes

### Gestion de la Mémoire et Ownership

- **#5**: Ownership et clone - Move sémantic
- **#6**: Mutabilité explicite avec `mut`
- **#9**: Emprunts mutables (`&mut`) vs immutables (`&`)
- **#12**: Move sémantic lors du return

### Types et Structures

- **#2**: String vs &str (owned vs borrowed)
- **#3**: Option<T> - Alternative type-safe aux NULL
- **#4**: Result<()> - Gestion d'erreurs
- **#11**: Type alias pour éviter les conflits de noms
- **#16**: Struct locale pour typage fort
- **#19**: Vec avec types tuples

### Pattern Matching

- **#7**: Match exhaustif - sécurité à la compilation
- **#10**: if let - pattern matching simplifié
- **#15**: Match avec pattern guard et underscore
- **#20**: while let - pattern matching dans une boucle
- **#21**: Option::is_none() et méthodes helper

### Macros et Génération de Code

- **#1**: Derive macros - programmation générative
- **#14**: Closures pour callbacks
- **#23**: include! macro pour réutilisation de code

### Fonctions et Modules

- **#13**: Signatures avec lifetime implicites
- **#17**: Visibilité publique avec `pub`
- **#22**: Structure des binaires (src/bin/)

### CLI et Configurations

- **#24**: Valeurs par défaut avec clap
- **#25**: std::process::exit()

### Itération

- **#8**: Itération avec référence (&) pour éviter le move
- **#18**: Accumulateurs mutables dans les boucles

## Algorithmes Documentés

### Récupération des Données (main.rs)

- **Algorithme de récupération par batch**: Remonte dans le temps, 1000 bougies à la fois
- **Mode de reprise intelligent**: Détecte automatiquement la dernière bougie stockée et reprend depuis là
- **Fonction get_last_candle_time()**: Récupère MAX(open_time) pour un (provider, symbol, timeframe)
- **Deux modes d'exécution**:
    - PREMIÈRE EXÉCUTION: Démarre de maintenant si aucune donnée
    - MODE REPRISE: Démarre de la dernière bougie si données existantes
- **Idempotence**: INSERT OR IGNORE + contrainte UNIQUE (protection supplémentaire)

### Interpolation Linéaire (fill_gaps_in_range)

- **Détection des gaps**: Compare intervalles réels vs attendus
- **Formule d'interpolation**: `valeur = A + (B-A) × ratio`
- **Justification**: Simple, rapide, acceptable pour petits gaps

### Vérification (verify.rs)

- **Détection des anomalies**: Gaps (trop grand) vs Overlaps (trop petit)
- **Statistiques**: Nombre total, période couverte, écarts

## Choix de Conception Expliqués

1. **Mode synchrone (pas async)**: L'API binance-rs v0.21.0 est synchrone
2. **INSERT OR IGNORE**: Garantit l'idempotence des insertions
3. **Contrainte UNIQUE(provider, symbol, timeframe, open_time)**: Évite les doublons
4. **Interpolation après chaque batch**: Garantit des données continues
5. **Vec<Candle> temporaire**: Chargement en mémoire pour fenêtre glissante
6. **Transactions SQL**: Atomicité des insertions par batch

## Patterns Rust Utilisés

- **Propagation d'erreur avec `?`**: Simplifie la gestion d'erreurs
- **Pattern matching exhaustif**: Sécurité à la compilation
- **Emprunts (borrowing)**: Zero-cost abstractions
- **Lifetimes implicites**: Inférés par le compilateur
- **Derive macros**: Génération automatique de code
- **Module system**: Séparation des responsabilités

## Exemple de Lecture de Code Commenté

Voir les fichiers suivants pour des exemples complets:

- `src/main.rs` (lignes 1-500): Cœur de l'application
- `src/verify.rs`: Module de vérification
- `src/bin/verify_data.rs`: Binaire standalone
- `src/bin/test_gap_fill.rs`: Tests d'interpolation

# Gestion de la Complétion des Timeframes

## Vue d'ensemble

Le système de complétion des timeframes permet au programme de savoir quand arrêter de récupérer des données historiques pour un timeframe donné, et de ne pas
re-télécharger les mêmes données lors des exécutions suivantes.

## Deux Conditions de Complétion

Un timeframe est marqué comme **complet** dans deux situations:

### 1. Limite Historique de l'API Atteinte

**Condition**: L'API Binance retourne 0 bougies (pas de données plus anciennes disponibles)

**Exemple**: Pour BTCUSDT, les données historiques remontent à août 2017

```rust
if klines.len() == 0 {
// Marquer comme complet
mark_timeframe_complete(conn, "binance", symbol, timeframe, oldest_time);
println ! ("✅ Timeframe {}/{} marqué comme complet (limite historique atteinte)",
symbol, timeframe);
}
```

**Ce qui se passe**:

- Le programme a récupéré toutes les données disponibles sur Binance
- Lors de la prochaine exécution, ce timeframe sera automatiquement sauté
- Aucune donnée plus ancienne n'existe, donc pas besoin de re-vérifier

### 2. Date Limite Utilisateur Atteinte

**Condition**: Le programme atteint la date spécifiée par `--start-date`

**Exemple**: Avec `--start-date "2024-01-01"`, le programme s'arrête au 1er janvier 2024

```rust
if oldest_kline_time < = start_ts {
// Marquer comme complet
mark_timeframe_complete(conn, "binance", symbol, timeframe, oldest_time);
println!("✅ Timeframe {}/{} marqué comme complet (date limite atteinte)",
         symbol, timeframe);
}
```

**Ce qui se passe**:

- Le programme a récupéré jusqu'à la date demandée
- Lors de la prochaine exécution avec la même date ou sans `--start-date`, ce timeframe sera sauté
- Évite de re-télécharger les mêmes données

## Table `timeframe_status`

### Schéma

```sql
CREATE TABLE timeframe_status
(
    provider           TEXT    NOT NULL,           -- Ex: "binance"
    symbol             TEXT    NOT NULL,           -- Ex: "BTCUSDT"
    timeframe          TEXT    NOT NULL,           -- Ex: "5m"
    oldest_candle_time INTEGER,                    -- Timestamp de la plus ancienne bougie
    is_complete        INTEGER NOT NULL DEFAULT 0, -- 0=incomplet, 1=complet
    last_updated       INTEGER NOT NULL,           -- Timestamp de dernière MAJ
    PRIMARY KEY (provider, symbol, timeframe)
)
```

### Exemple de Données

| provider | symbol  | timeframe | oldest_candle_time | is_complete | last_updated  |
|----------|---------|-----------|--------------------|-------------|---------------|
| binance  | BTCUSDT | 5m        | 1502942400000      | 1           | 1698765432000 |
| binance  | BTCUSDT | 15m       | 1704067200000      | 1           | 1698765433000 |
| binance  | BTCUSDT | 30m       | NULL               | 0           | 1698765434000 |

**Interprétation**:

- **5m**: Complet jusqu'à août 2017 (limite historique Binance)
- **15m**: Complet jusqu'au 1er janvier 2024 (date limite utilisateur)
- **30m**: Incomplet, en cours de récupération

## Option `--force`

### Usage

Pour forcer le retraitement d'un timeframe déjà marqué comme complet:

```bash
cargo run --release -- --symbol BTCUSDT --force
```

### Cas d'Usage

1. **Récupérer des données plus anciennes**:
   ```bash
   # Première fois: jusqu'au 2024-01-01
   cargo run -- --symbol BTCUSDT --start-date "2024-01-01"
   # → Timeframe marqué complet

   # Plus tard: vous voulez des données plus anciennes
   cargo run -- --symbol BTCUSDT --start-date "2023-01-01" --force
   # → Retraite le timeframe depuis le début
   ```

2. **Re-vérifier après une interruption**:
   ```bash
   # Si vous suspectez des données manquantes
   cargo run -- --symbol BTCUSDT --force
   ```

3. **Re-télécharger après une erreur**:
   ```bash
   # Si un timeframe a été marqué complet par erreur
   cargo run -- --symbol BTCUSDT --force
   ```

### Comportement avec `--force`

```rust
if ! args.force & & is_timeframe_complete( & conn, "binance", & symbol, tf) {
println ! ("⏭️  Timeframe {} déjà complet. Passage au suivant.", tf);
println !("   (Utilisez --force pour forcer le retraitement)");
continue;
}

if args.force & & is_timeframe_complete( & conn, "binance", & symbol, tf) {
println ! ("🔄 Mode --force activé: retraitement du timeframe {}", tf);
}
```

## Scénarios d'Utilisation

### Scénario 1: Première Récupération Complète

```bash
# Récupérer toutes les données historiques disponibles
cargo run --release -- --symbol BTCUSDT
```

**Résultat**:

- Le programme récupère jusqu'à la limite historique (août 2017)
- Tous les timeframes (5m, 15m, 30m, 1h) sont marqués complets
- Durée: ~30-60 minutes selon la connexion

### Scénario 2: Récupération Partielle

```bash
# Récupérer seulement depuis 2024
cargo run --release -- --symbol BTCUSDT --start-date "2024-01-01"
```

**Résultat**:

- Le programme récupère jusqu'au 1er janvier 2024
- Tous les timeframes sont marqués complets
- Durée: ~5-10 minutes

**Re-exécution**:

```bash
# Le lendemain
cargo run --release -- --symbol BTCUSDT --start-date "2024-01-01"
```

→ Tous les timeframes sont sautés (déjà complets)

### Scénario 3: Extension de la Période

```bash
# Première fois: données de 2024
cargo run -- --symbol BTCUSDT --start-date "2024-01-01"
# → Timeframes marqués complets

# Plus tard: vous voulez aussi 2023
cargo run -- --symbol BTCUSDT --start-date "2023-01-01" --force
# → Retraite depuis le début jusqu'au 2023-01-01
```

### Scénario 4: Mise à Jour Quotidienne

```bash
# Script de mise à jour quotidien
# Récupère seulement les nouvelles données (depuis la dernière bougie)
cargo run --release -- --symbol BTCUSDT --symbol ETHUSDT --symbol BNBUSDT

# Si tous les timeframes sont complets, le programme termine immédiatement
# Sinon, il reprend là où il s'était arrêté (mode reprise)
```

## Réinitialisation Manuelle

Si vous voulez réinitialiser un timeframe complet:

```bash
# Via SQL
sqlite3 candlesticks.db "DELETE FROM timeframe_status WHERE symbol='BTCUSDT' AND timeframe='5m'"

# Ou réinitialiser tous les timeframes
sqlite3 candlesticks.db "DELETE FROM timeframe_status"
```

Ou utilisez simplement `--force`:

```bash
cargo run --release -- --symbol BTCUSDT --force
```

## Avantages du Système

1. **Économie de Bande Passante**
    - Ne re-télécharge jamais les mêmes données
    - Respecte les rate limits de l'API Binance

2. **Reprise Automatique**
    - En cas d'interruption (Ctrl+C, crash, panne réseau)
    - Le programme reprend exactement où il s'était arrêté

3. **Flexibilité**
    - Récupération complète ou partielle selon vos besoins
    - Option `--force` pour les cas spéciaux

4. **Optimisation du Temps**
    - Les timeframes complets sont sautés en <1ms
    - Pas de requêtes API inutiles

## Monitoring

Pour voir l'état de vos timeframes:

```bash
sqlite3 candlesticks.db "
  SELECT
    symbol,
    timeframe,
    datetime(oldest_candle_time/1000, 'unixepoch') as oldest_candle,
    CASE is_complete
      WHEN 1 THEN 'COMPLET ✅'
      ELSE 'INCOMPLET ⏳'
    END as status,
    datetime(last_updated/1000, 'unixepoch') as last_updated
  FROM timeframe_status
  ORDER BY symbol, timeframe
"
```

Exemple de sortie:

```
BTCUSDT|5m |2017-08-17 04:00:00|COMPLET ✅ |2025-10-26 12:30:00
BTCUSDT|15m|2024-01-01 00:00:00|COMPLET ✅ |2025-10-26 12:35:00
BTCUSDT|30m|2024-06-15 08:30:00|INCOMPLET ⏳|2025-10-26 12:40:00
ETHUSDT|5m |2020-03-14 00:00:00|COMPLET ✅ |2025-10-26 13:00:00
```

## Résumé

- ✅ **2 conditions de complétion**: limite historique OU date limite utilisateur
- ✅ **Timeframes complets sautés automatiquement** lors des exécutions suivantes
- ✅ **Option `--force`** pour forcer le retraitement
- ✅ **Table `timeframe_status`** pour tracker l'état de chaque timeframe
- ✅ **Optimisation automatique** de la bande passante et du temps d'exécution
