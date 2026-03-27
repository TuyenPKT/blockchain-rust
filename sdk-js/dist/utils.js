"use strict";
// v19.5.1 — PKTCore SDK Utils
Object.defineProperty(exports, "__esModule", { value: true });
exports.PAKLETS_PER_PKT = void 0;
exports.pakletsToPkt = pakletsToPkt;
exports.pktToPaklets = pktToPaklets;
exports.shortHash = shortHash;
exports.shortAddr = shortAddr;
exports.fmtHashrate = fmtHashrate;
exports.fmtPkt = fmtPkt;
exports.timeAgo = timeAgo;
exports.PAKLETS_PER_PKT = 1073741824; // 2^30
/** Chuyển paklets → PKT (float). */
function pakletsToPkt(paklets) {
    return paklets / exports.PAKLETS_PER_PKT;
}
/** Chuyển PKT → paklets (integer). */
function pktToPaklets(pkt) {
    return Math.round(pkt * exports.PAKLETS_PER_PKT);
}
/** Rút gọn hash: "abcd1234…efgh5678" (8 ký tự đầu + … + 8 cuối). */
function shortHash(hash) {
    if (hash.length <= 20)
        return hash;
    return `${hash.slice(0, 8)}…${hash.slice(-8)}`;
}
/** Rút gọn địa chỉ: "pkt1abcd…wxyz" (8 ký tự đầu + … + 4 cuối). */
function shortAddr(addr) {
    if (addr.length <= 16)
        return addr;
    return `${addr.slice(0, 8)}…${addr.slice(-4)}`;
}
/** Format hashrate thành string dễ đọc (H/s, KH/s, MH/s, GH/s, TH/s, PH/s). */
function fmtHashrate(hps) {
    const units = ["H/s", "KH/s", "MH/s", "GH/s", "TH/s", "PH/s", "EH/s"];
    let val = hps;
    let i = 0;
    while (val >= 1000 && i < units.length - 1) {
        val /= 1000;
        i++;
    }
    return `${val.toFixed(2)} ${units[i]}`;
}
/** Format paklets thành string PKT dễ đọc, vd "1,234.56 PKT". */
function fmtPkt(paklets) {
    const pkt = pakletsToPkt(paklets);
    return `${pkt.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 6 })} PKT`;
}
/** Thời gian tương đối: "2 phút trước", "1 giờ trước", ... */
function timeAgo(unixSecs) {
    const diff = Math.floor(Date.now() / 1000) - unixSecs;
    if (diff < 60)
        return `${diff}s trước`;
    if (diff < 3600)
        return `${Math.floor(diff / 60)}m trước`;
    if (diff < 86400)
        return `${Math.floor(diff / 3600)}h trước`;
    return `${Math.floor(diff / 86400)}d trước`;
}
//# sourceMappingURL=utils.js.map