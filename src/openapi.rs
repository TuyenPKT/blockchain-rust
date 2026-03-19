#![allow(dead_code)]
//! v9.8 — OpenAPI 3.0 Spec (GET /api/openapi.json)
//!
//! Tự động generate OpenAPI 3.0.3 spec JSON cho tất cả PKTScan endpoints.
//! Không dùng macro hay external derive — spec được build thủ công qua `build_spec()`.
//!
//! Endpoint:
//!   GET /api/openapi.json  → OpenAPI 3.0.3 JSON document
//!
//! Usage:
//!   let app = router.merge(openapi::openapi_router());

use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn openapi_router() -> Router {
    Router::new()
        .route("/api/openapi.json", get(get_openapi))
}

async fn get_openapi() -> Json<Value> {
    Json(build_spec())
}

// ─── Spec Builder ─────────────────────────────────────────────────────────────

/// Build OpenAPI 3.0.3 spec cho PKTScan API.
pub fn build_spec() -> Value {
    json!({
        "openapi": "3.0.3",
        "info": {
            "title":       "PKTScan API",
            "version":     "9.8",
            "description": "PKTScan blockchain explorer REST API — read-only, Zero-Trust middleware, CORS allowlist."
        },
        "servers": [
            { "url": "http://localhost:8080", "description": "Local dev" },
            { "url": "https://pktscan.io",   "description": "Production" }
        ],
        "paths": build_paths(),
        "components": {
            "schemas":    build_schemas(),
            "parameters": build_common_params(),
        }
    })
}

// ─── Paths ────────────────────────────────────────────────────────────────────

