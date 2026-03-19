#![allow(dead_code)]
//! v10.8 — GraphQL read-only API
//!
//! Endpoint: POST /graphql  (JSON body: {"query": "...", "variables": {...}})
//!           GET  /graphql  → trả về schema SDL
//!
//! Schema (read-only, không có mutation cho đến Era 17):
//!   query { chainInfo { height difficulty mempoolCount } }
//!   query { block(height: 1) { hash txCount } }
//!   query { blocks(limit: 10, offset: 0) { height hash } }
//!   query { balance(address: "abc...") }
//!   query { mempoolTxs(limit: 5) { txId fee } }

use std::sync::Arc;
use tokio::sync::Mutex;

use async_graphql::{
    Context, EmptyMutation, EmptySubscription, Object, Schema, SimpleObject,
};
use axum::{
    extract::{Json, State},
    routing::get,
    Router,
};
use serde_json::Value;

use crate::chain::Blockchain;

// ─── Schema types ─────────────────────────────────────────────────────────────

pub type GqlSchema  = Schema<QueryRoot, EmptyMutation, EmptySubscription>;
pub type ChainState = Arc<Mutex<Blockchain>>;

/// Thông tin tổng quan về chain
#[derive(SimpleObject)]
pub struct ChainInfo {
    pub height:        i32,
    pub difficulty:    i32,
    pub mempool_count: i32,
}

/// Thông tin cơ bản của 1 block
#[derive(SimpleObject)]
pub struct BlockInfo {
    pub height:    i32,
    pub hash:      String,
    pub prev_hash: String,
    pub timestamp: i64,
    pub tx_count:  i32,
    pub nonce:     String,
}

/// Thông tin cơ bản của 1 transaction trong mempool
#[derive(SimpleObject)]
pub struct TxInfo {
    pub tx_id:   String,
    pub fee:     i64,
    pub inputs:  i32,
    pub outputs: i32,
}

// ─── Query Root ───────────────────────────────────────────────────────────────

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// Thông tin tổng quan về chain
    async fn chain_info(&self, ctx: &Context<'_>) -> async_graphql::Result<ChainInfo> {
        let state = ctx.data::<ChainState>()?;
        let bc    = state.lock().await;
        Ok(ChainInfo {
            height:        bc.chain.len().saturating_sub(1) as i32,
            difficulty:    bc.difficulty as i32,
            mempool_count: bc.mempool.entries.len() as i32,
        })
    }

    /// Block theo height. Null nếu không tồn tại hoặc height < 0.
    async fn block(&self, ctx: &Context<'_>, height: i32) -> async_graphql::Result<Option<BlockInfo>> {
        if height < 0 { return Ok(None); }
        let state = ctx.data::<ChainState>()?;
        let bc    = state.lock().await;
        Ok(bc.chain.get(height as usize).map(|b| BlockInfo {
            height:    b.index as i32,
            hash:      b.hash.clone(),
            prev_hash: b.prev_hash.clone(),
            timestamp: b.timestamp,
            tx_count:  b.transactions.len() as i32,
            nonce:     b.nonce.to_string(),
        }))
    }

    /// Danh sách blocks — tối đa 50 per query
    async fn blocks(
        &self,
        ctx: &Context<'_>,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> async_graphql::Result<Vec<BlockInfo>> {
        let limit  = limit.unwrap_or(10).clamp(1, 50) as usize;
        let offset = offset.unwrap_or(0).max(0) as usize;
        let state  = ctx.data::<ChainState>()?;
        let bc     = state.lock().await;
        Ok(bc.chain.iter().skip(offset).take(limit).map(|b| BlockInfo {
            height:    b.index as i32,
            hash:      b.hash.clone(),
            prev_hash: b.prev_hash.clone(),
            timestamp: b.timestamp,
            tx_count:  b.transactions.len() as i32,
            nonce:     b.nonce.to_string(),
        }).collect())
    }

    /// Số dư paklets của 1 địa chỉ (trả về string để tránh overflow i32)
    async fn balance(&self, ctx: &Context<'_>, address: String) -> async_graphql::Result<String> {
        let state = ctx.data::<ChainState>()?;
        let bc    = state.lock().await;
        Ok(bc.utxo_set.balance_of(&address).to_string())
    }

    /// Số transactions trong mempool
    async fn mempool_count(&self, ctx: &Context<'_>) -> async_graphql::Result<i32> {
        let state = ctx.data::<ChainState>()?;
        let bc    = state.lock().await;
        Ok(bc.mempool.entries.len() as i32)
    }

    /// Danh sách transactions trong mempool — tối đa 20
    async fn mempool_txs(
        &self,
        ctx: &Context<'_>,
        limit: Option<i32>,
    ) -> async_graphql::Result<Vec<TxInfo>> {
        let limit = limit.unwrap_or(10).clamp(1, 20) as usize;
        let state = ctx.data::<ChainState>()?;
        let bc    = state.lock().await;
        Ok(bc.mempool.entries.values().take(limit).map(|e| TxInfo {
            tx_id:   e.tx.tx_id.clone(),
            fee:     e.fee as i64,
            inputs:  e.tx.inputs.len() as i32,
            outputs: e.tx.outputs.len() as i32,
        }).collect())
    }
}

