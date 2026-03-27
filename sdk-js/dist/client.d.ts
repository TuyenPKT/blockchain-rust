import type { Block, BlockPage, Tx, TxPage, AddressInfo, Utxo, SyncStatus, NetworkSummary, MempoolTx, FeeHistogramBin, AnalyticsPoint, RichListEntry, AddressLabel, HealthStatus, EventCallback, Unsubscribe } from "./types.js";
export declare class PktApiError extends Error {
    readonly status: number;
    constructor(status: number, message: string);
}
export declare class PktClient {
    private readonly base;
    private ws;
    private wsListeners;
    private rpcId;
    /**
     * @param baseUrl  URL gốc của PKTScan, vd: "https://oceif.com"
     */
    constructor(baseUrl: string);
    private get;
    /** Lấy block theo height. */
    getBlock(height: number): Promise<Block>;
    /** Lấy danh sách blocks (paginated). */
    getBlocks(page?: number, limit?: number): Promise<BlockPage>;
    /** Lấy transaction theo txid. */
    getTx(txid: string): Promise<Tx>;
    /** Lấy danh sách transactions gần nhất. */
    getRecentTxs(page?: number, limit?: number): Promise<TxPage>;
    /** Lấy thông tin địa chỉ (balance, tx_count, ...). */
    getAddress(address: string): Promise<AddressInfo>;
    /** Lấy danh sách UTXOs của địa chỉ. */
    getUtxos(address: string): Promise<Utxo[]>;
    /** Lấy lịch sử giao dịch của địa chỉ (paginated). */
    getAddressTxs(address: string, page?: number, limit?: number): Promise<TxPage>;
    /** URL download CSV lịch sử giao dịch của địa chỉ. */
    exportAddressCsvUrl(address: string): string;
    /** URL download CSV blocks. */
    exportBlocksCsvUrl(from: number, to: number): string;
    /** Lấy tóm tắt mạng (hashrate, difficulty, mempool, ...). */
    getSummary(): Promise<NetworkSummary>;
    /** Lấy trạng thái sync. */
    getSyncStatus(): Promise<SyncStatus>;
    /** Lấy mempool hiện tại. */
    getMempool(): Promise<MempoolTx[]>;
    /** Lấy fee histogram của mempool. */
    getFeeHistogram(): Promise<FeeHistogramBin[]>;
    /** Lấy dữ liệu analytics (hashrate/difficulty time-series). */
    getAnalytics(): Promise<AnalyticsPoint[]>;
    /** Lấy rich list (top địa chỉ nhiều PKT nhất). */
    getRichList(): Promise<RichListEntry[]>;
    /** Lấy label của script/address. */
    getLabel(script: string): Promise<AddressLabel>;
    /** Lấy health status của node. */
    getHealth(): Promise<HealthStatus>;
    /** Full-text search (block height, txid, address). */
    search(query: string): Promise<unknown>;
    /** Gọi JSON-RPC 2.0 method trực tiếp. */
    rpc<T = unknown>(method: string, params?: unknown[]): Promise<T>;
    /** getblockcount — trả về block height hiện tại. */
    getBlockCount(): Promise<number>;
    /** getblockhash — trả về hash của block tại height. */
    getBlockHash(height: number): Promise<string>;
    /** getmininginfo — thông tin mining hiện tại. */
    getMiningInfo(): Promise<unknown>;
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
    subscribe(event: string, cb: EventCallback): Unsubscribe;
    private ensureWsConnected;
    /** Đóng WebSocket connection. */
    close(): void;
}
//# sourceMappingURL=client.d.ts.map