/// Build paths object bằng cách insert từng path vào Map (tránh recursion limit của json!).
fn build_paths() -> Value {
    let mut m = serde_json::Map::new();

    // ── Core chain ────────────────────────────────────────────────────────
    m.insert("/api/stats".into(), json!({
        "get": { "summary": "Network stats", "tags": ["chain"],
                 "responses": { "200": ok_json("NetworkStats") } }
    }));
    m.insert("/api/blocks".into(), json!({
        "get": {
            "summary": "List blocks (newest first)", "tags": ["chain"],
            "parameters": [
                qparam("limit",  "integer", "Max results (default 20, max 100)", false),
                qparam("offset", "integer", "Offset for pagination",              false),
                qparam("from",   "integer", "Cursor: start from block height",    false),
            ],
            "responses": { "200": ok_json("BlockList") }
        }
    }));
    m.insert("/api/block/{height}".into(), json!({
        "get": {
            "summary": "Block detail", "tags": ["chain"],
            "parameters": [ pparam("height", "integer", "Block height") ],
            "responses": { "200": ok_json("BlockDetail"), "404": err_resp("Block not found") }
        }
    }));
    m.insert("/api/txs".into(), json!({
        "get": {
            "summary": "List transactions with optional filters", "tags": ["chain"],
            "parameters": [
                qparam("limit",      "integer", "Max results (default 20, max 100)",         false),
                qparam("offset",     "integer", "Offset for pagination",                      false),
                qparam("from",       "integer", "Cursor: start from block height",            false),
                qparam("min_amount", "integer", "Filter: output_total >= min_amount (sats)",  false),
                qparam("max_amount", "integer", "Filter: output_total <= max_amount (sats)",  false),
                qparam("since",      "integer", "Filter: block_timestamp >= since (unix s)",  false),
                qparam("until",      "integer", "Filter: block_timestamp <= until (unix s)",  false),
            ],
            "responses": { "200": ok_json("TxList") }
        }
    }));
    m.insert("/api/tx/{txid}".into(), json!({
        "get": {
            "summary": "Transaction detail + status/confirmations", "tags": ["chain"],
            "parameters": [ pparam("txid", "string", "Transaction ID (hex)") ],
            "responses": { "200": ok_json("TxDetail"), "404": err_resp("Transaction not found") }
        }
    }));
    m.insert("/api/address/{addr}".into(), json!({
        "get": {
            "summary": "Address balance + UTXOs + tx history", "tags": ["chain"],
            "parameters": [ pparam("addr", "string", "Address (hex pubkey hash or P2TR xonly)") ],
            "responses": { "200": ok_json("AddressDetail") }
        }
    }));
    m.insert("/api/mempool".into(), json!({
        "get": { "summary": "Pending transactions + fee stats", "tags": ["chain"],
                 "responses": { "200": ok_json("MempoolStats") } }
    }));
    m.insert("/api/search".into(), json!({
        "get": {
            "summary": "Search blocks / txs / addresses", "tags": ["chain"],
            "parameters": [
                qparam("q",     "string",  "Search query (hash prefix, address, height)", true),
                qparam("limit", "integer", "Max results per category (default 10, max 50)", false),
            ],
            "responses": { "200": ok_json("SearchResults") }
        }
    }));
    m.insert("/api/analytics/{metric}".into(), json!({
        "get": {
            "summary": "Chain analytics time series", "tags": ["chain"],
            "parameters": [
                pparam("metric", "string",  "block_time | hashrate | fee_market | difficulty | tx_throughput"),
                qparam("window", "integer", "Number of recent blocks (default 100)", false),
            ],
            "responses": { "200": ok_json("AnalyticsSeries"), "404": err_resp("Unknown metric") }
        }
    }));

    // ── Export ────────────────────────────────────────────────────────────
    let csv_resp = json!({ "description": "CSV file", "content": { "text/csv": { "schema": { "type": "string" } } } });
    m.insert("/api/blocks.csv".into(), json!({
        "get": { "summary": "CSV export of blocks", "tags": ["export"],
                 "parameters": [ qparam("limit","integer","Max rows",false), qparam("from","integer","Cursor",false) ],
                 "responses": { "200": csv_resp.clone() } }
    }));
    m.insert("/api/txs.csv".into(), json!({
        "get": { "summary": "CSV export of transactions", "tags": ["export"],
                 "parameters": [ qparam("limit","integer","Max rows",false), qparam("from","integer","Cursor",false) ],
                 "responses": { "200": csv_resp } }
    }));

    // ── Mining Pool ────────────────────────────────────────────────────────
    m.insert("/api/pool/stats".into(), json!({
        "get": { "summary": "Mining pool aggregate stats", "tags": ["pool"],
                 "responses": { "200": ok_json("PoolStats") } }
    }));
    m.insert("/api/pool/miners".into(), json!({
        "get": { "summary": "Per-miner share breakdown", "tags": ["pool"],
                 "responses": { "200": ok_json("MinerList") } }
    }));

    // ── Tokens ────────────────────────────────────────────────────────────
    m.insert("/api/tokens".into(), json!({
        "get": { "summary": "List all tokens", "tags": ["tokens"],
                 "responses": { "200": ok_json("TokenList") } }
    }));
    m.insert("/api/token/{id}".into(), json!({
        "get": {
            "summary": "Token detail", "tags": ["tokens"],
            "parameters": [ pparam("id", "string", "Token symbol / ID") ],
            "responses": { "200": ok_json("TokenDetail"), "404": err_resp("Token not found") }
        }
    }));
    m.insert("/api/token/{id}/holders".into(), json!({
        "get": {
            "summary": "Token holder list", "tags": ["tokens"],
            "parameters": [ pparam("id","string","Token symbol / ID"), qparam("limit","integer","Max holders",false) ],
            "responses": { "200": ok_json("TokenHolders") }
        }
    }));
    m.insert("/api/token/{id}/balance/{addr}".into(), json!({
        "get": {
            "summary": "Token balance for an address", "tags": ["tokens"],
            "parameters": [ pparam("id","string","Token ID"), pparam("addr","string","Holder address") ],
            "responses": { "200": ok_json("TokenBalance") }
        }
    }));

    // ── Smart Contracts ────────────────────────────────────────────────────
    m.insert("/api/contracts".into(), json!({
        "get": { "summary": "List all deployed contracts", "tags": ["contracts"],
                 "responses": { "200": ok_json("ContractList") } }
    }));
    m.insert("/api/contract/{addr}".into(), json!({
        "get": {
            "summary": "Contract detail + function list", "tags": ["contracts"],
            "parameters": [ pparam("addr","string","Contract address (0x...)") ],
            "responses": { "200": ok_json("ContractDetail"), "404": err_resp("Contract not found") }
        }
    }));
    m.insert("/api/contract/{addr}/state".into(), json!({
        "get": {
            "summary": "Full contract storage snapshot", "tags": ["contracts"],
            "parameters": [ pparam("addr","string","Contract address") ],
            "responses": { "200": ok_json("ContractState"), "404": err_resp("Contract not found") }
        }
    }));
    m.insert("/api/contract/{addr}/state/{key}".into(), json!({
        "get": {
            "summary": "Single contract storage value", "tags": ["contracts"],
            "parameters": [ pparam("addr","string","Contract address"), pparam("key","string","Storage key") ],
            "responses": { "200": ok_json("StorageValue"), "404": err_resp("Contract not found") }
        }
    }));

    // ── Staking ────────────────────────────────────────────────────────────
    m.insert("/api/staking/stats".into(), json!({
        "get": { "summary": "Staking pool aggregate stats", "tags": ["staking"],
                 "responses": { "200": ok_json("StakingStats") } }
    }));
    m.insert("/api/staking/validators".into(), json!({
        "get": { "summary": "List all validators (sorted by stake desc)", "tags": ["staking"],
                 "responses": { "200": ok_json("ValidatorList") } }
    }));
    m.insert("/api/staking/validator/{addr}".into(), json!({
        "get": {
            "summary": "Validator detail + delegator list", "tags": ["staking"],
            "parameters": [ pparam("addr","string","Validator address") ],
            "responses": { "200": ok_json("ValidatorDetail"), "404": err_resp("Validator not found") }
        }
    }));
    m.insert("/api/staking/delegator/{addr}".into(), json!({
        "get": {
            "summary": "All stakes + pending rewards for a delegator", "tags": ["staking"],
            "parameters": [ pparam("addr","string","Delegator address") ],
            "responses": { "200": ok_json("DelegatorDetail") }
        }
    }));

    // ── DeFi ───────────────────────────────────────────────────────────────
    m.insert("/api/defi/feeds".into(), json!({
        "get": { "summary": "List all oracle price feeds", "tags": ["defi"],
                 "responses": { "200": ok_json("FeedList") } }
    }));
    m.insert("/api/defi/feed/{id}".into(), json!({
        "get": {
            "summary": "Oracle feed detail (latest price + metadata)", "tags": ["defi"],
            "parameters": [ pparam("id","string","Feed ID (e.g. BTC/USD)") ],
            "responses": { "200": ok_json("FeedDetail"), "404": err_resp("Feed not found") }
        }
    }));
    m.insert("/api/defi/feed/{id}/history".into(), json!({
        "get": {
            "summary": "Price history for a feed", "tags": ["defi"],
            "parameters": [ pparam("id","string","Feed ID") ],
            "responses": { "200": ok_json("FeedHistory") }
        }
    }));
    m.insert("/api/defi/loans".into(), json!({
        "get": { "summary": "List all active loans", "tags": ["defi"],
                 "responses": { "200": ok_json("LoanList") } }
    }));
    m.insert("/api/defi/loans/liquidatable".into(), json!({
        "get": { "summary": "List loans below minimum collateral ratio", "tags": ["defi"],
                 "responses": { "200": ok_json("LiquidatableLoans") } }
    }));

    // ── Address Labels ─────────────────────────────────────────────────────
    m.insert("/api/labels".into(), json!({
        "get": { "summary": "List all labeled addresses", "tags": ["labels"],
                 "responses": { "200": ok_json("LabelList") } }
    }));
    m.insert("/api/label/{addr}".into(), json!({
        "get": {
            "summary": "Label for a specific address", "tags": ["labels"],
            "parameters": [ pparam("addr","string","Blockchain address") ],
            "responses": { "200": ok_json("AddressLabel"), "404": err_resp("Address has no label") }
        }
    }));
    m.insert("/api/labels/category/{cat}".into(), json!({
        "get": {
            "summary": "Filter labeled addresses by category", "tags": ["labels"],
            "parameters": [ pparam("cat","string","Category (exchange, foundation, miner, contract, ...)") ],
            "responses": { "200": ok_json("LabelList") }
        }
    }));

    // ── OpenAPI self-reference ─────────────────────────────────────────────
    m.insert("/api/openapi.json".into(), json!({
        "get": { "summary": "This OpenAPI 3.0.3 spec", "tags": ["meta"],
                 "responses": { "200": { "description": "OpenAPI document",
                     "content": { "application/json": { "schema": { "type": "object" } } } } } }
    }));

    Value::Object(m)
}

