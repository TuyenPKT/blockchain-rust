#![allow(dead_code)]
//! v24.8 — OpenAPI 3.0.3 Spec (GET /api/openapi.json)
//!
//! Serve OpenAPI spec cho tất cả PKTScan testnet endpoints.
//! Routes khớp chính xác với testnet_web_router().

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

pub fn build_spec() -> Value {
    json!({
        "openapi": "3.0.3",
        "info": {
            "title":       "PKTScan Testnet API",
            "version":     "24.8",
            "description": "PKTScan blockchain explorer REST API — public read-only endpoints. Base URL: https://oceif.com/blockchain-rust"
        },
        "servers": [
            { "url": "https://oceif.com/blockchain-rust", "description": "Testnet (oceif.com)" },
            { "url": "http://localhost:8081",              "description": "Local dev" }
        ],
        "paths": build_paths(),
        "components": {
            "schemas":    build_schemas(),
            "parameters": build_common_params(),
        }
    })
}

// ─── Paths ────────────────────────────────────────────────────────────────────

fn build_paths() -> Value {
    let mut m = serde_json::Map::new();

    // ── Chain info ────────────────────────────────────────────────────────
    m.insert("/api/testnet/summary".into(), json!({
        "get": { "summary": "Network summary (height, hashrate, difficulty, mempool…)", "tags": ["chain"],
                 "responses": { "200": ok_json("Summary") } }
    }));
    m.insert("/api/testnet/stats".into(), json!({
        "get": { "summary": "Network stats (alias of summary)", "tags": ["chain"],
                 "responses": { "200": ok_json("Summary") } }
    }));
    m.insert("/api/testnet/headers".into(), json!({
        "get": {
            "summary": "List block headers (newest first)", "tags": ["chain"],
            "parameters": [
                qparam("limit",  "integer", "Max results (default 20, max 100)", false),
                qparam("offset", "integer", "Skip N results",                    false),
                qparam("from",   "integer", "Start from block height",           false),
            ],
            "responses": { "200": ok_json("HeaderList") }
        }
    }));
    m.insert("/api/testnet/header/{h}".into(), json!({
        "get": {
            "summary": "Block header by height or hash", "tags": ["chain"],
            "parameters": [ pparam("h", "string", "Block height (integer) or block hash (hex)") ],
            "responses": { "200": ok_json("BlockHeader"), "404": err_resp("Block not found") }
        }
    }));
    m.insert("/api/testnet/block/{height}".into(), json!({
        "get": {
            "summary": "Block detail (header + transactions)", "tags": ["chain"],
            "parameters": [ pparam("height", "integer", "Block height") ],
            "responses": { "200": ok_json("BlockDetail"), "404": err_resp("Block not found") }
        }
    }));
    m.insert("/api/testnet/analytics".into(), json!({
        "get": {
            "summary": "Chain analytics time series (hashrate, difficulty, block_time)", "tags": ["chain"],
            "parameters": [
                qparam("window", "integer", "Number of recent blocks (default 100)", false),
            ],
            "responses": { "200": ok_json("Analytics") }
        }
    }));

    // ── Transactions ──────────────────────────────────────────────────────
    m.insert("/api/testnet/txs".into(), json!({
        "get": {
            "summary": "List transactions", "tags": ["transactions"],
            "parameters": [
                qparam("limit",  "integer", "Max results (default 20, max 100)", false),
                qparam("offset", "integer", "Skip N results",                    false),
                qparam("from",   "integer", "Start from block height",           false),
            ],
            "responses": { "200": ok_json("TxList") }
        }
    }));
    m.insert("/api/testnet/tx/{txid}".into(), json!({
        "get": {
            "summary": "Transaction detail", "tags": ["transactions"],
            "parameters": [ pparam("txid", "string", "Transaction ID (hex, 64 chars)") ],
            "responses": { "200": ok_json("TxDetail"), "404": err_resp("Transaction not found") }
        }
    }));
    m.insert("/api/testnet/tx/broadcast".into(), json!({
        "post": {
            "summary": "Broadcast raw signed transaction", "tags": ["transactions"],
            "requestBody": {
                "required": true,
                "content": {
                    "application/json": {
                        "schema": {
                            "type": "object",
                            "required": ["raw_tx_hex"],
                            "properties": {
                                "raw_tx_hex": { "type": "string", "description": "Signed TX serialized as hex" }
                            }
                        }
                    }
                }
            },
            "responses": {
                "200": ok_json("BroadcastResult"),
                "400": err_resp("Invalid TX or parse error")
            }
        }
    }));
    m.insert("/api/testnet/mempool".into(), json!({
        "get": { "summary": "Pending transactions in mempool", "tags": ["transactions"],
                 "responses": { "200": ok_json("MempoolList") } }
    }));
    m.insert("/api/testnet/mempool/fee-histogram".into(), json!({
        "get": { "summary": "Fee rate histogram for mempool", "tags": ["transactions"],
                 "responses": { "200": ok_json("FeeHistogram") } }
    }));

    // ── Address ────────────────────────────────────────────────────────────
    m.insert("/api/testnet/balance/{addr}".into(), json!({
        "get": {
            "summary": "PKT balance for address (paklets)", "tags": ["address"],
            "parameters": [ pparam("addr", "string", "PKT address (Base58Check, bech32, or 0x EVM)") ],
            "responses": { "200": ok_json("Balance") }
        }
    }));
    m.insert("/api/testnet/utxos/{addr}".into(), json!({
        "get": {
            "summary": "UTXOs for address", "tags": ["address"],
            "parameters": [ pparam("addr", "string", "PKT address") ],
            "responses": { "200": ok_json("UtxoList") }
        }
    }));
    m.insert("/api/testnet/address/{addr}/txs".into(), json!({
        "get": {
            "summary": "Transaction history for address", "tags": ["address"],
            "parameters": [
                pparam("addr",   "string",  "PKT address"),
                qparam("limit",  "integer", "Max results (default 20)", false),
                qparam("offset", "integer", "Skip N results",           false),
            ],
            "responses": { "200": ok_json("TxList") }
        }
    }));
    m.insert("/api/testnet/address/{addr}/utxos".into(), json!({
        "get": {
            "summary": "UTXOs for address (full detail)", "tags": ["address"],
            "parameters": [ pparam("addr", "string", "PKT address") ],
            "responses": { "200": ok_json("UtxoList") }
        }
    }));
    m.insert("/api/testnet/addr/{base58}".into(), json!({
        "get": {
            "summary": "Address lookup by Base58Check address", "tags": ["address"],
            "parameters": [ pparam("base58", "string", "Base58Check PKT address (pkt1q...)") ],
            "responses": { "200": ok_json("AddressDetail") }
        }
    }));
    m.insert("/api/testnet/rich-list".into(), json!({
        "get": {
            "summary": "Top addresses by balance", "tags": ["address"],
            "parameters": [ qparam("limit", "integer", "Max results (default 50)", false) ],
            "responses": { "200": ok_json("RichList") }
        }
    }));
    m.insert("/api/testnet/address/{addr}/export.csv".into(), json!({
        "get": {
            "summary": "CSV export of address transactions", "tags": ["address"],
            "parameters": [ pparam("addr", "string", "PKT address") ],
            "responses": { "200": csv_resp() }
        }
    }));

    // ── Search ─────────────────────────────────────────────────────────────
    m.insert("/api/testnet/search".into(), json!({
        "get": {
            "summary": "Search blocks, transactions, addresses", "tags": ["search"],
            "parameters": [
                qparam("q",     "string",  "Query: block height, tx hash prefix, address", true),
                qparam("limit", "integer", "Max results per category (default 10)",         false),
            ],
            "responses": { "200": ok_json("SearchResults") }
        }
    }));

    // ── Labels ─────────────────────────────────────────────────────────────
    m.insert("/api/testnet/label/{script}".into(), json!({
        "get": {
            "summary": "Address label by script hex", "tags": ["labels"],
            "parameters": [ pparam("script", "string", "Script pubkey hex") ],
            "responses": { "200": ok_json("Label"), "404": err_resp("No label found") }
        }
    }));

    // ── Export ─────────────────────────────────────────────────────────────
    m.insert("/api/testnet/blocks/export.csv".into(), json!({
        "get": {
            "summary": "CSV export of blocks", "tags": ["export"],
            "parameters": [
                qparam("limit", "integer", "Max rows",              false),
                qparam("from",  "integer", "Start from height",     false),
            ],
            "responses": { "200": csv_resp() }
        }
    }));

    // ── Health ─────────────────────────────────────────────────────────────
    m.insert("/api/health/detailed".into(), json!({
        "get": { "summary": "Node health check (sync status, DB, peers)", "tags": ["health"],
                 "responses": { "200": ok_json("Health") } }
    }));

    // ── Sync control ──────────────────────────────────────────────────────
    m.insert("/api/testnet/sync-status".into(), json!({
        "get": { "summary": "Current sync status (height, peer, running)", "tags": ["sync"],
                 "responses": { "200": ok_json("SyncStatus") } }
    }));
    m.insert("/api/testnet/sync/proc-status".into(), json!({
        "get": { "summary": "Sync process status", "tags": ["sync"],
                 "responses": { "200": ok_json("SyncStatus") } }
    }));
    m.insert("/api/testnet/sync/start".into(), json!({
        "post": { "summary": "Start sync", "tags": ["sync"],
                  "responses": { "200": ok_json("SyncStatus") } }
    }));
    m.insert("/api/testnet/sync/stop".into(), json!({
        "post": { "summary": "Stop sync", "tags": ["sync"],
                  "responses": { "200": ok_json("SyncStatus") } }
    }));

    // ── Meta ──────────────────────────────────────────────────────────────
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
            "properties": { "error": { "type": "string" } }
        },
        "Summary": {
            "type": "object",
            "properties": {
                "height":           { "type": "integer", "description": "Latest block height" },
                "difficulty":       { "type": "integer" },
                "hashrate":         { "type": "integer", "description": "Estimated h/s" },
                "block_reward":     { "type": "integer", "description": "Current block reward (paklets)" },
                "avg_block_time_s": { "type": "number",  "description": "Average block time (seconds)" },
                "mempool_count":    { "type": "integer" },
                "total_supply":     { "type": "integer", "description": "Circulating supply (paklets)" }
            }
        },
        "BlockHeader": {
            "type": "object",
            "properties": {
                "height":     { "type": "integer" },
                "hash":       { "type": "string" },
                "timestamp":  { "type": "integer" },
                "difficulty": { "type": "integer" },
                "tx_count":   { "type": "integer" }
            }
        },
        "BlockDetail": {
            "type": "object",
            "properties": {
                "height":     { "type": "integer" },
                "hash":       { "type": "string" },
                "timestamp":  { "type": "integer" },
                "difficulty": { "type": "integer" },
                "tx_count":   { "type": "integer" },
                "txs":        { "type": "array", "items": { "$ref": "#/components/schemas/TxDetail" } }
            }
        },
        "TxDetail": {
            "type": "object",
            "properties": {
                "tx_id":        { "type": "string" },
                "is_coinbase":  { "type": "boolean" },
                "block_height": { "type": "integer", "nullable": true },
                "timestamp":    { "type": "integer", "nullable": true },
                "inputs":       { "type": "array",   "items": { "type": "object" } },
                "outputs":      { "type": "array",   "items": { "type": "object" } },
                "fee":          { "type": "integer" },
                "output_total": { "type": "integer" }
            }
        },
        "Balance": {
            "type": "object",
            "properties": {
                "address":          { "type": "string" },
                "balance_paklets":  { "type": "integer" },
                "balance_pkt":      { "type": "number" }
            }
        },
        "UtxoList": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "txid":          { "type": "string" },
                    "vout":          { "type": "integer" },
                    "value":         { "type": "integer", "description": "paklets" },
                    "height":        { "type": "integer" },
                    "script_pubkey": { "type": "string" }
                }
            }
        },
        "BroadcastResult": {
            "type": "object",
            "properties": {
                "txid":   { "type": "string" },
                "status": { "type": "string", "enum": ["broadcast"] }
            }
        },
        "RichList": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "address": { "type": "string" },
                    "balance": { "type": "integer" }
                }
            }
        },
        "SyncStatus": {
            "type": "object",
            "properties": {
                "running": { "type": "boolean" },
                "height":  { "type": "integer" },
                "peer":    { "type": "string" }
            }
        },
        "Health": {
            "type": "object",
            "properties": {
                "status": { "type": "string", "enum": ["ok", "degraded", "error"] },
                "height": { "type": "integer" },
                "synced": { "type": "boolean" }
            }
        },
        "Label": {
            "type": "object",
            "properties": {
                "script":   { "type": "string" },
                "label":    { "type": "string" },
                "category": { "type": "string" }
            }
        },
        "SearchResults": {
            "type": "object",
            "properties": {
                "blocks":  { "type": "array", "items": { "type": "object" } },
                "txs":     { "type": "array", "items": { "type": "object" } },
                "address": { "type": "array", "items": { "type": "object" } }
            }
        },
        "Analytics": {
            "type": "object",
            "properties": {
                "hashrate":    { "type": "array", "items": { "type": "number" } },
                "difficulty":  { "type": "array", "items": { "type": "number" } },
                "block_times": { "type": "array", "items": { "type": "number" } }
            }
        }
    })
}

