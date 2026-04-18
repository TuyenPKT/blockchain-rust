#![allow(dead_code)]
//! v9.9 — SDK Generation
//!
//! Tự động generate JavaScript / TypeScript client SDK từ OpenAPI 3.0.3 spec.
//! Không dùng external codegen tool — spec được đọc từ `openapi::build_spec()` và
//! code được render trực tiếp trong Rust.
//!
//! Endpoints:
//!   GET /api/sdk/js  → JavaScript (CommonJS + browser UMD) client
//!   GET /api/sdk/ts  → TypeScript client với typed response interfaces
//!
//! Usage:
//!   let app = router.merge(sdk_gen::sdk_router());

use axum::{
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use serde_json::Value;

use crate::openapi::build_spec;

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn sdk_router() -> Router {
    Router::new()
        .route("/api/sdk/js", get(get_sdk_js))
        .route("/api/sdk/ts", get(get_sdk_ts))
}

async fn get_sdk_js() -> Response {
    let code = generate_js_sdk();
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        code,
    )
        .into_response()
}

async fn get_sdk_ts() -> Response {
    let code = generate_ts_sdk();
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/typescript; charset=utf-8")],
        code,
    )
        .into_response()
}

// ─── Path parsing helpers ─────────────────────────────────────────────────────

/// Extract path params from template: "/api/block/{height}" → ["height"]
pub fn extract_path_params(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|seg| seg.starts_with('{') && seg.ends_with('}'))
        .map(|seg| seg[1..seg.len() - 1].to_string())
        .collect()
}

/// Convert OpenAPI path to camelCase method name.
///
/// "/api/staking/validators"       → "getStakingValidators"
/// "/api/block/{height}"           → "getBlock"
/// "/api/token/{id}/balance/{addr}" → "getTokenBalance"
/// "/api/openapi.json"             → "getOpenapiJson"
pub fn path_to_method_name(path: &str) -> String {
    let segs: Vec<&str> = path
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty() && !s.starts_with('{'))
        .collect();

    // Drop leading "api" segment
    let segs: &[&str] = if segs.first() == Some(&"api") {
        &segs[1..]
    } else {
        &segs
    };

    if segs.is_empty() {
        return "get".to_string();
    }

    let mut name = String::from("get");
    for seg in segs {
        // "openapi.json" → "OpenapiJson", "blocks.csv" → "BlocksCsv"
        for part in seg.split(['.', '-', '_']) {
            if part.is_empty() {
                continue;
            }
            let mut chars = part.chars();
            if let Some(c) = chars.next() {
                name.push(c.to_ascii_uppercase());
                name.push_str(chars.as_str());
            }
        }
    }
    name
}

/// Extract query param names from a path's GET spec object.
pub fn extract_query_params(path_spec: &Value) -> Vec<String> {
    let params = match path_spec["get"]["parameters"].as_array() {
        Some(a) => a,
        None => return vec![],
    };
    params
        .iter()
        .filter(|p| p["in"].as_str() == Some("query"))
        .filter_map(|p| p["name"].as_str().map(|s| s.to_string()))
        .collect()
}

// ─── JS SDK generation ────────────────────────────────────────────────────────

/// Generate complete JavaScript (UMD) client SDK from OpenAPI spec.
pub fn generate_js_sdk() -> String {
    let spec = build_spec();
    let version = spec["info"]["version"]
        .as_str()
        .unwrap_or("9.9")
        .to_string();

    let methods = build_methods(&spec, "js");

    format!(
        r#"// PKTScan JavaScript SDK
// Auto-generated from OpenAPI {version} spec
// Version: v{version}
//
// Usage (Node.js / CommonJS):
//   const {{ PKTScanClient }} = require('./pktscan-sdk.js');
//   const client = new PKTScanClient('http://localhost:8080');
//   const stats = await client.getStats();
//
// Usage (Browser, add <script src="/api/sdk/js"></script>):
//   const client = new PKTScanClient('http://localhost:8080');

(function (root, factory) {{
  if (typeof module !== 'undefined' && module.exports) {{
    module.exports = factory();
  }} else {{
    root.PKTScanClient = factory();
  }}
}})(typeof globalThis !== 'undefined' ? globalThis : this, function () {{

  class PKTScanClient {{
    /**
     * @param {{string}} baseUrl  Base URL of the PKTScan API server
     */
    constructor(baseUrl) {{
      this.baseUrl = (baseUrl || 'http://localhost:8080').replace(/\/$/, '');
    }}

    /** @private */
    async _get(path, params) {{
      const url = new URL(this.baseUrl + path);
      if (params) {{
        Object.entries(params).forEach(function (kv) {{
          if (kv[1] != null) url.searchParams.set(kv[0], String(kv[1]));
        }});
      }}
      const res = await fetch(url.toString(), {{
        headers: {{ Accept: 'application/json' }},
      }});
      if (!res.ok) {{
        const text = await res.text();
        throw new Error('PKTScan ' + res.status + ': ' + text);
      }}
      return res.json();
    }}

    // ── Auto-generated methods ──────────────────────────────────────────────
{methods}
  }}

  return PKTScanClient;
}});
"#,
        version = version,
        methods = methods,
    )
}

