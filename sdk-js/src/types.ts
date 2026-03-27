// v19.5.1 — PKTCore SDK Types
// Mirrors the JSON responses from /api/testnet/* endpoints

export interface Block {
  height: number;
  hash: string;
  prev_hash: string;
  timestamp: number;
  tx_count: number;
  size: number;
  difficulty: number;
  bits: number;
  nonce: number;
  merkle_root: string;
  miner?: string;
}

export interface TxInput {
  prev_txid: string;
  prev_vout: number;
  address?: string;
  value: number;
  script_sig?: string;
}

export interface TxOutput {
  address: string;
  value: number;
  script_pubkey?: string;
  spent?: boolean;
}

export interface Tx {
  txid: string;
  height: number;
  block_hash?: string;
  timestamp?: number;
  inputs: TxInput[];
  outputs: TxOutput[];
  fee: number;
  size: number;
  is_coinbase: boolean;
}

export interface AddressInfo {
  address: string;
  balance: number;
  received: number;
  sent: number;
  tx_count: number;
  unconfirmed_balance?: number;
}

export interface Utxo {
  txid: string;
  vout: number;
  value: number;
  height: number;
  address: string;
  confirmations?: number;
}

export interface SyncStatus {
  height: number;
  hash: string;
  syncing: boolean;
  peers: number;
  progress?: number;
}

export interface NetworkSummary {
  height: number;
  hashrate: string;
  hashrate_raw: number;
  difficulty: number;
  tx_count_24h: number;
  block_time_avg: number;
  mempool_size: number;
  peers: number;
}

export interface MempoolTx {
  txid: string;
  fee: number;
  fee_rate: number;
  size: number;
  received_at: number;
}

export interface FeeHistogramBin {
  fee_rate: number;
  count: number;
  total_fee: number;
}

export interface AnalyticsPoint {
  height: number;
  timestamp: number;
  difficulty: number;
  hashrate: number;
  block_time: number;
}

export interface RichListEntry {
  address: string;
  balance: number;
  rank: number;
}

export interface AddressLabel {
  script: string;
  label: string;
  category?: string;
}

export interface HealthStatus {
  status: "ok" | "degraded" | "down";
  sync_height: number;
  db_ok: boolean;
  mempool_ok: boolean;
  uptime_secs: number;
}

// ── Pagination ─────────────────────────────────────────────────────────────────

export interface PageInfo {
  page: number;
  limit: number;
  total: number;
  has_next: boolean;
}

export interface BlockPage {
  blocks: Block[];
  page_info: PageInfo;
}

export interface TxPage {
  txs: Tx[];
  page_info: PageInfo;
}

// ── JSON-RPC ──────────────────────────────────────────────────────────────────

export interface RpcRequest {
  jsonrpc: "2.0";
  id: number | string;
  method: string;
  params: unknown[];
}

export interface RpcResponse<T = unknown> {
  jsonrpc: "2.0";
  id: number | string;
  result?: T;
  error?: { code: number; message: string };
}

// ── WebSocket events ──────────────────────────────────────────────────────────

export type WsEventType = "block" | "tx" | "mempool" | "ping";

export interface WsEvent {
  type: WsEventType;
  data: unknown;
}

export type EventCallback = (event: WsEvent) => void;
export type Unsubscribe = () => void;
