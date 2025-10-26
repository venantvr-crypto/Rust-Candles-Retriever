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
