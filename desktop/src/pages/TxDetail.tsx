// TxDetail.tsx — v20.6: Transaction detail page
// Inputs/outputs flow, fee rate badge, confirmation count, size
import { useState, useEffect } from "react";
import { colors, fonts } from "../theme";
import { Panel } from "../components/Panel";
import { fetchTxDetail, shortHash, timeAgo, type TxDetail as TxDetailType, type TxInput, type TxOutput } from "../api";

interface TxDetailProps {
  nodeUrl: string;
  txid:    string;
  onBack:  () => void;
  onAddr:  (addr: string) => void;
}

// ── Fee rate badge ────────────────────────────────────────────────────────────

function FeeRateBadge({ fee, size }: { fee: number | undefined; size: number | undefined }) {
  if (fee === undefined || !size) return null;
  const rate = fee / size;
  const color = rate < 1 ? colors.green : rate < 10 ? colors.accent : colors.red;
  const label = rate < 1 ? "Low" : rate < 10 ? "Medium" : "High";
  return (
    <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
      <span style={{
        fontSize: 11, fontWeight: 700, padding: "3px 10px", borderRadius: 6,
        background: color + "22", color, border: `1px solid ${color}44`,
      }}>{label} fee</span>
      <span style={{ fontSize: 12, color: colors.muted, fontFamily: fonts.mono }}>
        {rate.toFixed(2)} sat/byte
      </span>
    </div>
  );
}

// ── Confirmation badge ────────────────────────────────────────────────────────

function ConfBadge({ conf }: { conf: number | undefined }) {
  if (conf === undefined) return null;
  const color = conf === 0 ? colors.red : conf < 6 ? colors.accent : colors.green;
  const label = conf === 0 ? "Unconfirmed" : conf >= 100 ? "100+ confirmations" : `${conf} confirmations`;
  return (
    <span style={{
      fontSize: 12, fontWeight: 700, padding: "3px 10px", borderRadius: 6,
      background: color + "22", color, border: `1px solid ${color}44`,
    }}>{label}</span>
  );
}

// ── Input/Output rows ─────────────────────────────────────────────────────────

function InputRow({ inp, onAddr }: { inp: TxInput; onAddr: (addr: string) => void }) {
  const isCoinbase = !inp.txid && !inp.address;
  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 12, padding: "11px 0",
      borderBottom: `1px solid ${colors.border}`,
    }}>
      {/* Coinbase vs regular */}
      {isCoinbase ? (
        <span style={{
          fontSize: 11, fontWeight: 700, padding: "2px 8px", borderRadius: 4,
          background: `${colors.accent}22`, color: colors.accent,
          border: `1px solid ${colors.accent}44`, flexShrink: 0,
        }}>COINBASE</span>
      ) : (
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
          stroke={colors.muted} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" style={{ flexShrink: 0 }}>
          <polyline points="15 18 9 12 15 6" />
        </svg>
      )}

      <div style={{ flex: 1, minWidth: 0 }}>
        {inp.txid && (
          <div style={{ fontFamily: fonts.mono, fontSize: 11, color: colors.blue,
            overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            {shortHash(inp.txid)}:{inp.vout ?? 0}
          </div>
        )}
        {inp.address && (
          <button onClick={() => onAddr(inp.address!)} style={{
            background: "none", border: "none", cursor: "pointer",
            fontFamily: fonts.mono, fontSize: 12, color: colors.green, padding: 0,
            overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
            maxWidth: "100%", textAlign: "left",
          }}>
            {inp.address}
          </button>
        )}
        {isCoinbase && (
          <span style={{ fontSize: 12, color: colors.muted }}>Block reward</span>
        )}
      </div>

      {inp.amount !== undefined && (
        <span style={{ fontFamily: fonts.mono, fontWeight: 700, fontSize: 13,
          color: colors.text, flexShrink: 0 }}>
          {(inp.amount / 1e9).toFixed(4)} PKT
        </span>
      )}
    </div>
  );
}

