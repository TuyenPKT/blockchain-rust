// BlockDetail.tsx — v20.6: Block detail page
// Block header metadata + TX list (clickable) + fee + miner + confirmations
import { useState, useEffect } from "react";
import { colors, fonts } from "../theme";
import { Panel } from "../components/Panel";
import { fetchBlockDetail, shortHash, timeAgo, PACKETS_PER_PKT, type BlockDetail, type TxDetail } from "../api";

interface BlockDetailProps {
  nodeUrl:   string;
  height:    number;
  onBack:    () => void;
  onTx:      (txid: string) => void;
}

// ── Metadata row ─────────────────────────────────────────────────────────────

function MetaRow({ label, value, mono = false, accent = false }: {
  label: string; value: string | number | undefined;
  mono?: boolean; accent?: boolean;
}) {
  if (value === undefined || value === null) return null;
  return (
    <div style={{
      display: "flex", justifyContent: "space-between", alignItems: "center",
      padding: "12px 0",
      borderBottom: `1px solid ${colors.border}`,
    }}>
      <span style={{ fontSize: 12, color: colors.muted, fontWeight: 600, textTransform: "uppercase", letterSpacing: ".06em" }}>
        {label}
      </span>
      <span style={{
        fontFamily: mono ? fonts.mono : fonts.sans,
        fontSize: 13,
        color: accent ? colors.accent : colors.text,
        fontWeight: accent ? 700 : 400,
        maxWidth: "60%", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
      }}>
        {value}
      </span>
    </div>
  );
}

// ── Confirmation badge ────────────────────────────────────────────────────────

function ConfBadge({ conf }: { conf: number | undefined }) {
  if (conf === undefined) return null;
  const color = conf === 0 ? colors.red : conf < 6 ? colors.accent : colors.green;
  const label = conf === 0 ? "Unconfirmed" : `${conf} confirmations`;
  return (
    <span style={{
      fontSize: 12, fontWeight: 700, padding: "3px 10px", borderRadius: 6,
      background: color + "22", color, border: `1px solid ${color}44`,
    }}>{label}</span>
  );
}

// ── TX list row ───────────────────────────────────────────────────────────────

function TxRow({ tx, onTx }: { tx: TxDetail; onTx: (txid: string) => void }) {
  const txid = tx.txid ?? tx.hash ?? "";
  const outTotal = (tx.outputs ?? []).reduce((s, o) => s + (Number(o.value) || 0), 0);

  return (
    <div
      onClick={() => txid && onTx(txid)}
      style={{
        display: "flex", alignItems: "center", gap: 16,
        padding: "12px 18px",
        borderBottom: `1px solid ${colors.border}`,
        cursor: txid ? "pointer" : "default",
        transition: "background .15s",
      }}
      onMouseEnter={e => (e.currentTarget.style.background = colors.surface2)}
      onMouseLeave={e => (e.currentTarget.style.background = "transparent")}
    >
      {/* TXID */}
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontFamily: fonts.mono, fontSize: 12, color: colors.blue,
          overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {txid ? shortHash(txid) : "coinbase"}
        </div>
        <div style={{ fontSize: 11, color: colors.muted, marginTop: 3 }}>
          {(tx.inputs?.length ?? 0)} in · {(tx.outputs?.length ?? 0)} out
        </div>
      </div>

      {/* Fee */}
      {tx.fee !== undefined && (
        <div style={{ textAlign: "right", flexShrink: 0 }}>
          <span style={{
            fontSize: 11, fontFamily: fonts.mono, fontWeight: 700,
            color: colors.purple, padding: "2px 8px", borderRadius: 4,
            background: `${colors.purple}18`, border: `1px solid ${colors.purple}33`,
          }}>
            fee {(tx.fee / PACKETS_PER_PKT).toFixed(6)} PKT
          </span>
        </div>
      )}

      {/* Total output */}
      <div style={{ textAlign: "right", flexShrink: 0, minWidth: 90 }}>
        <div style={{ fontFamily: fonts.mono, fontWeight: 700, fontSize: 13, color: colors.green }}>
          {(outTotal / PACKETS_PER_PKT).toFixed(4)}
        </div>
        <div style={{ fontSize: 10, color: colors.muted }}>PKT out</div>
      </div>

      {/* Arrow */}
      {txid && (
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none"
          stroke={colors.muted} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <polyline points="9 18 15 12 9 6" />
        </svg>
      )}
    </div>
  );
}

// ── Main ─────────────────────────────────────────────────────────────────────