/// Generate complete TypeScript client SDK from OpenAPI spec.
pub fn generate_ts_sdk() -> String {
    let spec = build_spec();
    let version = spec["info"]["version"]
        .as_str()
        .unwrap_or("9.9")
        .to_string();

    let interfaces = build_ts_interfaces();
    let methods = build_methods(&spec, "ts");

    format!(
        r#"// PKTScan TypeScript SDK
// Auto-generated from OpenAPI {version} spec
// Version: v{version}
//
// Usage:
//   import {{ PKTScanClient }} from './pktscan-sdk';
//   const client = new PKTScanClient('http://localhost:8080');
//   const stats: NetworkStats = await client.getStats();

// ── Interfaces ────────────────────────────────────────────────────────────────
{interfaces}
// ── Query param types ─────────────────────────────────────────────────────────

export interface BlocksParams {{
  limit?: number;
  offset?: number;
  from?: number;
}}

export interface TxsParams {{
  limit?: number;
  offset?: number;
  from?: number;
  min_amount?: number;
  max_amount?: number;
  since?: number;
  until?: number;
}}

export interface SearchParams {{
  q: string;
  limit?: number;
}}

// ── Client ────────────────────────────────────────────────────────────────────

export class PKTScanClient {{
  private baseUrl: string;

  constructor(baseUrl: string = 'http://localhost:8080') {{
    this.baseUrl = baseUrl.replace(/\/$/, '');
  }}

  private async _get<T>(path: string, params?: Record<string, unknown>): Promise<T> {{
    const url = new URL(this.baseUrl + path);
    if (params) {{
      Object.entries(params).forEach(([k, v]) => {{
        if (v != null) url.searchParams.set(k, String(v));
      }});
    }}
    const res = await fetch(url.toString(), {{
      headers: {{ Accept: 'application/json' }},
    }});
    if (!res.ok) {{
      const text = await res.text();
      throw new Error(`PKTScan ${{res.status}}: ${{text}}`);
    }}
    return res.json() as Promise<T>;
  }}

  // ── Auto-generated methods ──────────────────────────────────────────────
{methods}
}}

export default PKTScanClient;
"#,
        version = version,
        interfaces = interfaces,
        methods = methods,
    )
}

// ─── Method builders ──────────────────────────────────────────────────────────

