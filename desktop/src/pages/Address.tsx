// Address.tsx — v20.5: Address Detail UI
// Balance hero · Copy · QR modal · TX history paginated · UTXO list
import { useState, useEffect, useCallback } from "react";
import { colors, fonts } from "../theme";
import { Panel } from "../components/Panel";
import { useAnimatedNumber } from "../hooks/useAnimatedNumber";
import {
  fetchBalance, fetchAddressTxs, fetchAddressUtxos,
  fmtNum, fmtPkt, timeAgo, shortHash, PACKETS_PER_PKT,
  type AddressTx, type AddressUtxo,
} from "../api";

interface AddressProps {
  nodeUrl: string;
  address: string;         // pre-filled from search
  onBack:  () => void;
}

const PAGE_SIZE = 20;

// ── QR modal (address display, copy) ─────────────────────────────────────────

function QrModal({ address, onClose }: { address: string; onClose: () => void }) {
  const [copied, setCopied] = useState(false);

  function copy() {
    navigator.clipboard.writeText(address).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1800);
    });
  }

  return (
    <>
      <div onClick={onClose} style={{
        position: "fixed", inset: 0, zIndex: 999,
        background: "rgba(0,0,0,0.6)", backdropFilter: "blur(4px)",
      }} />
      <div style={{
        position: "fixed", top: "50%", left: "50%",
        transform: "translate(-50%,-50%)",
        zIndex: 1000, width: 380,
        background: colors.surface,
        border: `1px solid ${colors.border}`,
        borderRadius: 20,
        boxShadow: "0 24px 80px rgba(0,0,0,0.65)",
        padding: 32, textAlign: "center",
      }}>
        {/* Close */}
        <button onClick={onClose} style={{
          position: "absolute", top: 14, right: 16,
          background: "none", border: "none", cursor: "pointer",
          color: colors.muted, fontSize: 18,
        }}>✕</button>

        <div style={{ fontSize: 13, color: colors.muted, marginBottom: 20, fontWeight: 600 }}>
          PKT Address
        </div>

        {/* QR placeholder visual */}
        <div style={{
          width: 200, height: 200, margin: "0 auto 20px",
          background: colors.surface2,
          border: `2px solid ${colors.border}`,
          borderRadius: 12,
          display: "flex", flexDirection: "column",
          alignItems: "center", justifyContent: "center",
          gap: 8,
        }}>
          <QrPattern address={address} />
        </div>

        {/* Address text */}
        <div style={{
          fontFamily: fonts.mono, fontSize: 11,
          color: colors.text, wordBreak: "break-all",
          background: colors.surface2, border: `1px solid ${colors.border}`,
          borderRadius: 10, padding: "12px 16px", marginBottom: 16,
          lineHeight: 1.7,
        }}>
          {address}
        </div>

        <button onClick={copy} style={{
          width: "100%", padding: "11px 0",
          background: copied ? colors.green : colors.accent,
          color: "#000", border: "none", borderRadius: 10,
          fontWeight: 700, fontSize: 14, cursor: "pointer",
          transition: "background .2s",
        }}>
          {copied ? "✓ Copied!" : "Copy Address"}
        </button>
      </div>
    </>
  );
}

// Simple deterministic pixel pattern based on address chars
function QrPattern({ address }: { address: string }) {
  const SIZE = 13;
  const cells: boolean[] = [];
  for (let i = 0; i < SIZE * SIZE; i++) {
    const c = address.charCodeAt(i % address.length);
    cells.push((c ^ (i * 17)) % 3 !== 0);
  }
  return (
    <div style={{
      display: "grid",
      gridTemplateColumns: `repeat(${SIZE}, 1fr)`,
      gap: 1, padding: 12,
    }}>
      {cells.map((on, i) => (
        <div key={i} style={{
          width: 11, height: 11, borderRadius: 1,
          background: on ? colors.accent : colors.surface,
        }} />
      ))}
    </div>
  );
}

