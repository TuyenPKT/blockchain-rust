// api.ts — IPC bridge to Tauri commands
import { invoke } from "@tauri-apps/api/core";

/** 1 PKT = 2^30 packets (not 10^9) */
export const PACKETS_PER_PKT = 1_073_741_824;

export interface NetworkSummary {
  height?:           number;
  hashrate?:         number;
  avg_block_time_s?: number;  // alias
  block_time_avg?:   number;  // backend field name
  mempool_count?:    number;
  difficulty?:       number;
  utxo_count?:       number;
  total_value_pkt?:  number;
  total_value_sat?:  number;
  [key: string]: unknown;
}

export interface BlockHeader {
  index?:       number;
  height?:      number;
  hash?:        string;
  prev_hash?:   string;
  timestamp?:   number;
  tx_count?:    number;
  miner?:       string;
  [key: string]: unknown;
}

export interface SearchResult {
  type?:    string;
  height?:  number;
  hash?:    string;
  address?: string;
  [key: string]: unknown;
}

export async function fetchSummary(nodeUrl: string): Promise<NetworkSummary> {
  return invoke<NetworkSummary>("get_summary", { nodeUrl });
}

export async function fetchBlocks(nodeUrl: string, limit = 10): Promise<{ blocks?: BlockHeader[]; headers?: BlockHeader[] }> {
  return invoke("get_blocks", { nodeUrl, limit });
}

export async function fetchBalance(nodeUrl: string, address: string): Promise<unknown> {
  return invoke("get_balance", { nodeUrl, address });
}

export async function searchQuery(nodeUrl: string, query: string): Promise<unknown> {
  return invoke("search", { nodeUrl, query });
}

export interface AnalyticsPoint {
  height:    number;
  timestamp: number;
  value:     number;
}

export interface AnalyticsSeries {
  metric: string;
  unit:   string;
  window: number;
  points: AnalyticsPoint[];
}

export async function fetchAnalytics(
  nodeUrl: string,
  metric: "hashrate" | "block_time" | "difficulty",
  window = 100,
): Promise<AnalyticsSeries> {
  return invoke("get_analytics", { nodeUrl, metric, window });
}

export interface AddressTx {
  txid?:      string;
  hash?:      string;
  height?:    number;
  timestamp?: number;
  amount?:    number;
  fee?:       number;
  type?:      string;
  [key: string]: unknown;
}

export interface AddressUtxo {
  txid?:          string;
  vout?:          number;
  amount?:        number;
  height?:        number;
  script_pubkey?: string;
  [key: string]: unknown;
}

export async function fetchAddressTxs(
  nodeUrl: string,
  address: string,
  page = 0,
  limit = 20,
): Promise<{ txs?: AddressTx[]; total?: number; page?: number }> {
  return invoke("get_address_txs", { nodeUrl, address, page, limit });
}

export async function fetchAddressUtxos(
  nodeUrl: string,
  address: string,
): Promise<{ utxos?: AddressUtxo[]; error?: string }> {
  return invoke("get_address_utxos", { nodeUrl, address });
}

export interface TxInput {
  txid?:    string;
  vout?:    number;
  address?: string;
  amount?:  number;
  [key: string]: unknown;
}

export interface TxOutput {
  address?: string;
  amount?:  number;
  vout?:    number;
  type?:    string;
  [key: string]: unknown;
}

export interface TxDetail {
  txid?:          string;
  hash?:          string;
  height?:        number;
  timestamp?:     number;
  fee?:           number;
  size?:          number;
  inputs?:        TxInput[];
  outputs?:       TxOutput[];
  confirmations?: number;
  [key: string]: unknown;
}

export interface BlockDetail {
  height?:        number;
  index?:         number;
  hash?:          string;
  prev_hash?:     string;
  timestamp?:     number;
  tx_count?:      number;
  miner?:         string;
  size?:          number;
  difficulty?:    number;
  total_fees?:    number;
  confirmations?: number;
  txids?:         string[];
  txs?:           TxDetail[];
  [key: string]: unknown;
}

export async function fetchBlockDetail(nodeUrl: string, height: number): Promise<BlockDetail> {
  return invoke("get_block_detail", { nodeUrl, height });
}

export async function fetchTxDetail(nodeUrl: string, txid: string): Promise<TxDetail> {
  return invoke("get_tx_detail", { nodeUrl, txid });
}

export interface RichHolder {
  rank?:    number;
  address?: string;
  balance?: number;
  pct?:     number;   // % of total supply
  [key: string]: unknown;
}

export interface MempoolTx {
  txid?:      string;
  hash?:      string;
  fee?:       number;
  size?:      number;
  fee_rate?:  number;
  inputs?:    number;
  outputs?:   number;
  timestamp?: number;
  [key: string]: unknown;
}

export async function fetchRichList(
  nodeUrl: string,
  limit = 100,
): Promise<{ holders?: RichHolder[]; total_supply?: number }> {
  return invoke("get_rich_list", { nodeUrl, limit });
}

export async function fetchMempool(
  nodeUrl: string,
  limit = 50,
): Promise<{ txs?: MempoolTx[]; count?: number; total_fee?: number }> {
  return invoke("get_mempool", { nodeUrl, limit });
}

export function fmtHashrate(h: number): string {
  if (h >= 1e15) return (h / 1e15).toFixed(2) + " PH/s";
  if (h >= 1e12) return (h / 1e12).toFixed(2) + " TH/s";
  if (h >= 1e9)  return (h / 1e9).toFixed(2)  + " GH/s";
  if (h >= 1e6)  return (h / 1e6).toFixed(2)  + " MH/s";
  return h + " H/s";
}

export function shortHash(h: string): string {
  return h ? h.slice(0, 10) + "…" + h.slice(-8) : "—";
}

export function timeAgo(ts: number): string {
  const secs = Math.max(0, Math.floor((Date.now() - ts * 1000) / 1000));
  if (secs < 60) return secs + "s ago";
  if (secs < 3600) return Math.floor(secs / 60) + "m ago";
  return Math.floor(secs / 3600) + "h ago";
}
