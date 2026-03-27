// v19.5.1 — PKTCore SDK Client

import type {
  Block, BlockPage, Tx, TxPage,
  AddressInfo, Utxo,
  SyncStatus, NetworkSummary,
  MempoolTx, FeeHistogramBin,
  AnalyticsPoint, RichListEntry,
  AddressLabel, HealthStatus,
  RpcRequest, RpcResponse,
  WsEvent, EventCallback, Unsubscribe,
} from "./types.js";

export class PktApiError extends Error {
  constructor(
    public readonly status: number,
    message: string,
  ) {
    super(`HTTP ${status}: ${message}`);
    this.name = "PktApiError";
  }
}

export class PktClient {
  private readonly base: string;
  private ws: WebSocket | null = null;
  private wsListeners: Map<string, Set<EventCallback>> = new Map();
  private rpcId = 0;

  /**
   * @param baseUrl  URL gốc của PKTScan, vd: "https://oceif.com"
   */
  constructor(baseUrl: string) {
    this.base = baseUrl.replace(/\/$/, "");
  }

  // ── Private helpers ────────────────────────────────────────────────────────

  private async get<T>(path: string): Promise<T> {
    const res = await fetch(`${this.base}${path}`);
    if (!res.ok) {
      const text = await res.text().catch(() => res.statusText);
      throw new PktApiError(res.status, text);
    }
    return res.json() as Promise<T>;
  }

  // ── Block API ──────────────────────────────────────────────────────────────

  /** Lấy block theo height. */
  async getBlock(height: number): Promise<Block> {
    return this.get<Block>(`/api/testnet/block/${height}`);
  }

  /** Lấy danh sách blocks (paginated). */
  async getBlocks(page = 0, limit = 25): Promise<BlockPage> {
    return this.get<BlockPage>(`/api/testnet/headers?page=${page}&limit=${limit}`);
  }

  // ── Transaction API ────────────────────────────────────────────────────────

  /** Lấy transaction theo txid. */
  async getTx(txid: string): Promise<Tx> {
    return this.get<Tx>(`/api/testnet/tx/${txid}`);
  }

  /** Lấy danh sách transactions gần nhất. */
  async getRecentTxs(page = 0, limit = 25): Promise<TxPage> {
    return this.get<TxPage>(`/api/testnet/txs?page=${page}&limit=${limit}`);
  }

  // ── Address API ────────────────────────────────────────────────────────────

  /** Lấy thông tin địa chỉ (balance, tx_count, ...). */
  async getAddress(address: string): Promise<AddressInfo> {
    return this.get<AddressInfo>(`/api/testnet/balance/${address}`);
  }

  /** Lấy danh sách UTXOs của địa chỉ. */
  async getUtxos(address: string): Promise<Utxo[]> {
    return this.get<Utxo[]>(`/api/testnet/utxos/${address}`);
  }

  /** Lấy lịch sử giao dịch của địa chỉ (paginated). */
  async getAddressTxs(address: string, page = 0, limit = 25): Promise<TxPage> {
    return this.get<TxPage>(`/api/testnet/address/${address}/txs?page=${page}&limit=${limit}`);
  }

  /** URL download CSV lịch sử giao dịch của địa chỉ. */
  exportAddressCsvUrl(address: string): string {
    return `${this.base}/api/testnet/address/${address}/export.csv`;
  }

  /** URL download CSV blocks. */
  exportBlocksCsvUrl(from: number, to: number): string {
    return `${this.base}/api/testnet/blocks/export.csv?from=${from}&to=${to}`;
  }

  // ── Network API ────────────────────────────────────────────────────────────

  /** Lấy tóm tắt mạng (hashrate, difficulty, mempool, ...). */
  async getSummary(): Promise<NetworkSummary> {
    return this.get<NetworkSummary>("/api/testnet/summary");
  }

  /** Lấy trạng thái sync. */
  async getSyncStatus(): Promise<SyncStatus> {
    return this.get<SyncStatus>("/api/testnet/sync-status");
  }

  /** Lấy mempool hiện tại. */
  async getMempool(): Promise<MempoolTx[]> {
    return this.get<MempoolTx[]>("/api/testnet/mempool");
  }