// ── Balance Hero ──────────────────────────────────────────────────────────────

function BalanceHero({
  address, balance, onQr,
}: { address: string; balance: number; onQr: () => void }) {
  const animated = useAnimatedNumber(balance);
  const [copied, setCopied] = useState(false);

  function copy() {
    navigator.clipboard.writeText(address).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1800);
    });
  }

  const pkt = (animated / PACKETS_PER_PKT).toFixed(4);

  return (
    <div style={{
      background: colors.surface,
      border: `1px solid ${colors.border}`,
      borderRadius: 16, padding: "28px 32px",
      marginBottom: 20, position: "relative", overflow: "hidden",
    }}>
      {/* Glow accent */}
      <div style={{
        position: "absolute", top: 0, left: 0, right: 0, height: 3,
        background: `linear-gradient(90deg, transparent, ${colors.accent}88, transparent)`,
      }} />

      <div style={{ display: "flex", alignItems: "flex-start", gap: 20 }}>
        {/* Icon */}
        <div style={{
          width: 56, height: 56, borderRadius: 14, flexShrink: 0,
          background: `${colors.accent}18`, border: `1.5px solid ${colors.accent}44`,
          display: "flex", alignItems: "center", justifyContent: "center",
          fontSize: 26,
        }}>💼</div>

        {/* Address + balance */}
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ fontSize: 11, color: colors.muted, textTransform: "uppercase", letterSpacing: ".08em", marginBottom: 4 }}>
            PKT Address
          </div>
          <div style={{
            fontFamily: fonts.mono, fontSize: 13, color: colors.blue,
            overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
            marginBottom: 14,
          }}>
            {address}
          </div>

          <div style={{
            fontFamily: fonts.mono, fontSize: 36, fontWeight: 800,
            color: colors.accent, letterSpacing: "-.02em", lineHeight: 1,
          }}>
            {parseFloat(pkt).toLocaleString(undefined, { minimumFractionDigits: 4 })}
            <span style={{ fontSize: 18, color: colors.muted, marginLeft: 8, fontWeight: 600 }}>PKT</span>
          </div>
        </div>

        {/* Action buttons */}
        <div style={{ display: "flex", gap: 8, flexShrink: 0 }}>
          <button onClick={copy} title="Copy address" style={{
            display: "flex", alignItems: "center", gap: 6,
            padding: "8px 14px",
            background: copied ? `${colors.green}22` : colors.surface2,
            border: `1px solid ${copied ? colors.green : colors.border}`,
            borderRadius: 8, cursor: "pointer",
            color: copied ? colors.green : colors.muted,
            fontFamily: fonts.sans, fontSize: 13, fontWeight: 600,
            transition: "all .2s",
          }}>
            {copied ? "✓" : (
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none"
                stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <rect x="9" y="9" width="13" height="13" rx="2" ry="2"/>
                <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/>
              </svg>
            )}
            {copied ? "Copied" : "Copy"}
          </button>

          <button onClick={onQr} title="Show QR code" style={{
            display: "flex", alignItems: "center", gap: 6,
            padding: "8px 14px",
            background: colors.surface2, border: `1px solid ${colors.border}`,
            borderRadius: 8, cursor: "pointer", color: colors.muted,
            fontFamily: fonts.sans, fontSize: 13, fontWeight: 600,
            transition: "border-color .2s",
          }}
            onMouseEnter={e => (e.currentTarget.style.borderColor = colors.accent)}
            onMouseLeave={e => (e.currentTarget.style.borderColor = colors.border)}
          >
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none"
              stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/>
              <rect x="3" y="14" width="7" height="7"/>
              <path d="M14 14h3v3M17 17v4M21 14v3"/>
            </svg>
            QR
          </button>
        </div>
      </div>
    </div>
  );
}

// ── TX History table ──────────────────────────────────────────────────────────

