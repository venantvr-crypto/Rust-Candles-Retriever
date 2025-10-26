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