// ─── Schema builder ───────────────────────────────────────────────────────────

pub fn build_schema(chain: ChainState) -> GqlSchema {
    Schema::build(QueryRoot, EmptyMutation, EmptySubscription)
        .data(chain)
        .finish()
}

// ─── Axum handlers ────────────────────────────────────────────────────────────

/// POST /graphql — JSON body {"query": "...", "variables": {...}}
async fn graphql_post(
    State(schema): State<GqlSchema>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let query = body["query"].as_str().unwrap_or("").to_string();
    let mut req = async_graphql::Request::new(query);

    if let Some(vars) = body.get("variables") {
        req = req.variables(async_graphql::Variables::from_json(vars.clone()));
    }
    let res = schema.execute(req).await;
    // async_graphql::Response implements Serialize
    let val = serde_json::to_value(&res).unwrap_or(serde_json::json!({"errors":[]}));
    Json(val)
}

/// GET /graphql — trả về schema SDL (introspection)
async fn graphql_sdl(State(schema): State<GqlSchema>) -> String {
    schema.sdl()
}

/// Router cho `/graphql` — merge vào pktscan_api::serve()
pub fn graphql_router(chain: ChainState) -> Router {
    let schema = build_schema(chain);
    Router::new()
        .route("/graphql", get(graphql_sdl).post(graphql_post))
        .with_state(schema)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::Blockchain;

    async fn make_schema() -> GqlSchema {
        let state = Arc::new(Mutex::new(Blockchain::new()));
        build_schema(state)
    }

    #[tokio::test]
    async fn test_chain_info_genesis() {
        let schema = make_schema().await;
        let res    = schema.execute("{ chainInfo { height difficulty mempoolCount } }").await;
        assert!(res.errors.is_empty(), "{:?}", res.errors);
        let data = res.data.into_json().unwrap();
        assert_eq!(data["chainInfo"]["height"], 0);
        assert_eq!(data["chainInfo"]["mempoolCount"], 0);
    }

    #[tokio::test]
    async fn test_block_genesis() {
        let schema = make_schema().await;
        let res    = schema.execute("{ block(height: 0) { height hash txCount } }").await;
        assert!(res.errors.is_empty(), "{:?}", res.errors);
        let data = res.data.into_json().unwrap();
        assert_eq!(data["block"]["height"], 0);
        assert_eq!(data["block"]["txCount"], 0);
    }

    #[tokio::test]
    async fn test_block_out_of_range_returns_null() {
        let schema = make_schema().await;
        let res    = schema.execute("{ block(height: 9999) { hash } }").await;
        assert!(res.errors.is_empty(), "{:?}", res.errors);
        assert!(res.data.into_json().unwrap()["block"].is_null());
    }

    #[tokio::test]
    async fn test_block_negative_returns_null() {
        let schema = make_schema().await;
        let res    = schema.execute("{ block(height: -1) { hash } }").await;
        assert!(res.errors.is_empty(), "{:?}", res.errors);
        assert!(res.data.into_json().unwrap()["block"].is_null());
    }

    #[tokio::test]
    async fn test_blocks_genesis_only() {
        let schema = make_schema().await;
        let res    = schema.execute("{ blocks { height hash } }").await;
        assert!(res.errors.is_empty(), "{:?}", res.errors);
        let data = res.data.into_json().unwrap();
        assert_eq!(data["blocks"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_blocks_limit_clamped() {
        let schema = make_schema().await;
        let res    = schema.execute("{ blocks(limit: 9999) { height } }").await;
        assert!(res.errors.is_empty(), "{:?}", res.errors);
        assert!(res.data.into_json().unwrap()["blocks"].as_array().unwrap().len() <= 50);
    }

    #[tokio::test]
    async fn test_balance_unknown_address() {
        let schema = make_schema().await;
        let res    = schema.execute(r#"{ balance(address: "unknown") }"#).await;
        assert!(res.errors.is_empty(), "{:?}", res.errors);
        assert_eq!(res.data.into_json().unwrap()["balance"].as_str().unwrap(), "0");
    }

    #[tokio::test]
    async fn test_mempool_count_empty() {
        let schema = make_schema().await;
        let res    = schema.execute("{ mempoolCount }").await;
        assert!(res.errors.is_empty(), "{:?}", res.errors);
        assert_eq!(res.data.into_json().unwrap()["mempoolCount"], 0);
    }

    #[tokio::test]
    async fn test_mempool_txs_empty() {
        let schema = make_schema().await;
        let res    = schema.execute("{ mempoolTxs { txId fee } }").await;
        assert!(res.errors.is_empty(), "{:?}", res.errors);
        assert_eq!(res.data.into_json().unwrap()["mempoolTxs"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_sdl_has_query_and_no_mutation() {
        let schema = make_schema().await;
        let sdl    = schema.sdl();
        assert!(sdl.contains("chainInfo"),  "SDL must have chainInfo");
        assert!(sdl.contains("balance"),    "SDL must have balance");
        assert!(!sdl.contains("type Mutation"), "Must not expose mutations");
    }

    #[tokio::test]
    async fn test_chain_info_after_mining() {
        let mut bc = Blockchain::new();
        bc.difficulty = 1;
        bc.add_block(vec![], "aabbccddaabbccddaabbccddaabbccddaabbccdd");
        let state  = Arc::new(Mutex::new(bc));
        let schema = build_schema(state);
        let res    = schema.execute("{ chainInfo { height } }").await;
        assert!(res.errors.is_empty(), "{:?}", res.errors);
        assert_eq!(res.data.into_json().unwrap()["chainInfo"]["height"], 1);
    }

    #[tokio::test]
    async fn test_blocks_offset() {
        let mut bc = Blockchain::new();
        bc.difficulty = 1;
        let addr = "aabbccddaabbccddaabbccddaabbccddaabbccdd";
        bc.add_block(vec![], addr);
        bc.add_block(vec![], addr);
        let state  = Arc::new(Mutex::new(bc));
        let schema = build_schema(state);
        let res    = schema.execute("{ blocks(limit: 2, offset: 1) { height } }").await;
        assert!(res.errors.is_empty(), "{:?}", res.errors);
        let items = res.data.into_json().unwrap();
        let arr   = items["blocks"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["height"], 1);
    }

    #[tokio::test]
    async fn test_block_prev_hash_field() {
        let schema = make_schema().await;
        let res    = schema.execute("{ block(height: 0) { prevHash } }").await;
        assert!(res.errors.is_empty(), "{:?}", res.errors);
        let data = res.data.into_json().unwrap();
        assert!(data["block"]["prevHash"].is_string());
    }
}