function TxTable({
  txs, page, total, onPage, loading,
}: {
  txs: AddressTx[];
  page: number;
  total: number;
  onPage: (p: number) => void;
  loading: boolean;
}) {
  const totalPages = Math.max(1, Math.ceil(total / PAGE_SIZE));

  return (
    <div>
      <div style={{ overflowX: "auto" }}>
        <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 13 }}>
          <thead>
            <tr style={{ borderBottom: `1px solid ${colors.border}` }}>
              {["Tx Hash", "Method", "Block", "Age", "From", "", "To", "Amount", "Txn Fee"].map(h => (
                <th key={h} style={{
                  padding: "10px 12px", textAlign: "left",
                  fontSize: 10, fontWeight: 700, textTransform: "uppercase",
                  letterSpacing: ".08em", color: colors.muted, whiteSpace: "nowrap",
                }}>{h}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {loading && (
              <tr><td colSpan={9} style={{ padding: 24, textAlign: "center", color: colors.muted }}>Loading…</td></tr>
            )}
            {!loading && txs.length === 0 && (
              <tr><td colSpan={9} style={{ padding: 24, textAlign: "center", color: colors.muted }}>No transactions found</td></tr>
            )}
            {txs.map((tx, i) => {
              const id       = (tx.txid ?? tx.hash ?? "") as string;
              const netSat   = (tx.net_sat ?? 0) as number;
              const isRecv   = netSat > 0;
              const isSent   = netSat < 0;
              const amtStr   = netSat === 0
                ? "—"
                : `${isRecv ? "+" : ""}${(netSat / PACKETS_PER_PKT).toLocaleString(undefined, { maximumFractionDigits: 4 })} PKT`;
              const amtColor = isRecv ? colors.green : isSent ? colors.red : colors.muted;
              const shortTx  = id.length >= 14 ? id.slice(0, 8) + "…" + id.slice(-6) : id;
              const from     = (tx.from ?? "") as string;
              const to       = (tx.to   ?? "") as string;
              const feeSat   = (tx.fee_sat ?? 0) as number;
              const ts       = (tx.timestamp ?? 0) as number;
              const height   = tx.height as number | undefined;
              const isCoinbase = !from;
              const isSelf   = from && to && from === to;
              const method   = isCoinbase ? "Coinbase" : isSelf ? "Transfer*" : "Transfer";
              const shortAddr = (a: string) => a.length >= 12 ? a.slice(0, 8) + "…" + a.slice(-4) : a || "—";
              return (
                <tr key={i} style={{ borderBottom: `1px solid ${colors.border}`, transition: "background .15s" }}
                  onMouseEnter={e => (e.currentTarget.style.background = colors.surface2)}
                  onMouseLeave={e => (e.currentTarget.style.background = "transparent")}
                >
                  {/* Tx Hash */}
                  <td style={{ padding: "10px 12px", fontFamily: fonts.mono, fontSize: 11, color: colors.blue }}>
                    {id ? shortTx : "—"}
                  </td>
                  {/* Method */}
                  <td style={{ padding: "10px 12px" }}>
                    <span style={{
                      fontSize: 10, fontWeight: 600, padding: "2px 10px", borderRadius: 4,
                      border: `1px solid ${colors.border}`,
                      background: colors.surface2, color: colors.text,
                    }}>
                      {method}
                    </span>
                  </td>
                  {/* Block */}
                  <td style={{ padding: "10px 12px", fontFamily: fonts.mono, fontSize: 11, color: colors.accent }}>
                    {height !== undefined ? height.toLocaleString() : "—"}
                  </td>
                  {/* Age */}
                  <td style={{ padding: "10px 12px", color: colors.muted, fontSize: 11, whiteSpace: "nowrap" }}>
                    {ts > 0 ? timeAgo(ts) : "—"}
                  </td>
                  {/* From */}
                  <td style={{ padding: "10px 12px", fontFamily: fonts.mono, fontSize: 11, color: colors.blue }}>
                    <span title={from}>{shortAddr(from)}</span>
                  </td>
                  {/* Arrow */}
                  <td style={{ padding: "10px 4px" }}>
                    <span style={{
                      display: "inline-flex", alignItems: "center", justifyContent: "center",
                      width: 20, height: 20, borderRadius: "50%",
                      background: `${colors.green}20`, color: colors.green, fontSize: 10,
                    }}>→</span>
                  </td>
                  {/* To */}
                  <td style={{ padding: "10px 12px", fontFamily: fonts.mono, fontSize: 11, color: colors.blue }}>
                    {isSelf
                      ? <span style={{ fontSize: 10, padding: "1px 6px", borderRadius: 4, border: `1px solid ${colors.border}`, color: colors.muted }}>SELF</span>
                      : <span title={to}>{shortAddr(to)}</span>}
                  </td>
                  {/* Amount */}
                  <td style={{ padding: "10px 12px", fontFamily: fonts.mono, fontSize: 11, fontWeight: 600, color: amtColor }}>
                    {amtStr}
                  </td>
                  {/* Txn Fee */}
                  <td style={{ padding: "10px 12px", fontFamily: fonts.mono, fontSize: 11, color: colors.muted }}>
                    {feeSat > 0 ? fmtPkt(feeSat) : "—"}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      {/* Pagination */}
      <div style={{
        display: "flex", justifyContent: "space-between", alignItems: "center",
        padding: "12px 16px", borderTop: `1px solid ${colors.border}`,
      }}>
        <span style={{ fontSize: 12, color: colors.muted }}>
          {total > 0 ? `${fmtNum(total)} transactions total` : ""}
        </span>
        <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
          <button onClick={() => onPage(page - 1)} disabled={page === 0} style={paginBtn(page === 0)}>
            ← Prev
          </button>
          <span style={{ fontSize: 12, color: colors.muted, fontFamily: fonts.mono, padding: "0 8px" }}>
            {page + 1} / {totalPages}
          </span>
          <button onClick={() => onPage(page + 1)} disabled={page >= totalPages - 1} style={paginBtn(page >= totalPages - 1)}>
            Next →
          </button>
        </div>
      </div>
    </div>
  );
}

function paginBtn(disabled: boolean): React.CSSProperties {
  return {
    padding: "5px 14px",
    background: disabled ? "transparent" : colors.surface2,
    border: `1px solid ${colors.border}`,
    borderRadius: 6, cursor: disabled ? "default" : "pointer",
    color: disabled ? colors.muted : colors.text,
    fontFamily: fonts.sans, fontSize: 12, opacity: disabled ? .4 : 1,
  };
}

// ── UTXO list ─────────────────────────────────────────────────────────────────

function UtxoList({ utxos, loading }: { utxos: AddressUtxo[]; loading: boolean }) {
  const total = utxos.reduce((s, u) => s + (u.amount ?? 0), 0);

  return (
    <div>
      {utxos.length > 0 && (
        <div style={{
          padding: "10px 16px", borderBottom: `1px solid ${colors.border}`,
          display: "flex", justifyContent: "space-between", alignItems: "center",
        }}>
          <span style={{ fontSize: 12, color: colors.muted }}>
            {utxos.length} unspent output{utxos.length !== 1 ? "s" : ""}
          </span>
          <span style={{ fontFamily: fonts.mono, fontSize: 13, color: colors.green, fontWeight: 700 }}>
            {(total / PACKETS_PER_PKT).toFixed(4)} PKT
          </span>
        </div>
      )}
      <div style={{ overflowX: "auto" }}>
        <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 13 }}>
          <thead>
            <tr style={{ borderBottom: `1px solid ${colors.border}` }}>
              {["TXID", "Vout", "Amount (PKT)", "Height"].map(h => (
                <th key={h} style={{
                  padding: "10px 16px", textAlign: "left",
                  fontSize: 10, fontWeight: 700, textTransform: "uppercase",
                  letterSpacing: ".08em", color: colors.muted,
                }}>{h}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {loading && (
              <tr><td colSpan={4} style={{ padding: 24, textAlign: "center", color: colors.muted }}>Loading…</td></tr>
            )}
            {!loading && utxos.length === 0 && (
              <tr><td colSpan={4} style={{ padding: 24, textAlign: "center", color: colors.muted }}>No UTXOs found</td></tr>
            )}
            {utxos.map((u, i) => (
              <tr key={i} style={{ borderBottom: `1px solid ${colors.border}`, transition: "background .15s" }}
                onMouseEnter={e => (e.currentTarget.style.background = colors.surface2)}
                onMouseLeave={e => (e.currentTarget.style.background = "transparent")}
              >
                <td style={{ padding: "11px 16px", fontFamily: fonts.mono, fontSize: 12, color: colors.blue }}>
                  {u.txid ? shortHash(u.txid) : "—"}
                </td>
                <td style={{ padding: "11px 16px", fontFamily: fonts.mono, color: colors.muted }}>
                  {u.vout ?? "—"}
                </td>
                <td style={{ padding: "11px 16px", fontFamily: fonts.mono, fontWeight: 700, color: colors.green }}>
                  {u.amount !== undefined ? (u.amount / PACKETS_PER_PKT).toFixed(4) : "—"}
                </td>
                <td style={{ padding: "11px 16px", fontFamily: fonts.mono, color: colors.accent }}>
                  {u.height !== undefined ? `#${fmtNum(u.height)}` : "—"}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// ── Main Address page ─────────────────────────────────────────────────────────

export function Address({ nodeUrl, address, onBack }: AddressProps) {
  const [balance,    setBalance]    = useState(0);
  const [txs,        setTxs]        = useState<AddressTx[]>([]);
  const [txTotal,    setTxTotal]    = useState(0);
  const [utxos,      setUtxos]      = useState<AddressUtxo[]>([]);
  const [page,       setPage]       = useState(0);
  const [loadBal,    setLoadBal]    = useState(true);
  const [loadTxs,    setLoadTxs]    = useState(true);
  const [loadUtxos,  setLoadUtxos]  = useState(true);
  const [showQr,     setShowQr]     = useState(false);
  const [activeTab,  setActiveTab]  = useState<"txs" | "utxos">("txs");

  // Load balance
  useEffect(() => {
    if (!address) return;
    setLoadBal(true);
    fetchBalance(nodeUrl, address)
      .then(d => {
        const r = d as Record<string, unknown>;
        const b = r["balance"] as number | undefined;
        setBalance(b ?? 0);
      })
      .catch(() => {})
      .finally(() => setLoadBal(false));
  }, [nodeUrl, address]);

  // Load TXs (re-fetches on page change)
  const loadTxs_ = useCallback(async (p: number) => {
    setLoadTxs(true);
    try {
      const d = await fetchAddressTxs(nodeUrl, address, p, PAGE_SIZE);
      setTxs(d.txs ?? []);
      setTxTotal(d.total ?? 0);
    } catch (_) {
      setTxs([]);
    } finally {
      setLoadTxs(false);
    }
  }, [nodeUrl, address]);

  useEffect(() => { loadTxs_(page); }, [loadTxs_, page]);

  // Load UTXOs once
  useEffect(() => {
    if (!address) return;
    setLoadUtxos(true);
    fetchAddressUtxos(nodeUrl, address)
      .then(d => setUtxos(d.utxos ?? []))
      .catch(() => setUtxos([]))
      .finally(() => setLoadUtxos(false));
  }, [nodeUrl, address]);

  function handlePage(p: number) {
    setPage(p);
    window.scrollTo({ top: 0, behavior: "smooth" });
  }

  return (
    <div>
      {/* Back button */}
      <button onClick={onBack} style={{
        display: "flex", alignItems: "center", gap: 6,
        background: "none", border: "none", cursor: "pointer",
        color: colors.muted, fontFamily: fonts.sans, fontSize: 13,
        marginBottom: 16, padding: 0,
        transition: "color .15s",
      }}
        onMouseEnter={e => (e.currentTarget.style.color = colors.text)}
        onMouseLeave={e => (e.currentTarget.style.color = colors.muted)}
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
          stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
          <polyline points="15 18 9 12 15 6" />
        </svg>
        Back to Explorer
      </button>

      {/* Balance hero */}
      {!loadBal && (
        <BalanceHero address={address} balance={balance} onQr={() => setShowQr(true)} />
      )}
      {loadBal && (
        <div style={{
          background: colors.surface, border: `1px solid ${colors.border}`,
          borderRadius: 16, padding: 28, marginBottom: 20,
          display: "flex", alignItems: "center", gap: 16,
          color: colors.muted, fontSize: 14,
        }}>
          <div style={{ width: 56, height: 56, borderRadius: 14, background: colors.surface2 }} />
          <span>Loading balance…</span>
        </div>
      )}

      {/* Tab switcher */}
      <div style={{
        display: "flex", gap: 2,
        background: colors.surface2, border: `1px solid ${colors.border}`,
        borderRadius: 10, padding: 3, width: "fit-content",
        marginBottom: 16,
      }}>
        {([["txs", "Transactions"], ["utxos", "UTXOs"]] as const).map(([id, label]) => (
          <button key={id} onClick={() => setActiveTab(id)} style={{
            padding: "7px 22px", border: "none", borderRadius: 8, cursor: "pointer",
            fontFamily: fonts.sans, fontWeight: 600, fontSize: 13,
            background: activeTab === id ? colors.surface : "transparent",
            color: activeTab === id ? colors.text : colors.muted,
            transition: "all .2s",
          }}>
            {label}
            {id === "txs" && txTotal > 0 && (
              <span style={{
                marginLeft: 6, fontSize: 10, padding: "1px 6px",
                background: `${colors.accent}22`, color: colors.accent,
                border: `1px solid ${colors.accent}44`, borderRadius: 4,
              }}>{txTotal}</span>
            )}
            {id === "utxos" && utxos.length > 0 && (
              <span style={{
                marginLeft: 6, fontSize: 10, padding: "1px 6px",
                background: `${colors.green}22`, color: colors.green,
                border: `1px solid ${colors.green}44`, borderRadius: 4,
              }}>{utxos.length}</span>
            )}
          </button>
        ))}
      </div>

      {/* TX history */}
      {activeTab === "txs" && (
        <Panel icon="📋" title="Transaction History"
          right={
            <button onClick={() => loadTxs_(page)} style={{
              padding: "4px 12px", background: colors.surface2,
              border: `1px solid ${colors.border}`, borderRadius: 6,
              color: colors.muted, cursor: "pointer", fontSize: 12,
            }}>↻</button>
          }
        >
          <TxTable txs={txs} page={page} total={txTotal} onPage={handlePage} loading={loadTxs} />
        </Panel>
      )}

      {/* UTXO list */}
      {activeTab === "utxos" && (
        <Panel icon="🔒" title="Unspent Outputs (UTXOs)"
          right={
            <button onClick={() => {
              setLoadUtxos(true);
              fetchAddressUtxos(nodeUrl, address)
                .then(d => setUtxos(d.utxos ?? []))
                .catch(() => {})
                .finally(() => setLoadUtxos(false));
            }} style={{
              padding: "4px 12px", background: colors.surface2,
              border: `1px solid ${colors.border}`, borderRadius: 6,
              color: colors.muted, cursor: "pointer", fontSize: 12,
            }}>↻</button>
          }
        >
          <UtxoList utxos={utxos} loading={loadUtxos} />
        </Panel>
      )}

      {/* QR modal */}
      {showQr && <QrModal address={address} onClose={() => setShowQr(false)} />}
    </div>
  );
}
