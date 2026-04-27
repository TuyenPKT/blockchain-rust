// api.ts — v25.1: invoke("fetch_json") through Rust reqwest — bypasses WKWebView restrictions
import { invoke } from "@tauri-apps/api/core";

async function api<T>(nodeUrl: string, path: string): Promise<T> {
  const base = nodeUrl.replace(/\/$/, "");
  const url  = `${base}${path}`;
  const data = await invoke<unknown>("fetch_json", { url });
  const d = data as Record<string, unknown>;
  // 503-equivalent: server returns error object
  if (d && typeof d === "object" && "error" in d) {
    const errVal = d["error"];
    if (errVal === "not_synced") return {} as T;
  }
  return data as T;
}

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
  total_value_pkt?:   number;
  total_value_sat?:   number;
  block_reward?:      number;     // paklets
  block_reward_pkt?:  number;     // PKT float
  [key: string]: unknown;
}

export interface BlockHeader {
  index?:       number;
  height?:      number;
  hash?:        string;
  prev_hash?:   string;
  timestamp?:   number;
  tx_count?:    number;
  txids?:       string[];
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
  return api<NetworkSummary>(nodeUrl, "/api/testnet/summary");
}

export async function fetchBlocks(nodeUrl: string, limit = 10): Promise<{ blocks?: BlockHeader[]; headers?: BlockHeader[] }> {
  return api(nodeUrl, `/api/testnet/headers?limit=${limit}`);
}

export async function fetchBalance(nodeUrl: string, address: string): Promise<unknown> {
  return api(nodeUrl, `/api/testnet/balance/${encodeURIComponent(address)}`);
}

export async function searchQuery(nodeUrl: string, query: string): Promise<unknown> {
  return api(nodeUrl, `/api/testnet/search?q=${encodeURIComponent(query)}`);
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
  return api<AnalyticsSeries>(nodeUrl, `/api/testnet/analytics?metric=${metric}&window=${window}`);
}

export interface AddressTx {
  txid?:      string;
  hash?:      string;
  height?:    number;
  timestamp?: number;   // unix seconds (0 for old entries)
  net_sat?:   number;   // positive = received, negative = sent (satoshis)
  from?:      string;   // sender address
  to?:        string;   // recipient address
  fee_sat?:   number;   // miner fee in satoshis
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
  return api(nodeUrl, `/api/testnet/address/${encodeURIComponent(address)}/txs?page=${page}&limit=${limit}`);
}

export async function fetchAddressUtxos(
  nodeUrl: string,
  address: string,
): Promise<{ utxos?: AddressUtxo[]; error?: string }> {
  return api(nodeUrl, `/api/testnet/address/${encodeURIComponent(address)}/utxos`);
}

export interface TxInput {
  txid?:    string;
  vout?:    number;
  address?: string;
  value?:   number;   // paklets — backend field name
  [key: string]: unknown;
}

export interface TxOutput {
  address?: string;
  value?:   number;   // paklets — backend field name
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
  return api<BlockDetail>(nodeUrl, `/api/testnet/block/${height}`);
}

export async function fetchTxDetail(nodeUrl: string, txid: string): Promise<TxDetail> {
  return api<TxDetail>(nodeUrl, `/api/testnet/tx/${encodeURIComponent(txid)}`);
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
  return api(nodeUrl, `/api/testnet/richlist?limit=${limit}`);
}

export async function fetchMempool(
  nodeUrl: string,
  limit = 50,
): Promise<{ txs?: MempoolTx[]; count?: number; total_fee?: number }> {
  return api(nodeUrl, `/api/testnet/mempool?limit=${limit}`);
}

export function fmtNum(n: number, decimals = 0): string {
  return n.toLocaleString("en-US", { minimumFractionDigits: decimals, maximumFractionDigits: decimals });
}

export function fmtPkt(sat: number): string {
  if (!sat) return "—";
  const pkt = sat / PACKETS_PER_PKT;
  return fmtNum(pkt, pkt >= 1 ? 0 : 4) + " PKT";
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

const MIN_VALID_TS = 1577836800; // 2020-01-01 — bất kỳ ts nào trước đây là lỗi header

export function timeAgo(ts: number): string {
  if (!ts || ts <= 0 || ts < MIN_VALID_TS) return "—";
  const secs = Math.max(0, Math.floor((Date.now() / 1000) - ts));
  if (secs < 10)      return "just now";
  if (secs < 60)      return secs + " secs ago";
  if (secs < 3600)    return Math.floor(secs / 60) + " mins ago";
  if (secs < 86400)   return Math.floor(secs / 3600) + " hrs ago";
  if (secs < 2592000) return Math.floor(secs / 86400) + " days ago";
  return new Date(ts * 1000).toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" });
}
