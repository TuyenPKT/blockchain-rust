export declare const PAKLETS_PER_PKT = 1073741824;
/** Chuyển paklets → PKT (float). */
export declare function pakletsToPkt(paklets: number): number;
/** Chuyển PKT → paklets (integer). */
export declare function pktToPaklets(pkt: number): number;
/** Rút gọn hash: "abcd1234…efgh5678" (8 ký tự đầu + … + 8 cuối). */
export declare function shortHash(hash: string): string;
/** Rút gọn địa chỉ: "pkt1abcd…wxyz" (8 ký tự đầu + … + 4 cuối). */
export declare function shortAddr(addr: string): string;
/** Format hashrate thành string dễ đọc (H/s, KH/s, MH/s, GH/s, TH/s, PH/s). */
export declare function fmtHashrate(hps: number): string;
/** Format paklets thành string PKT dễ đọc, vd "1,234.56 PKT". */
export declare function fmtPkt(paklets: number): string;
/** Thời gian tương đối: "2 phút trước", "1 giờ trước", ... */
export declare function timeAgo(unixSecs: number): string;
//# sourceMappingURL=utils.d.ts.map