function OutputRow({ out, onAddr }: { out: TxOutput; onAddr: (addr: string) => void }) {
  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 12, padding: "11px 0",
      borderBottom: `1px solid ${colors.border}`,
    }}>
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
        stroke={colors.green} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" style={{ flexShrink: 0 }}>
        <polyline points="9 18 15 12 9 6" />
      </svg>

      <div style={{ flex: 1, minWidth: 0 }}>
        {out.address && (
          <button onClick={() => onAddr(out.address!)} style={{
            background: "none", border: "none", cursor: "pointer",
            fontFamily: fonts.mono, fontSize: 12, color: colors.green, padding: 0,
            overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
            maxWidth: "100%", textAlign: "left",
          }}>
            {out.address}
          </button>
        )}
        {out.type && (
          <span style={{ fontSize: 10, color: colors.muted, fontFamily: fonts.mono, marginLeft: 4 }}>
            {out.type}
          </span>
        )}
      </div>

      {out.amount !== undefined && (
        <span style={{ fontFamily: fonts.mono, fontWeight: 700, fontSize: 13,
          color: colors.green, flexShrink: 0 }}>
          {(out.amount / 1e9).toFixed(4)} PKT
        </span>
      )}
    </div>
  );
}

// ── Flow diagram (summary bar) ────────────────────────────────────────────────

function FlowBar({ tx }: { tx: TxDetailType }) {
  const inTotal  = (tx.inputs  ?? []).reduce((s, i) => s + (i.amount ?? 0), 0);
  const outTotal = (tx.outputs ?? []).reduce((s, o) => s + (o.amount ?? 0), 0);
  const fee      = tx.fee ?? (inTotal > 0 ? inTotal - outTotal : undefined);

  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 0,
      background: colors.surface2, border: `1px solid ${colors.border}`,
      borderRadius: 12, overflow: "hidden", marginBottom: 20,
    }}>
      <div style={{
        flex: 1, padding: "14px 20px",
        borderRight: `1px solid ${colors.border}`,
      }}>
        <div style={{ fontSize: 10, color: colors.muted, textTransform: "uppercase", letterSpacing: ".07em", marginBottom: 4 }}>
          Total Input
        </div>
        <div style={{ fontFamily: fonts.mono, fontWeight: 700, fontSize: 18, color: colors.text }}>
          {inTotal > 0 ? (inTotal / 1e9).toFixed(4) : "—"}
          <span style={{ fontSize: 12, color: colors.muted, marginLeft: 4 }}>PKT</span>
        </div>
      </div>

      {/* Arrow */}
      <div style={{ padding: "0 16px", color: colors.muted }}>
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
          stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <line x1="5" y1="12" x2="19" y2="12" />
          <polyline points="12 5 19 12 12 19" />
        </svg>
      </div>

      <div style={{ flex: 1, padding: "14px 20px", borderRight: `1px solid ${colors.border}` }}>
        <div style={{ fontSize: 10, color: colors.muted, textTransform: "uppercase", letterSpacing: ".07em", marginBottom: 4 }}>
          Total Output
        </div>
        <div style={{ fontFamily: fonts.mono, fontWeight: 700, fontSize: 18, color: colors.green }}>
          {(outTotal / 1e9).toFixed(4)}
          <span style={{ fontSize: 12, color: colors.muted, marginLeft: 4 }}>PKT</span>
        </div>
      </div>

      {fee !== undefined && (
        <div style={{ padding: "14px 20px" }}>
          <div style={{ fontSize: 10, color: colors.muted, textTransform: "uppercase", letterSpacing: ".07em", marginBottom: 4 }}>
            Fee
          </div>
          <div style={{ fontFamily: fonts.mono, fontWeight: 700, fontSize: 18, color: colors.purple }}>
            {(fee / 1e9).toFixed(6)}
            <span style={{ fontSize: 12, color: colors.muted, marginLeft: 4 }}>PKT</span>
          </div>
        </div>
      )}
    </div>
  );
}

// ── Main ─────────────────────────────────────────────────────────────────────