  /** Lấy fee histogram của mempool. */
  async getFeeHistogram(): Promise<FeeHistogramBin[]> {
    return this.get<FeeHistogramBin[]>("/api/testnet/mempool/fee-histogram");
  }

  /** Lấy dữ liệu analytics (hashrate/difficulty time-series). */
  async getAnalytics(): Promise<AnalyticsPoint[]> {
    return this.get<AnalyticsPoint[]>("/api/testnet/analytics");
  }

  /** Lấy rich list (top địa chỉ nhiều PKT nhất). */
  async getRichList(): Promise<RichListEntry[]> {
    return this.get<RichListEntry[]>("/api/testnet/rich-list");
  }

  /** Lấy label của script/address. */
  async getLabel(script: string): Promise<AddressLabel> {
    return this.get<AddressLabel>(`/api/testnet/label/${script}`);
  }

  /** Lấy health status của node. */
  async getHealth(): Promise<HealthStatus> {
    return this.get<HealthStatus>("/api/health/detailed");
  }

  /** Full-text search (block height, txid, address). */
  async search(query: string): Promise<unknown> {
    return this.get<unknown>(`/api/testnet/search?q=${encodeURIComponent(query)}`);
  }

  // ── JSON-RPC ───────────────────────────────────────────────────────────────

  /** Gọi JSON-RPC 2.0 method trực tiếp. */
  async rpc<T = unknown>(method: string, params: unknown[] = []): Promise<T> {
    const id = ++this.rpcId;
    const body: RpcRequest = { jsonrpc: "2.0", id, method, params };
    const res = await fetch(`${this.base}/rpc`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    if (!res.ok) {
      throw new PktApiError(res.status, res.statusText);
    }
    const json = await res.json() as RpcResponse<T>;
    if (json.error) {
      throw new Error(`RPC error ${json.error.code}: ${json.error.message}`);
    }
    return json.result as T;
  }

  /** getblockcount — trả về block height hiện tại. */
  async getBlockCount(): Promise<number> {
    return this.rpc<number>("getblockcount");
  }

  /** getblockhash — trả về hash của block tại height. */
  async getBlockHash(height: number): Promise<string> {
    return this.rpc<string>("getblockhash", [height]);
  }

  /** getmininginfo — thông tin mining hiện tại. */
  async getMiningInfo(): Promise<unknown> {
    return this.rpc("getmininginfo");
  }

  // ── WebSocket Subscribe ────────────────────────────────────────────────────

  /**
   * Subscribe nhận events realtime qua WebSocket.
   *
   * @param event  "block" | "tx" | "mempool" | "ping"
   * @param cb     callback nhận WsEvent
   * @returns      hàm unsubscribe
   *
   * @example
   * const unsub = client.subscribe("block", (e) => console.log(e.data));
   * // sau đó: unsub();
   */
  subscribe(event: string, cb: EventCallback): Unsubscribe {
    if (!this.wsListeners.has(event)) {
      this.wsListeners.set(event, new Set());
    }
    this.wsListeners.get(event)!.add(cb);
    this.ensureWsConnected();
    return () => {
      this.wsListeners.get(event)?.delete(cb);
    };
  }

  private ensureWsConnected(): void {
    if (this.ws && this.ws.readyState <= WebSocket.OPEN) return;
    const wsUrl = this.base.replace(/^http/, "ws") + "/ws/live";
    this.ws = new WebSocket(wsUrl);
    this.ws.onmessage = (msg: MessageEvent) => {
      try {
        const event = JSON.parse(msg.data as string) as WsEvent;
        const listeners = this.wsListeners.get(event.type);
        listeners?.forEach((cb) => cb(event));
      } catch {
        // ignore malformed messages
      }
    };
    this.ws.onclose = () => {
      this.ws = null;
      // Auto-reconnect sau 3 giây nếu còn listeners
      const hasListeners = [...this.wsListeners.values()].some((s) => s.size > 0);
      if (hasListeners) {
        setTimeout(() => this.ensureWsConnected(), 3000);
      }
    };
  }

  /** Đóng WebSocket connection. */
  close(): void {
    this.ws?.close();
    this.ws = null;
    this.wsListeners.clear();
  }
}
