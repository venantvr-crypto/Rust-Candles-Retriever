.PHONY: help build check test clean run-ada run-bnb run-btc run-sol verify-data

# Variables
CARGO = cargo
BINARY = rust_candles_retriever
DB_FILE = candlesticks.db
START_DATE = 2024-01-01

# Couleurs pour l'output
GREEN = \033[0;32m
YELLOW = \033[0;33m
NC = \033[0m # No Color

help: ## Affiche cette aide
	@echo "$(GREEN)Makefile pour Rust Candles Retriever$(NC)"
	@echo ""
	@echo "Cibles disponibles:"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  $(YELLOW)%-15s$(NC) %s\n", $$1, $$2}'

build: ## Compile le projet
	@echo "$(GREEN)Compilation du projet...$(NC)"
	$(CARGO) build --release

check: ## Vérifie le code sans compiler
	@echo "$(GREEN)Vérification du code...$(NC)"
	$(CARGO) check

clean: ## Nettoie les fichiers générés
	@echo "$(GREEN)Nettoyage...$(NC)"
	$(CARGO) clean
	rm -f $(DB_FILE)

# Configurations principales - Run/Debug

run-ada: ## Lance la récupération pour ADAUSDT
	@echo "$(GREEN)Lancement de la récupération pour ADAUSDT...$(NC)"
	$(CARGO) run --bin $(BINARY) -- --symbol ADAUSDT --db-file $(DB_FILE)

run-bnb: ## Lance la récupération pour BNBUSDT
	@echo "$(GREEN)Lancement de la récupération pour BNBUSDT...$(NC)"
	$(CARGO) run --bin $(BINARY) -- --symbol BNBUSDT --db-file $(DB_FILE)

run-btc: ## Lance la récupération pour BTCUSDT
	@echo "$(GREEN)Lancement de la récupération pour BTCUSDT...$(NC)"
	$(CARGO) run --bin $(BINARY) -- --symbol BTCUSDT --db-file $(DB_FILE)

run-sol: ## Lance la récupération pour SOLUSDT
	@echo "$(GREEN)Lancement de la récupération pour SOLUSDT...$(NC)"
	$(CARGO) run --bin $(BINARY) -- --symbol SOLUSDT --db-file $(DB_FILE)

# Configurations avec date de début

run-ada-from: ## Lance ADAUSDT avec START_DATE
	@echo "$(GREEN)Lancement de la récupération pour ADAUSDT depuis $(START_DATE)...$(NC)"
	$(CARGO) run --bin $(BINARY) -- --symbol ADAUSDT --start-date $(START_DATE) --db-file $(DB_FILE)

run-bnb-from: ## Lance BNBUSDT avec START_DATE
	@echo "$(GREEN)Lancement de la récupération pour BNBUSDT depuis $(START_DATE)...$(NC)"
	$(CARGO) run --bin $(BINARY) -- --symbol BNBUSDT --start-date $(START_DATE) --db-file $(DB_FILE)

run-btc-from: ## Lance BTCUSDT avec START_DATE
	@echo "$(GREEN)Lancement de la récupération pour BTCUSDT depuis $(START_DATE)...$(NC)"
	$(CARGO) run --bin $(BINARY) -- --symbol BTCUSDT --start-date $(START_DATE) --db-file $(DB_FILE)

run-sol-from: ## Lance SOLUSDT avec START_DATE
	@echo "$(GREEN)Lancement de la récupération pour SOLUSDT depuis $(START_DATE)...$(NC)"
	$(CARGO) run --bin $(BINARY) -- --symbol SOLUSDT --start-date $(START_DATE) --db-file $(DB_FILE)

# Développement

fmt: ## Formate le code
	@echo "$(GREEN)Formatage du code...$(NC)"
	$(CARGO) fmt

clippy: ## Lance clippy (linter)
	@echo "$(GREEN)Analyse avec clippy...$(NC)"
	$(CARGO) clippy -- -D warnings

doc: ## Génère la documentation
	@echo "$(GREEN)Génération de la documentation...$(NC)"
	$(CARGO) doc --open

# Inspection de la base de données

db-status: ## Affiche le statut des timeframes
	@echo "$(GREEN)Statut des timeframes:$(NC)"
	@sqlite3 $(DB_FILE) "SELECT provider, symbol, timeframe, datetime(oldest_candle_time/1000, 'unixepoch') as oldest FROM timeframe_status ORDER BY symbol, timeframe;"

db-counts: ## Affiche le nombre de bougies par timeframe
	@echo "$(GREEN)Nombre de bougies par timeframe:$(NC)"
	@sqlite3 $(DB_FILE) "SELECT symbol, timeframe, COUNT(*) as count FROM candlesticks GROUP BY symbol, timeframe ORDER BY symbol, timeframe;"

db-shell: ## Ouvre un shell SQLite
	@echo "$(GREEN)Ouverture du shell SQLite...$(NC)"
	@sqlite3 $(DB_FILE)