export function TxDetail({ nodeUrl, txid, onBack, onAddr }: TxDetailProps) {
  const [tx,      setTx]      = useState<TxDetailType | null>(null);
  const [loading, setLoading] = useState(true);
  const [error,   setError]   = useState<string | null>(null);

  useEffect(() => {
    setLoading(true); setError(null);
    fetchTxDetail(nodeUrl, txid)
      .then(setTx)
      .catch(e => setError(String(e)))
      .finally(() => setLoading(false));
  }, [nodeUrl, txid]);

  const inputs  = tx?.inputs  ?? [];
  const outputs = tx?.outputs ?? [];

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
        Back
      </button>

      {/* Hero */}
      <div style={{
        background: colors.surface, border: `1px solid ${colors.border}`,
        borderRadius: 16, padding: "24px 28px", marginBottom: 20,
        position: "relative", overflow: "hidden",
      }}>
        <div style={{
          position: "absolute", top: 0, left: 0, right: 0, height: 3,
          background: `linear-gradient(90deg, transparent, ${colors.blue}88, transparent)`,
        }} />

        <div style={{ marginBottom: 10, display: "flex", alignItems: "center", gap: 12, flexWrap: "wrap" }}>
          <span style={{ fontSize: 11, color: colors.muted, fontWeight: 700,
            textTransform: "uppercase", letterSpacing: ".07em" }}>Transaction</span>
          {!loading && <ConfBadge conf={tx?.confirmations} />}
          {!loading && <FeeRateBadge fee={tx?.fee} size={tx?.size} />}
        </div>

        <div style={{
          fontFamily: fonts.mono, fontSize: 13, color: colors.blue,
          wordBreak: "break-all", marginBottom: 12,
        }}>
          {txid}
        </div>

        <div style={{ display: "flex", gap: 20, flexWrap: "wrap" }}>
          {tx?.height !== undefined && (
            <span style={{ fontSize: 13, color: colors.muted }}>
              Block <span style={{ color: colors.accent, fontFamily: fonts.mono }}>#{tx.height.toLocaleString()}</span>
            </span>
          )}
          {tx?.timestamp && (
            <span style={{ fontSize: 13, color: colors.muted }}>
              {timeAgo(tx.timestamp)} · {new Date(tx.timestamp * 1000).toLocaleString()}
            </span>
          )}
          {tx?.size !== undefined && (
            <span style={{ fontSize: 13, color: colors.muted }}>
              {tx.size} bytes
            </span>
          )}
        </div>
      </div>

      {loading && (
        <div style={{ padding: 40, textAlign: "center", color: colors.muted }}>Loading transaction…</div>
      )}

      {error && (
        <div style={{
          padding: "12px 18px", borderRadius: 10, marginBottom: 16,
          background: "rgba(240,96,96,.08)", border: `1px solid rgba(240,96,96,.2)`,
          color: colors.red, fontSize: 13,
        }}>⚠ {error}</div>
      )}

      {!loading && tx && (
        <>
          {/* Flow bar */}
          <FlowBar tx={tx} />

          {/* Inputs + Outputs side by side */}
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 16 }}>
            <Panel icon="↙" title={`Inputs (${inputs.length})`}>
              <div style={{ padding: "0 18px" }}>
                {inputs.length === 0
                  ? <div style={{ padding: "16px 0", color: colors.muted, fontSize: 13 }}>No inputs</div>
                  : inputs.map((inp, i) => <InputRow key={i} inp={inp} onAddr={onAddr} />)
                }
              </div>
            </Panel>

            <Panel icon="↗" title={`Outputs (${outputs.length})`}>
              <div style={{ padding: "0 18px" }}>
                {outputs.length === 0
                  ? <div style={{ padding: "16px 0", color: colors.muted, fontSize: 13 }}>No outputs</div>
                  : outputs.map((out, i) => <OutputRow key={i} out={out} onAddr={onAddr} />)
                }
              </div>
            </Panel>
          </div>
        </>
      )}
    </div>
  );
}
