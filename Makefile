.PHONY: help build check test clean run-ada run-bnb run-btc run-sol verify-ada verify-bnb verify-btc verify-eth verify-sol verify-all web fmt clippy doc db-status db-counts db-shell migrate

# Variables
CARGO = cargo
BINARY = rust_candles_retriever
DB_DIR = .
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

# Configurations principales - Run/Debug

run-ada: ## Lance la récupération pour ADAUSDT
	@echo "$(GREEN)Lancement de la récupération pour ADAUSDT...$(NC)"
	$(CARGO) run --bin $(BINARY) -- --symbol ADAUSDT --db-dir $(DB_DIR)

run-bnb: ## Lance la récupération pour BNBUSDT
	@echo "$(GREEN)Lancement de la récupération pour BNBUSDT...$(NC)"
	$(CARGO) run --bin $(BINARY) -- --symbol BNBUSDT --db-dir $(DB_DIR)

run-btc: ## Lance la récupération pour BTCUSDT
	@echo "$(GREEN)Lancement de la récupération pour BTCUSDT...$(NC)"
	$(CARGO) run --bin $(BINARY) -- --symbol BTCUSDT --db-dir $(DB_DIR)

run-eth: ## Lance la récupération pour SOLUSDT
	@echo "$(GREEN)Lancement de la récupération pour ETHUSDT...$(NC)"
	$(CARGO) run --bin $(BINARY) -- --symbol ETHUSDT --db-dir $(DB_DIR)

run-sol: ## Lance la récupération pour SOLUSDT
	@echo "$(GREEN)Lancement de la récupération pour SOLUSDT...$(NC)"
	$(CARGO) run --bin $(BINARY) -- --symbol SOLUSDT --db-dir $(DB_DIR)

# Configurations avec date de début

run-ada-from: ## Lance ADAUSDT avec START_DATE
	@echo "$(GREEN)Lancement de la récupération pour ADAUSDT depuis $(START_DATE)...$(NC)"
	$(CARGO) run --bin $(BINARY) -- --symbol ADAUSDT --start-date $(START_DATE) --db-dir $(DB_DIR)

run-bnb-from: ## Lance BNBUSDT avec START_DATE
	@echo "$(GREEN)Lancement de la récupération pour BNBUSDT depuis $(START_DATE)...$(NC)"
	$(CARGO) run --bin $(BINARY) -- --symbol BNBUSDT --start-date $(START_DATE) --db-dir $(DB_DIR)

run-btc-from: ## Lance BTCUSDT avec START_DATE
	@echo "$(GREEN)Lancement de la récupération pour BTCUSDT depuis $(START_DATE)...$(NC)"
	$(CARGO) run --bin $(BINARY) -- --symbol BTCUSDT --start-date $(START_DATE) --db-dir $(DB_DIR)

run-eth-from: ## Lance SOLUSDT avec START_DATE
	@echo "$(GREEN)Lancement de la récupération pour ETHUSDT depuis $(START_DATE)...$(NC)"
	$(CARGO) run --bin $(BINARY) -- --symbol ETHUSDT --start-date $(START_DATE) --db-dir $(DB_DIR)

run-sol-from: ## Lance SOLUSDT avec START_DATE
	@echo "$(GREEN)Lancement de la récupération pour SOLUSDT depuis $(START_DATE)...$(NC)"
	$(CARGO) run --bin $(BINARY) -- --symbol SOLUSDT --start-date $(START_DATE) --db-dir $(DB_DIR)

# Vérification des données

verify-ada: ## Vérifie les données ADAUSDT
	@echo "$(GREEN)Vérification des données pour ADAUSDT...$(NC)"
	$(CARGO) run --bin verify_data -- --symbol ADAUSDT --db-file $(DB_DIR)/ADAUSDT.db

verify-bnb: ## Vérifie les données BNBUSDT
	@echo "$(GREEN)Vérification des données pour BNBUSDT...$(NC)"
	$(CARGO) run --bin verify_data -- --symbol BNBUSDT --db-file $(DB_DIR)/BNBUSDT.db

verify-btc: ## Vérifie les données BTCUSDT
	@echo "$(GREEN)Vérification des données pour BTCUSDT...$(NC)"
	$(CARGO) run --bin verify_data -- --symbol BTCUSDT --db-file $(DB_DIR)/BTCUSDT.db

verify-eth: ## Vérifie les données ETHUSDT
	@echo "$(GREEN)Vérification des données pour ETHUSDT...$(NC)"
	$(CARGO) run --bin verify_data -- --symbol ETHUSDT --db-file $(DB_DIR)/ETHUSDT.db

verify-sol: ## Vérifie les données SOLUSDT
	@echo "$(GREEN)Vérification des données pour SOLUSDT...$(NC)"
	$(CARGO) run --bin verify_data -- --symbol SOLUSDT --db-file $(DB_DIR)/SOLUSDT.db

verify-all: ## Vérifie toutes les paires
	@echo "$(GREEN)Vérification de toutes les paires...$(NC)"
	@$(MAKE) verify-ada
	@$(MAKE) verify-bnb
	@$(MAKE) verify-btc
	@$(MAKE) verify-eth
	@$(MAKE) verify-sol

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

db-status: ## Affiche le statut des timeframes (toutes BDD)
	@echo "$(GREEN)Statut des timeframes:$(NC)"
	@for db in $(DB_DIR)/*.db; do \
		if [ -f "$$db" ]; then \
			echo "\n=== $$db ===" && \
			sqlite3 "$$db" "SELECT provider, symbol, timeframe, datetime(oldest_time/1000, 'unixepoch') as oldest FROM timeframe_status ORDER BY timeframe;"; \
		fi \
	done

db-counts: ## Affiche le nombre de bougies par timeframe (toutes BDD)
	@echo "$(GREEN)Nombre de bougies par timeframe:$(NC)"
	@for db in $(DB_DIR)/*.db; do \
		if [ -f "$$db" ]; then \
			echo "\n=== $$db ===" && \
			sqlite3 "$$db" "SELECT symbol, timeframe, COUNT(*) as count FROM candlesticks GROUP BY symbol, timeframe ORDER BY timeframe;"; \
		fi \
	done

db-shell: ## Ouvre un shell SQLite (spécifiez SYMBOL=BTCUSDT)
	@if [ -z "$(SYMBOL)" ]; then \
		echo "$(YELLOW)Usage: make db-shell SYMBOL=BTCUSDT$(NC)"; \
		exit 1; \
	fi
	@echo "$(GREEN)Ouverture du shell SQLite pour $(SYMBOL)...$(NC)"
	@sqlite3 $(DB_DIR)/$(SYMBOL).db

# Serveur Web

web: ## Lance le serveur web (port 8080)
	@echo "$(GREEN)Arrêt du serveur web existant (si actif)...$(NC)"
	-@pkill -f web_server 2>/dev/null
	# -@fuser -k 8080/tcp 2>/dev/null
	@sleep 1
	@echo "$(GREEN)Build avec esbuild (TypeScript + PixiJS)...$(NC)"
	@cd web && npm run build
	@echo "$(GREEN)Démarrage du serveur web sur http://127.0.0.1:8080$(NC)"
	@echo "$(YELLOW)Ouvrez votre navigateur à: http://127.0.0.1:8080$(NC)"
	DB_DIR=$(DB_DIR) $(CARGO) run --bin web_server

migrate: ## Migre candlesticks.db vers des BDD par paire
	@echo "$(GREEN)Migration des données vers bases par paire...$(NC)"
	$(CARGO) run --bin migrate_to_per_pair -- --source candlesticks.db --dest-dir $(DB_DIR)
