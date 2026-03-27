"use strict";
// v19.5.1 — PKTCore SDK Client
Object.defineProperty(exports, "__esModule", { value: true });
exports.PktClient = exports.PktApiError = void 0;
class PktApiError extends Error {
    constructor(status, message) {
        super(`HTTP ${status}: ${message}`);
        this.status = status;
        this.name = "PktApiError";
    }
}
exports.PktApiError = PktApiError;
class PktClient {
    /**
     * @param baseUrl  URL gốc của PKTScan, vd: "https://oceif.com"
     */
    constructor(baseUrl) {
        this.ws = null;
        this.wsListeners = new Map();
        this.rpcId = 0;
        this.base = baseUrl.replace(/\/$/, "");
    }
    // ── Private helpers ────────────────────────────────────────────────────────
    async get(path) {
        const res = await fetch(`${this.base}${path}`);
        if (!res.ok) {
            const text = await res.text().catch(() => res.statusText);
            throw new PktApiError(res.status, text);
        }
        return res.json();
    }
    // ── Block API ──────────────────────────────────────────────────────────────
    /** Lấy block theo height. */
    async getBlock(height) {
        return this.get(`/api/testnet/block/${height}`);
    }
    /** Lấy danh sách blocks (paginated). */
    async getBlocks(page = 0, limit = 25) {
        return this.get(`/api/testnet/headers?page=${page}&limit=${limit}`);
    }
    // ── Transaction API ────────────────────────────────────────────────────────
    /** Lấy transaction theo txid. */
    async getTx(txid) {
        return this.get(`/api/testnet/tx/${txid}`);
    }
    /** Lấy danh sách transactions gần nhất. */
    async getRecentTxs(page = 0, limit = 25) {
        return this.get(`/api/testnet/txs?page=${page}&limit=${limit}`);
    }
    // ── Address API ────────────────────────────────────────────────────────────
    /** Lấy thông tin địa chỉ (balance, tx_count, ...). */
    async getAddress(address) {
        return this.get(`/api/testnet/balance/${address}`);
    }
    /** Lấy danh sách UTXOs của địa chỉ. */
    async getUtxos(address) {
        return this.get(`/api/testnet/utxos/${address}`);
    }
    /** Lấy lịch sử giao dịch của địa chỉ (paginated). */
    async getAddressTxs(address, page = 0, limit = 25) {
        return this.get(`/api/testnet/address/${address}/txs?page=${page}&limit=${limit}`);
    }
    /** URL download CSV lịch sử giao dịch của địa chỉ. */
    exportAddressCsvUrl(address) {
        return `${this.base}/api/testnet/address/${address}/export.csv`;
    }
    /** URL download CSV blocks. */
    exportBlocksCsvUrl(from, to) {
        return `${this.base}/api/testnet/blocks/export.csv?from=${from}&to=${to}`;
    }
    // ── Network API ────────────────────────────────────────────────────────────
    /** Lấy tóm tắt mạng (hashrate, difficulty, mempool, ...). */
    async getSummary() {
        return this.get("/api/testnet/summary");
    }
    /** Lấy trạng thái sync. */
    async getSyncStatus() {
        return this.get("/api/testnet/sync-status");
    }
    /** Lấy mempool hiện tại. */
    async getMempool() {
        return this.get("/api/testnet/mempool");
    }
    /** Lấy fee histogram của mempool. */
    async getFeeHistogram() {
        return this.get("/api/testnet/mempool/fee-histogram");
    }
    /** Lấy dữ liệu analytics (hashrate/difficulty time-series). */
    async getAnalytics() {
        return this.get("/api/testnet/analytics");
    }
    /** Lấy rich list (top địa chỉ nhiều PKT nhất). */
    async getRichList() {
        return this.get("/api/testnet/rich-list");
    }
    /** Lấy label của script/address. */
    async getLabel(script) {
        return this.get(`/api/testnet/label/${script}`);
    }
    /** Lấy health status của node. */
    async getHealth() {
        return this.get("/api/health/detailed");
    }
    /** Full-text search (block height, txid, address). */
    async search(query) {
        return this.get(`/api/testnet/search?q=${encodeURIComponent(query)}`);
    }
    // ── JSON-RPC ───────────────────────────────────────────────────────────────
    /** Gọi JSON-RPC 2.0 method trực tiếp. */
    async rpc(method, params = []) {
        const id = ++this.rpcId;
        const body = { jsonrpc: "2.0", id, method, params };
        const res = await fetch(`${this.base}/rpc`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(body),
        });
        if (!res.ok) {
            throw new PktApiError(res.status, res.statusText);
        }
        const json = await res.json();
        if (json.error) {
            throw new Error(`RPC error ${json.error.code}: ${json.error.message}`);
        }
        return json.result;
    }
    /** getblockcount — trả về block height hiện tại. */
    async getBlockCount() {
        return this.rpc("getblockcount");
    }
    /** getblockhash — trả về hash của block tại height. */
    async getBlockHash(height) {
        return this.rpc("getblockhash", [height]);
    }
    /** getmininginfo — thông tin mining hiện tại. */
    async getMiningInfo() {
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
    subscribe(event, cb) {
        if (!this.wsListeners.has(event)) {
            this.wsListeners.set(event, new Set());
        }
        this.wsListeners.get(event).add(cb);
        this.ensureWsConnected();
        return () => {
            this.wsListeners.get(event)?.delete(cb);
        };
    }
    ensureWsConnected() {
        if (this.ws && this.ws.readyState <= WebSocket.OPEN)
            return;
        const wsUrl = this.base.replace(/^http/, "ws") + "/ws/live";
        this.ws = new WebSocket(wsUrl);
        this.ws.onmessage = (msg) => {
            try {
                const event = JSON.parse(msg.data);
                const listeners = this.wsListeners.get(event.type);
                listeners?.forEach((cb) => cb(event));
            }
            catch {
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
    close() {
        this.ws?.close();
        this.ws = null;
        this.wsListeners.clear();
    }
}
exports.PktClient = PktClient;
//# sourceMappingURL=client.js.map