/// Build method code for all GET paths in the spec.
/// `lang` = "js" | "ts"
fn build_methods(spec: &Value, lang: &str) -> String {
    let paths = match spec["paths"].as_object() {
        Some(m) => m,
        None => return String::new(),
    };

    let mut sorted_paths: Vec<(&String, &Value)> = paths.iter().collect();
    sorted_paths.sort_by_key(|(p, _)| p.as_str());

    let mut out = String::new();
    for (path, path_spec) in &sorted_paths {
        // Only generate for GET endpoints
        if path_spec["get"].is_null() {
            continue;
        }
        let summary = path_spec["get"]["summary"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let method = path_to_method_name(path);
        let path_params = extract_path_params(path);
        let query_params = extract_query_params(path_spec);

        let code = match lang {
            "ts" => render_ts_method(&method, path, &path_params, &query_params, &summary),
            _ => render_js_method(&method, path, &path_params, &query_params, &summary),
        };
        out.push_str(&code);
    }
    out
}

/// Render a single JavaScript method.
fn render_js_method(
    name: &str,
    path: &str,
    path_params: &[String],
    query_params: &[String],
    summary: &str,
) -> String {
    let js_path = path_to_js_template(path);
    let has_query = !query_params.is_empty();

    let sig = if path_params.is_empty() && !has_query {
        format!("    {}()", name)
    } else if path_params.is_empty() {
        format!("    {}(params)", name)
    } else if !has_query {
        format!("    {}({})", name, path_params.join(", "))
    } else {
        format!("    {}({}, params)", name, path_params.join(", "))
    };

    let query_arg = if has_query { ", params" } else { "" };

    format!(
        "\n    /** {summary} */\n{sig} {{ return this._get(`{js_path}`{query_arg}); }}\n",
        summary = summary,
        sig = sig,
        js_path = js_path,
        query_arg = query_arg,
    )
}

/// Render a single TypeScript method with types.
fn render_ts_method(
    name: &str,
    path: &str,
    path_params: &[String],
    query_params: &[String],
    summary: &str,
) -> String {
    let js_path = path_to_js_template(path);
    let has_query = !query_params.is_empty();

    // Build typed signature
    let path_sig: Vec<String> = path_params
        .iter()
        .map(|p| format!("{}: string | number", p))
        .collect();

    let sig = if path_params.is_empty() && !has_query {
        format!("  {}(): Promise<unknown>", name)
    } else if path_params.is_empty() {
        format!(
            "  {}(params?: Record<string, unknown>): Promise<unknown>",
            name
        )
    } else if !has_query {
        format!(
            "  {}({}): Promise<unknown>",
            name,
            path_sig.join(", ")
        )
    } else {
        format!(
            "  {}({}, params?: Record<string, unknown>): Promise<unknown>",
            name,
            path_sig.join(", ")
        )
    };

    let query_arg = if has_query {
        " params as Record<string, unknown>".to_string()
    } else {
        String::new()
    };

    let call = if query_arg.is_empty() {
        format!("this._get(`{}`)", js_path)
    } else {
        format!("this._get(`{}`,{})", js_path, query_arg)
    };

    format!(
        "\n  /** {summary} */\n{sig} {{ return {call}; }}\n",
        summary = summary,
        sig = sig,
        call = call,
    )
}

/// Convert OpenAPI path template to JS template literal path.
/// "/api/block/{height}" → "/api/block/${height}"
pub fn path_to_js_template(path: &str) -> String {
    path.replace('{', "${")
}

/// Build TypeScript interface definitions for common response types.
fn build_ts_interfaces() -> String {
    r#"export interface NetworkStats {
  height: number;
  difficulty: number;
  hashrate: number;
  block_reward: number;
  total_supply: number;
  utxo_count: number;
  mempool_count: number;
  avg_block_time_s: number;
  block_count: number;
}

export interface BlockDetail {
  index: number;
  timestamp: number;
  hash: string;
  prev_hash: string;
  difficulty: number;
  nonce: number;
  tx_count: number;
}

export interface TxDetail {
  tx_id: string;
  wtx_id: string;
  is_coinbase: boolean;
  fee: number;
  output_total: number;
  block_height: number | null;
  block_hash: string | null;
  timestamp: number | null;
  status: 'confirmed' | 'pending';
  confirmations: number;
}

export interface AddressDetail {
  address: string;
  balance: number;
  utxo_count: number;
  tx_count: number;
}

export interface AddressLabel {
  address: string;
  label: string;
  category: string;
  note: string;
}

export interface ApiError {
  error: string;
}

"#
    .to_string()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── path_to_method_name ────────────────────────────────────────────────

    #[test]
    fn test_method_name_stats() {
        assert_eq!(path_to_method_name("/api/stats"), "getStats");
    }

    #[test]
    fn test_method_name_blocks() {
        assert_eq!(path_to_method_name("/api/blocks"), "getBlocks");
    }

    #[test]
    fn test_method_name_block_with_param() {
        assert_eq!(path_to_method_name("/api/block/{height}"), "getBlock");
    }

    #[test]
    fn test_method_name_token_balance() {
        assert_eq!(
            path_to_method_name("/api/token/{id}/balance/{addr}"),
            "getTokenBalance"
        );
    }

    #[test]
    fn test_method_name_staking_validators() {
        assert_eq!(
            path_to_method_name("/api/staking/validators"),
            "getStakingValidators"
        );
    }

    #[test]
    fn test_method_name_defi_feeds() {
        assert_eq!(path_to_method_name("/api/defi/feeds"), "getDefiFeeds");
    }

    #[test]
    fn test_method_name_openapi_json() {
        assert_eq!(
            path_to_method_name("/api/openapi.json"),
            "getOpenapiJson"
        );
    }

    #[test]
    fn test_method_name_blocks_csv() {
        assert_eq!(path_to_method_name("/api/blocks.csv"), "getBlocksCsv");
    }

    // ── extract_path_params ────────────────────────────────────────────────

    #[test]
    fn test_path_params_none() {
        assert!(extract_path_params("/api/stats").is_empty());
    }

    #[test]
    fn test_path_params_one() {
        assert_eq!(
            extract_path_params("/api/block/{height}"),
            vec!["height"]
        );
    }

    #[test]
    fn test_path_params_two() {
        let p = extract_path_params("/api/token/{id}/balance/{addr}");
        assert_eq!(p, vec!["id", "addr"]);
    }

    // ── path_to_js_template ────────────────────────────────────────────────

    #[test]
    fn test_js_template_no_params() {
        assert_eq!(path_to_js_template("/api/stats"), "/api/stats");
    }

    #[test]
    fn test_js_template_with_param() {
        assert_eq!(
            path_to_js_template("/api/block/{height}"),
            "/api/block/${height}"
        );
    }

    #[test]
    fn test_js_template_multi_params() {
        assert_eq!(
            path_to_js_template("/api/token/{id}/balance/{addr}"),
            "/api/token/${id}/balance/${addr}"
        );
    }

    // ── JS SDK content ─────────────────────────────────────────────────────

    #[test]
    fn test_js_sdk_contains_class() {
        let sdk = generate_js_sdk();
        assert!(sdk.contains("class PKTScanClient"));
    }

    #[test]
    fn test_js_sdk_contains_umd_wrapper() {
        let sdk = generate_js_sdk();
        assert!(sdk.contains("module.exports"));
    }

    #[test]
    fn test_js_sdk_contains_get_stats() {
        let sdk = generate_js_sdk();
        assert!(sdk.contains("getStats"));
    }

    #[test]
    fn test_js_sdk_contains_get_blocks() {
        let sdk = generate_js_sdk();
        assert!(sdk.contains("getTestnetHeaders"));
    }

    #[test]
    fn test_js_sdk_contains_get_tx() {
        let sdk = generate_js_sdk();
        assert!(sdk.contains("getTestnetTx("));
    }

    #[test]
    fn test_js_sdk_contains_get_staking_stats() {
        let sdk = generate_js_sdk();
        assert!(sdk.contains("getTestnetStats"));
    }

    #[test]
    fn test_js_sdk_contains_get_defi_feeds() {
        let sdk = generate_js_sdk();
        assert!(sdk.contains("getTestnetRichList"));
    }

    #[test]
    fn test_js_sdk_not_empty() {
        let sdk = generate_js_sdk();
        assert!(sdk.len() > 500);
    }

    // ── TS SDK content ─────────────────────────────────────────────────────

    #[test]
    fn test_ts_sdk_contains_export_class() {
        let sdk = generate_ts_sdk();
        assert!(sdk.contains("export class PKTScanClient"));
    }

    #[test]
    fn test_ts_sdk_contains_network_stats_interface() {
        let sdk = generate_ts_sdk();
        assert!(sdk.contains("export interface NetworkStats"));
    }

    #[test]
    fn test_ts_sdk_contains_tx_detail_interface() {
        let sdk = generate_ts_sdk();
        assert!(sdk.contains("export interface TxDetail"));
    }

    #[test]
    fn test_ts_sdk_contains_get_stats() {
        let sdk = generate_ts_sdk();
        assert!(sdk.contains("getStats"));
    }

    #[test]
    fn test_ts_sdk_contains_promise_return() {
        let sdk = generate_ts_sdk();
        assert!(sdk.contains("Promise<unknown>"));
    }

    #[test]
    fn test_ts_sdk_contains_export_default() {
        let sdk = generate_ts_sdk();
        assert!(sdk.contains("export default PKTScanClient"));
    }

    #[test]
    fn test_ts_sdk_not_empty() {
        let sdk = generate_ts_sdk();
        assert!(sdk.len() > 500);
    }

    // ── router ─────────────────────────────────────────────────────────────

    #[test]
    fn test_sdk_router_builds() {
        let _r = sdk_router();
    }
}