export function BlockDetail({ nodeUrl, height, onBack, onTx }: BlockDetailProps) {
  const [block,   setBlock]   = useState<BlockDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error,   setError]   = useState<string | null>(null);

  useEffect(() => {
    setLoading(true); setError(null);
    fetchBlockDetail(nodeUrl, height)
      .then(setBlock)
      .catch(e => setError(String(e)))
      .finally(() => setLoading(false));
  }, [nodeUrl, height]);

  const h         = block?.height ?? block?.index ?? height;
  const ts        = block?.timestamp;
  const feeTotal  = block?.total_fees !== undefined ? block.total_fees : undefined;
  const txList    = block?.txs ?? [];
  const txids     = block?.txids ?? [];

  return (
    <div>
      {/* Back */}
      <button onClick={onBack} style={{
        display: "flex", alignItems: "center", gap: 6,
        background: "none", border: "none", cursor: "pointer",
        color: colors.muted, fontFamily: fonts.sans, fontSize: 13,
        marginBottom: 16, padding: 0, transition: "color .15s",
      }}
        onMouseEnter={e => (e.currentTarget.style.color = colors.text)}
        onMouseLeave={e => (e.currentTarget.style.color = colors.muted)}
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
          stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
          <polyline points="15 18 9 12 15 6" />
        </svg>
        Back to Blocks
      </button>

      {/* Hero */}
      <div style={{
        background: colors.surface, border: `1px solid ${colors.border}`,
        borderRadius: 16, padding: "24px 28px", marginBottom: 20,
        position: "relative", overflow: "hidden",
      }}>
        <div style={{
          position: "absolute", top: 0, left: 0, right: 0, height: 3,
          background: `linear-gradient(90deg, transparent, ${colors.accent}88, transparent)`,
        }} />

        <div style={{ display: "flex", alignItems: "flex-start", gap: 20, flexWrap: "wrap" }}>
          <div style={{
            width: 56, height: 56, borderRadius: 14, flexShrink: 0,
            background: `${colors.accent}18`, border: `1.5px solid ${colors.accent}44`,
            display: "flex", alignItems: "center", justifyContent: "center",
            fontFamily: fonts.mono, fontWeight: 800, fontSize: 18, color: colors.accent,
          }}>
            #{h.toString().slice(-4)}
          </div>

          <div style={{ flex: 1, minWidth: 200 }}>
            <div style={{ display: "flex", alignItems: "center", gap: 12, flexWrap: "wrap", marginBottom: 8 }}>
              <span style={{ fontFamily: fonts.mono, fontWeight: 800, fontSize: 28, color: colors.text }}>
                Block #{h.toLocaleString()}
              </span>
              {!loading && <ConfBadge conf={block?.confirmations} />}
            </div>
            {block?.hash && (
              <div style={{ fontFamily: fonts.mono, fontSize: 12, color: colors.muted, wordBreak: "break-all" }}>
                {block.hash}
              </div>
            )}
          </div>

          {/* Stats chips */}
          <div style={{ display: "flex", gap: 10, flexWrap: "wrap" }}>
            {block?.tx_count !== undefined && (
              <div style={chipStyle(colors.blue)}>
                {block.tx_count} txs
              </div>
            )}
            {ts && (
              <div style={chipStyle(colors.green)}>
                {timeAgo(ts)}
              </div>
            )}
            {feeTotal !== undefined && (
              <div style={chipStyle(colors.purple)}>
                fee {(feeTotal / PACKETS_PER_PKT).toFixed(4)} PKT
              </div>
            )}
          </div>
        </div>
      </div>

      {loading && (
        <div style={{ padding: 40, textAlign: "center", color: colors.muted }}>Loading block…</div>
      )}

      {error && (
        <div style={{
          padding: "12px 18px", borderRadius: 10, marginBottom: 16,
          background: "rgba(240,96,96,.08)", border: `1px solid rgba(240,96,96,.2)`,
          color: colors.red, fontSize: 13,
        }}>⚠ {error}</div>
      )}

      {!loading && block && (
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1.6fr", gap: 16, alignItems: "start" }}>

          {/* Metadata panel */}
          <Panel icon="ℹ" title="Block Info">
            <div style={{ padding: "0 20px" }}>
              <MetaRow label="Height"     value={`#${h.toLocaleString()}`} accent />
              <MetaRow label="Hash"       value={block.hash}       mono />
              <MetaRow label="Prev Hash"  value={block.prev_hash}  mono />
              <MetaRow label="Timestamp"  value={ts ? new Date(ts * 1000).toLocaleString() : undefined} />
              <MetaRow label="Txs"        value={block.tx_count}   />
              <MetaRow label="Size"       value={block.size ? `${block.size.toLocaleString()} bytes` : undefined} />
              <MetaRow label="Difficulty" value={block.difficulty !== undefined ? Number(block.difficulty).toExponential(3) : undefined} />
              <MetaRow label="Miner"      value={block.miner}      mono />
              <MetaRow label="Total Fees" value={feeTotal !== undefined ? `${(feeTotal / PACKETS_PER_PKT).toFixed(6)} PKT` : undefined} accent />
            </div>
          </Panel>

          {/* TX list panel */}
          <Panel icon="📋" title={`Transactions (${txList.length || txids.length})`}>
            {txList.length > 0 && txList.map((tx, i) => (
              <TxRow key={i} tx={tx} onTx={onTx} />
            ))}
            {txList.length === 0 && txids.length > 0 && txids.map((txid, i) => (
              <div key={i}
                onClick={() => onTx(txid)}
                style={{
                  padding: "11px 18px", cursor: "pointer",
                  borderBottom: `1px solid ${colors.border}`,
                  fontFamily: fonts.mono, fontSize: 12, color: colors.blue,
                  transition: "background .15s",
                  display: "flex", alignItems: "center", justifyContent: "space-between",
                }}
                onMouseEnter={e => (e.currentTarget.style.background = colors.surface2)}
                onMouseLeave={e => (e.currentTarget.style.background = "transparent")}
              >
                <span>{shortHash(txid)}</span>
                <svg width="13" height="13" viewBox="0 0 24 24" fill="none"
                  stroke={colors.muted} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <polyline points="9 18 15 12 9 6" />
                </svg>
              </div>
            ))}
            {txList.length === 0 && txids.length === 0 && (
              <div style={{ padding: 24, textAlign: "center", color: colors.muted, fontSize: 13 }}>
                No transaction data available
              </div>
            )}
          </Panel>
        </div>
      )}
    </div>
  );
}

function chipStyle(color: string): React.CSSProperties {
  return {
    fontSize: 12, fontWeight: 700, padding: "4px 12px", borderRadius: 6,
    background: color + "18", color, border: `1px solid ${color}33`,
  };
}