// ─── Schemas ──────────────────────────────────────────────────────────────────

fn build_schemas() -> Value {
    json!({
        "Error": {
            "type": "object",
            "properties": {
                "error": { "type": "string" }
            }
        },
        "NetworkStats": {
            "type": "object",
            "properties": {
                "height":           { "type": "integer" },
                "difficulty":       { "type": "integer" },
                "hashrate":         { "type": "integer" },
                "block_reward":     { "type": "integer" },
                "total_supply":     { "type": "integer" },
                "utxo_count":       { "type": "integer" },
                "mempool_count":    { "type": "integer" },
                "avg_block_time_s": { "type": "number"  },
                "block_count":      { "type": "integer" }
            }
        },
        "TxDetail": {
            "type": "object",
            "properties": {
                "tx_id":         { "type": "string"  },
                "wtx_id":        { "type": "string"  },
                "is_coinbase":   { "type": "boolean" },
                "fee":           { "type": "integer" },
                "output_total":  { "type": "integer" },
                "block_height":  { "type": "integer", "nullable": true },
                "block_hash":    { "type": "string",  "nullable": true },
                "timestamp":     { "type": "integer", "nullable": true },
                "status":        { "type": "string", "enum": ["confirmed", "pending"] },
                "confirmations": { "type": "integer" }
            }
        },
        "AddressLabel": {
            "type": "object",
            "properties": {
                "address":  { "type": "string" },
                "label":    { "type": "string" },
                "category": { "type": "string" },
                "note":     { "type": "string" }
            }
        }
    })
}