// ─── Common parameters ────────────────────────────────────────────────────────

fn build_common_params() -> Value {
    json!({
        "Limit":  qparam("limit",  "integer", "Max results per page (default 20, max 100)", false),
        "Offset": qparam("offset", "integer", "Skip N results",                             false),
        "From":   qparam("from",   "integer", "Start from block height",                   false),
    })
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn qparam(name: &str, schema_type: &str, description: &str, required: bool) -> Value {
    json!({
        "name": name, "in": "query", "required": required,
        "description": description, "schema": { "type": schema_type }
    })
}

fn pparam(name: &str, schema_type: &str, description: &str) -> Value {
    json!({
        "name": name, "in": "path", "required": true,
        "description": description, "schema": { "type": schema_type }
    })
}

fn ok_json(schema_ref: &str) -> Value {
    json!({
        "description": "Success",
        "content": { "application/json": { "schema": { "$ref": format!("#/components/schemas/{}", schema_ref) } } }
    })
}

fn err_resp(description: &str) -> Value {
    json!({
        "description": description,
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Error" } } }
    })
}

fn csv_resp() -> Value {
    json!({ "description": "CSV file", "content": { "text/csv": { "schema": { "type": "string" } } } })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spec_openapi_version() {
        let s = build_spec();
        assert_eq!(s["openapi"].as_str(), Some("3.0.3"));
    }

    #[test]
    fn test_spec_info_version_24_8() {
        let s = build_spec();
        assert_eq!(s["info"]["version"].as_str(), Some("24.8"));
    }

    #[test]
    fn test_spec_servers_has_oceif() {
        let s = build_spec();
        let servers = s["servers"].as_array().unwrap();
        assert!(servers.iter().any(|sv| sv["url"].as_str()
            .map(|u| u.contains("oceif.com")).unwrap_or(false)));
    }

    #[test]
    fn test_spec_paths_not_empty() {
        let s = build_spec();
        assert!(s["paths"].as_object().unwrap().len() >= 20);
    }

    #[test]
    fn test_spec_has_summary_path() {
        let s = build_spec();
        assert!(s["paths"]["/api/testnet/summary"]["get"].is_object());
    }

    #[test]
    fn test_spec_has_headers_path() {
        let s = build_spec();
        assert!(s["paths"]["/api/testnet/headers"]["get"].is_object());
    }

    #[test]
    fn test_spec_has_block_detail_path() {
        let s = build_spec();
        assert!(s["paths"]["/api/testnet/block/{height}"]["get"].is_object());
    }

    #[test]
    fn test_spec_has_tx_detail_path() {
        let s = build_spec();
        assert!(s["paths"]["/api/testnet/tx/{txid}"]["get"].is_object());
    }

    #[test]
    fn test_spec_has_broadcast_path() {
        let s = build_spec();
        assert!(s["paths"]["/api/testnet/tx/broadcast"]["post"].is_object());
    }

    #[test]
    fn test_spec_has_balance_path() {
        let s = build_spec();
        assert!(s["paths"]["/api/testnet/balance/{addr}"]["get"].is_object());
    }

    #[test]
    fn test_spec_has_rich_list_path() {
        let s = build_spec();
        assert!(s["paths"]["/api/testnet/rich-list"]["get"].is_object());
    }

    #[test]
    fn test_spec_has_search_path() {
        let s = build_spec();
        assert!(s["paths"]["/api/testnet/search"]["get"].is_object());
    }

    #[test]
    fn test_spec_has_health_path() {
        let s = build_spec();
        assert!(s["paths"]["/api/health/detailed"]["get"].is_object());
    }

    #[test]
    fn test_spec_has_sync_status_path() {
        let s = build_spec();
        assert!(s["paths"]["/api/testnet/sync-status"]["get"].is_object());
    }

    #[test]
    fn test_spec_broadcast_has_request_body() {
        let s = build_spec();
        assert!(s["paths"]["/api/testnet/tx/broadcast"]["post"]["requestBody"].is_object());
    }

    #[test]
    fn test_spec_schema_summary_exists() {
        let s = build_spec();
        assert!(s["components"]["schemas"]["Summary"].is_object());
    }

    #[test]
    fn test_spec_schema_broadcast_result_exists() {
        let s = build_spec();
        assert!(s["components"]["schemas"]["BroadcastResult"].is_object());
    }

    #[test]
    fn test_spec_schema_utxo_list_is_array() {
        let s = build_spec();
        assert_eq!(s["components"]["schemas"]["UtxoList"]["type"].as_str(), Some("array"));
    }

    #[test]
    fn test_qparam_fields() {
        let p = qparam("limit", "integer", "Max rows", false);
        assert_eq!(p["name"].as_str(),            Some("limit"));
        assert_eq!(p["in"].as_str(),              Some("query"));
        assert_eq!(p["required"].as_bool(),        Some(false));
        assert_eq!(p["schema"]["type"].as_str(),  Some("integer"));
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
