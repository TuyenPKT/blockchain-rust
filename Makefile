# Makefile — Open Consensus Execution Interface Framework
#
# Targets:
#   make build          build debug binary (native)
#   make release        build release binary (native)
#   make build-linux    cross-compile static Linux binary (musl)
#   make test           run all tests
#   make deploy         rsync source → build on VPS (default)
#   make deploy-binary  cross-compile → scp binary → VPS
#   make logs           tail pkt-sync logs trên VPS
#   make logs-api       tail blockchain-api logs trên VPS
#   make status         xem status tất cả services trên VPS
#   make sync-restart   restart pkt-sync trên VPS
#   make help           show this help

BINARY      := blockchain-rust
PKT_SERVER  ?= oceif.com
PKT_USER    ?= tuyenpkt
PKT_REMOTE  ?= ~/blockchain-rust

export PKT_SERVER
export PKT_USER
export PKT_REMOTE

.PHONY: build release build-linux test \
        deploy deploy-binary \
        logs logs-api status \
        sync-start sync-stop sync-restart \
        api-start api-stop api-restart \
        help

# ── Build ─────────────────────────────────────────────────────────────────────

build: ## Build debug binary (native)
	cargo build

release: ## Build release binary (native)
	cargo build --release

build-linux: ## Cross-compile static Linux x86_64 binary (musl)
	bash scripts/build-linux.sh

test: ## Run all tests
	cargo test

# ── Deploy ────────────────────────────────────────────────────────────────────

deploy: ## rsync source + build release trên VPS (default)
	bash scripts/deploy.sh --source

deploy-binary: ## Cross-compile local → scp binary → restart VPS services
	bash scripts/deploy.sh --binary

# ── VPS Logs ──────────────────────────────────────────────────────────────────

logs: ## Tail pkt-sync logs (Ctrl+C để dừng)
	$(_RUN) journalctl -u pkt-sync -f --no-pager

logs-api: ## Tail blockchain-api logs
	$(_RUN) journalctl -u blockchain-api -f --no-pager

logs-node: ## Tail blockchain-node logs
	$(_RUN) journalctl -u blockchain-node -f --no-pager

# ── Service Management (local hoặc remote) ────────────────────────────────────
# Nếu đang chạy trực tiếp trên VPS: make LOCAL=1 sync-restart
# Nếu chạy từ máy local:            make sync-restart

ifdef LOCAL
  _RUN =
else
  _RUN = ssh $(PKT_USER)@$(PKT_SERVER)
endif

status: ## Xem status tất cả PKT services
	$(_RUN) systemctl status pkt-sync blockchain-api blockchain-node --no-pager || true

sync-start: ## Start pkt-sync
	$(_RUN) sudo systemctl start pkt-sync

sync-stop: ## Stop pkt-sync
	$(_RUN) sudo systemctl stop pkt-sync

sync-restart: ## Restart pkt-sync
	$(_RUN) sudo systemctl restart pkt-sync

api-start: ## Start blockchain-api
	$(_RUN) sudo systemctl start blockchain-api

api-stop: ## Stop blockchain-api
	$(_RUN) sudo systemctl stop blockchain-api

api-restart: ## Restart blockchain-api
	$(_RUN) sudo systemctl restart blockchain-api

# ── Help ──────────────────────────────────────────────────────────────────────

help: ## Show this help
	@echo ""
	@echo "OCEIF — Make targets"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
	    | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}'
	@echo ""
	@echo "Config (override via env):"
	@echo "  PKT_SERVER=$(PKT_SERVER)  PKT_USER=$(PKT_USER)  PKT_REMOTE=$(PKT_REMOTE)"
	@echo ""

.DEFAULT_GOAL := help
