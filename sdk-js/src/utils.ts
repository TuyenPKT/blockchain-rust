// v19.5.1 — PKTCore SDK Utils

export const PAKLETS_PER_PKT = 1_073_741_824; // 2^30

/** Chuyển paklets → PKT (float). */
export function pakletsToPkt(paklets: number): number {
  return paklets / PAKLETS_PER_PKT;
}

/** Chuyển PKT → paklets (integer). */
export function pktToPaklets(pkt: number): number {
  return Math.round(pkt * PAKLETS_PER_PKT);
}

/** Rút gọn hash: "abcd1234…efgh5678" (8 ký tự đầu + … + 8 cuối). */
export function shortHash(hash: string): string {
  if (hash.length <= 20) return hash;
  return `${hash.slice(0, 8)}…${hash.slice(-8)}`;
}

/** Rút gọn địa chỉ: "pkt1abcd…wxyz" (8 ký tự đầu + … + 4 cuối). */
export function shortAddr(addr: string): string {
  if (addr.length <= 16) return addr;
  return `${addr.slice(0, 8)}…${addr.slice(-4)}`;
}

/** Format hashrate thành string dễ đọc (H/s, KH/s, MH/s, GH/s, TH/s, PH/s). */
export function fmtHashrate(hps: number): string {
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
export function fmtPkt(paklets: number): string {
  const pkt = pakletsToPkt(paklets);
  return `${pkt.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 6 })} PKT`;
}

/** Thời gian tương đối: "2 phút trước", "1 giờ trước", ... */
export function timeAgo(unixSecs: number): string {
  const diff = Math.floor(Date.now() / 1000) - unixSecs;
  if (diff < 60)      return `${diff}s trước`;
  if (diff < 3600)    return `${Math.floor(diff / 60)}m trước`;
  if (diff < 86400)   return `${Math.floor(diff / 3600)}h trước`;
  return `${Math.floor(diff / 86400)}d trước`;
}