// ─── Common parameters ────────────────────────────────────────────────────────

fn build_common_params() -> Value {
    json!({
        "Limit": qparam("limit",  "integer", "Max results per page (default 20, max 100)", false),
        "Offset": qparam("offset", "integer", "Skip N results",                             false),
        "From":   qparam("from",   "integer", "Cursor: start from block height",            false),
    })
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Inline query parameter object.
fn qparam(name: &str, schema_type: &str, description: &str, required: bool) -> Value {
    json!({
        "name":        name,
        "in":          "query",
        "required":    required,
        "description": description,
        "schema":      { "type": schema_type }
    })
}

/// Inline path parameter object.
fn pparam(name: &str, schema_type: &str, description: &str) -> Value {
    json!({
        "name":        name,
        "in":          "path",
        "required":    true,
        "description": description,
        "schema":      { "type": schema_type }
    })
}

/// 200 OK with JSON content referencing a schema.
fn ok_json(schema_ref: &str) -> Value {
    json!({
        "description": "Success",
        "content": {
            "application/json": {
                "schema": { "$ref": format!("#/components/schemas/{}", schema_ref) }
            }
        }
    })
}

/// Generic error response.
fn err_resp(description: &str) -> Value {
    json!({
        "description": description,
        "content": {
            "application/json": {
                "schema": { "$ref": "#/components/schemas/Error" }
            }
        }
    })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Top-level structure ────────────────────────────────────────────────

    #[test]
    fn test_spec_openapi_version() {
        let s = build_spec();
        assert_eq!(s["openapi"].as_str(), Some("3.0.3"));
    }

    #[test]
    fn test_spec_info_title() {
        let s = build_spec();
        assert_eq!(s["info"]["title"].as_str(), Some("PKTScan API"));
    }

    #[test]
    fn test_spec_info_version() {
        let s = build_spec();
        assert!(s["info"]["version"].is_string());
    }

    #[test]
    fn test_spec_info_description_present() {
        let s = build_spec();
        assert!(s["info"]["description"].is_string());
    }

    #[test]
    fn test_spec_servers_not_empty() {
        let s = build_spec();
        let servers = s["servers"].as_array().unwrap();
        assert!(!servers.is_empty());
    }

    #[test]
    fn test_spec_paths_not_empty() {
        let s = build_spec();
        let paths = s["paths"].as_object().unwrap();
        assert!(!paths.is_empty());
    }

    #[test]
    fn test_spec_path_count_at_least_30() {
        let s = build_spec();
        let paths = s["paths"].as_object().unwrap();
        assert!(paths.len() >= 30);
    }

    // ── Core chain paths ──────────────────────────────────────────────────

    #[test]
    fn test_spec_has_stats_path() {
        let s = build_spec();
        assert!(s["paths"]["/api/stats"]["get"].is_object());
    }

    #[test]
    fn test_spec_has_blocks_path() {
        let s = build_spec();
        assert!(s["paths"]["/api/blocks"]["get"].is_object());
    }

    #[test]
    fn test_spec_has_txs_path() {
        let s = build_spec();
        assert!(s["paths"]["/api/txs"]["get"].is_object());
    }

    #[test]
    fn test_spec_has_tx_path() {
        let s = build_spec();
        assert!(s["paths"]["/api/tx/{txid}"]["get"].is_object());
    }

    #[test]
    fn test_spec_has_address_path() {
        let s = build_spec();
        assert!(s["paths"]["/api/address/{addr}"]["get"].is_object());
    }

    #[test]
    fn test_spec_has_mempool_path() {
        let s = build_spec();
        assert!(s["paths"]["/api/mempool"]["get"].is_object());
    }

    #[test]
    fn test_spec_has_search_path() {
        let s = build_spec();
        assert!(s["paths"]["/api/search"]["get"].is_object());
    }

    #[test]
    fn test_spec_has_openapi_self_path() {
        let s = build_spec();
        assert!(s["paths"]["/api/openapi.json"]["get"].is_object());
    }

    // ── Filter params on /api/txs ──────────────────────────────────────────

    #[test]
    fn test_spec_txs_has_min_amount_param() {
        let s = build_spec();
        let params = s["paths"]["/api/txs"]["get"]["parameters"].as_array().unwrap();
        assert!(params.iter().any(|p| p["name"].as_str() == Some("min_amount")));
    }

    #[test]
    fn test_spec_txs_has_since_param() {
        let s = build_spec();
        let params = s["paths"]["/api/txs"]["get"]["parameters"].as_array().unwrap();
        assert!(params.iter().any(|p| p["name"].as_str() == Some("since")));
    }

    // ── Section paths present ─────────────────────────────────────────────

    #[test]
    fn test_spec_has_staking_stats_path() {
        let s = build_spec();
        assert!(s["paths"]["/api/staking/stats"]["get"].is_object());
    }

    #[test]
    fn test_spec_has_defi_feeds_path() {
        let s = build_spec();
        assert!(s["paths"]["/api/defi/feeds"]["get"].is_object());
    }

    #[test]
    fn test_spec_has_labels_path() {
        let s = build_spec();
        assert!(s["paths"]["/api/labels"]["get"].is_object());
    }

    #[test]
    fn test_spec_has_tokens_path() {
        let s = build_spec();
        assert!(s["paths"]["/api/tokens"]["get"].is_object());
    }

    #[test]
    fn test_spec_has_contracts_path() {
        let s = build_spec();
        assert!(s["paths"]["/api/contracts"]["get"].is_object());
    }

    // ── Components ────────────────────────────────────────────────────────

    #[test]
    fn test_spec_components_schemas_exist() {
        let s = build_spec();
        assert!(s["components"]["schemas"].is_object());
    }

    #[test]
    fn test_spec_schema_error_exists() {
        let s = build_spec();
        assert!(s["components"]["schemas"]["Error"].is_object());
    }

    #[test]
    fn test_spec_schema_network_stats_exists() {
        let s = build_spec();
        assert!(s["components"]["schemas"]["NetworkStats"].is_object());
    }

    #[test]
    fn test_spec_schema_tx_detail_has_status() {
        let s = build_spec();
        assert!(s["components"]["schemas"]["TxDetail"]["properties"]["status"].is_object());
    }

    // ── Helpers + router ──────────────────────────────────────────────────

    #[test]
    fn test_qparam_fields() {
        let p = qparam("limit", "integer", "Max rows", false);
        assert_eq!(p["name"].as_str(),     Some("limit"));
        assert_eq!(p["in"].as_str(),       Some("query"));
        assert_eq!(p["required"].as_bool(), Some(false));
        assert_eq!(p["schema"]["type"].as_str(), Some("integer"));
    }

    #[test]
    fn test_pparam_required() {
        let p = pparam("addr", "string", "Address");
        assert_eq!(p["in"].as_str(),        Some("path"));
        assert_eq!(p["required"].as_bool(), Some(true));
    }

    #[test]
    fn test_openapi_router_builds() {
        let _r = openapi_router();
    }
}
