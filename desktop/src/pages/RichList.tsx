// RichList.tsx — v20.7: Rich List & Mempool UI
// Top holders leaderboard · Mempool fee histogram · Pending TX table
import { useState, useEffect, useCallback, useRef } from "react";
import { colors, fonts } from "../theme";
import { Panel } from "../components/Panel";
import {
  fetchRichList, fetchMempool,
  fmtNum, shortHash, timeAgo, PACKETS_PER_PKT,
  type RichHolder, type MempoolTx,
} from "../api";

interface RichListProps {
  nodeUrl: string;
  onAddr:  (addr: string) => void;
}

// ── Avatar placeholder ────────────────────────────────────────────────────────

function Avatar({ address, rank }: { address: string; rank: number }) {
  // Deterministic color from address
  const hue = address
    ? address.split("").reduce((acc, c) => acc + c.charCodeAt(0), 0) % 360
    : 0;
  const initials = rank <= 3
    ? ["🥇", "🥈", "🥉"][rank - 1]
    : `#${rank}`;
  const isEmoji = rank <= 3;

  return (
    <div style={{
      width: 40, height: 40, borderRadius: 12, flexShrink: 0,
      background: isEmoji ? "transparent" : `hsl(${hue},55%,28%)`,
      border: isEmoji ? "none" : `1.5px solid hsl(${hue},55%,42%)`,
      display: "flex", alignItems: "center", justifyContent: "center",
      fontSize: isEmoji ? 22 : 13,
      fontFamily: fonts.mono, fontWeight: 700,
      color: isEmoji ? undefined : `hsl(${hue},80%,75%)`,
    }}>
      {initials}
    </div>
  );
}

// ── Balance bar ───────────────────────────────────────────────────────────────

function BalanceBar({ pct, max }: { pct: number; max: number }) {
  const width = max > 0 ? Math.max(2, (pct / max) * 100) : 2;
  return (
    <div style={{
      height: 4, background: colors.surface2,
      borderRadius: 2, overflow: "hidden", flex: 1,
    }}>
      <div style={{
        height: "100%", width: `${width}%`,
        background: `linear-gradient(90deg, ${colors.accent}, ${colors.blue})`,
        borderRadius: 2, transition: "width .4s ease",
      }} />
    </div>
  );
}

// ── Leaderboard ───────────────────────────────────────────────────────────────

function Leaderboard({
  holders, loading, onAddr,
}: { holders: RichHolder[]; loading: boolean; onAddr: (a: string) => void }) {
  const maxPct = holders.length > 0 ? (holders[0].pct ?? 0) : 1;

  return (
    <div>
      {/* Header */}
      <div style={{
        display: "grid", gridTemplateColumns: "56px 1fr 120px 80px 140px",
        padding: "8px 18px",
        borderBottom: `1px solid ${colors.border}`,
        fontSize: 10, fontWeight: 700, textTransform: "uppercase",
        letterSpacing: ".07em", color: colors.muted,
      }}>
        <span>Rank</span>
        <span>Address</span>
        <span style={{ textAlign: "right" }}>Balance</span>
        <span style={{ textAlign: "right" }}>%</span>
        <span style={{ paddingLeft: 12 }}>Share</span>
      </div>

      {loading && (
        <div style={{ padding: 32, textAlign: "center", color: colors.muted, fontSize: 13 }}>
          Loading rich list…
        </div>
      )}
      {!loading && holders.length === 0 && (
        <div style={{ padding: 32, textAlign: "center", color: colors.muted, fontSize: 13 }}>
          No data available
        </div>
      )}

      {holders.map((h, i) => {
        const rank    = h.rank ?? i + 1;
        const addr    = h.address ?? "unknown";
        const bal     = h.balance ?? 0;
        const pkt     = fmtNum(bal / PACKETS_PER_PKT);
        const pct     = h.pct ?? 0;

        return (
          <div key={i}
            style={{
              display: "grid", gridTemplateColumns: "56px 1fr 120px 80px 140px",
              alignItems: "center", gap: 0,
              padding: "10px 18px",
              borderBottom: `1px solid ${colors.border}`,
              transition: "background .15s", cursor: "pointer",
            }}
            onMouseEnter={e => (e.currentTarget.style.background = colors.surface2)}
            onMouseLeave={e => (e.currentTarget.style.background = "transparent")}
            onClick={() => addr !== "unknown" && onAddr(addr)}
          >
            <Avatar address={addr} rank={rank} />

            <div style={{ minWidth: 0, paddingLeft: 12 }}>
              <div style={{
                fontFamily: fonts.mono, fontSize: 12, color: colors.blue,
                overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
              }}>
                {addr}
              </div>
            </div>

            <div style={{
              textAlign: "right", fontFamily: fonts.mono, fontWeight: 700,
              fontSize: 13, color: colors.accent,
            }}>
              {pkt}
            </div>

            <div style={{
              textAlign: "right", fontFamily: fonts.mono, fontSize: 12,
              color: colors.muted,
            }}>
              {pct > 0 ? pct.toFixed(2) + "%" : "—"}
            </div>

            <div style={{ paddingLeft: 12, display: "flex", alignItems: "center" }}>
              <BalanceBar pct={pct} max={maxPct} />
            </div>
          </div>
        );
      })}
    </div>
  );
}

// ── Mempool fee histogram (pure canvas) ──────────────────────────────────────

function FeeHistogram({ txs }: { txs: MempoolTx[] }) {
  const ref = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = ref.current;
    if (!canvas || txs.length === 0) return;
    const dpr = window.devicePixelRatio || 1;
    const W   = canvas.offsetWidth;
    const H   = canvas.offsetHeight;
    canvas.width  = W * dpr;
    canvas.height = H * dpr;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.scale(dpr, dpr);

    // Bucket fee rates: 0-1, 1-5, 5-10, 10-50, 50-100, 100+
    const BUCKETS = [
      { label: "<1",  min: 0,   max: 1   },
      { label: "1-5", min: 1,   max: 5   },
      { label: "5-10",min: 5,   max: 10  },
      { label: "10-50",min: 10, max: 50  },
      { label: "50-100",min: 50,max: 100 },
      { label: "100+", min: 100,max: Infinity },
    ];

    const counts = BUCKETS.map(b =>
      txs.filter(t => {
        const r = t.fee_rate ?? (t.fee && t.size ? t.fee / t.size : 0);
        return r >= b.min && r < b.max;
      }).length
    );
    const maxCount = Math.max(...counts, 1);

    const pad  = { t: 16, r: 16, b: 32, l: 36 };
    const gW   = W - pad.l - pad.r;
    const gH   = H - pad.t - pad.b;
    const barW = (gW / BUCKETS.length) * 0.72;
    const gap  = (gW / BUCKETS.length) * 0.28;

    ctx.clearRect(0, 0, W, H);

    // Grid lines
    ctx.strokeStyle = colors.border;
    ctx.lineWidth   = 0.5;
    for (let i = 0; i <= 4; i++) {
      const y = pad.t + gH - (i / 4) * gH;
      ctx.beginPath(); ctx.moveTo(pad.l, y); ctx.lineTo(pad.l + gW, y); ctx.stroke();
      ctx.fillStyle = colors.muted;
      ctx.font = `10px monospace`;
      ctx.textAlign = "right";
      ctx.fillText(String(Math.round((i / 4) * maxCount)), pad.l - 4, y + 4);
    }

    // Bars
    BUCKETS.forEach((b, i) => {
      const x   = pad.l + i * (gW / BUCKETS.length) + gap / 2;
      const bH  = (counts[i] / maxCount) * gH;
      const y   = pad.t + gH - bH;

      // Gradient fill
      const grad = ctx.createLinearGradient(x, y, x, y + bH);
      grad.addColorStop(0, colors.accent + "ee");
      grad.addColorStop(1, colors.blue   + "88");
      ctx.fillStyle = grad;
      ctx.beginPath();
      ctx.roundRect(x, y, barW, bH, [4, 4, 0, 0]);
      ctx.fill();

      // Label
      ctx.fillStyle   = colors.muted;
      ctx.font        = "9px monospace";
      ctx.textAlign   = "center";
      ctx.fillText(b.label, x + barW / 2, H - pad.b + 14);

      // Count above bar
      if (counts[i] > 0) {
        ctx.fillStyle = colors.text;
        ctx.font      = "10px monospace";
        ctx.fillText(String(counts[i]), x + barW / 2, y - 4);
      }
    });
  }, [txs, colors.border, colors.muted, colors.text, colors.accent, colors.blue]);

  if (txs.length === 0) {
    return (
      <div style={{ height: 140, display: "flex", alignItems: "center",
        justifyContent: "center", color: colors.muted, fontSize: 13 }}>
        No mempool data
      </div>
    );
  }

  return (
    <div style={{ padding: "12px 18px" }}>
      <div style={{ fontSize: 11, color: colors.muted, marginBottom: 8, fontFamily: fonts.mono }}>
        Fee rate distribution (sat/byte)
      </div>
      <canvas ref={ref} style={{ width: "100%", height: 140, display: "block" }} />
    </div>
  );
}

// ── Mempool TX table ──────────────────────────────────────────────────────────

function MempoolTable({
  txs, loading, onTx,
}: { txs: MempoolTx[]; loading: boolean; onTx: (txid: string) => void }) {
  return (
    <div style={{ overflowX: "auto" }}>
      <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 13 }}>
        <thead>
          <tr style={{ borderBottom: `1px solid ${colors.border}` }}>
            {["TXID", "Fee (PKT)", "Rate (sat/B)", "Size", "In/Out", "Age"].map(h => (
              <th key={h} style={{
                padding: "10px 16px", textAlign: "left",
                fontSize: 10, fontWeight: 700, textTransform: "uppercase",
                letterSpacing: ".07em", color: colors.muted,
              }}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {loading && (
            <tr><td colSpan={6} style={{ padding: 24, textAlign: "center", color: colors.muted }}>Loading…</td></tr>
          )}
          {!loading && txs.length === 0 && (
            <tr><td colSpan={6} style={{ padding: 24, textAlign: "center", color: colors.muted }}>Mempool empty</td></tr>
          )}
          {txs.map((tx, i) => {
            const txid    = tx.txid ?? tx.hash ?? "—";
            const fee     = tx.fee ?? 0;
            const rate    = tx.fee_rate ?? (tx.fee && tx.size ? tx.fee / tx.size : 0);
            const rColor  = rate < 1 ? colors.green : rate < 10 ? colors.accent : colors.red;

            return (
              <tr key={i}
                onClick={() => txid !== "—" && onTx(txid)}
                style={{
                  borderBottom: `1px solid ${colors.border}`,
                  cursor: txid !== "—" ? "pointer" : "default",
                  transition: "background .15s",
                }}
                onMouseEnter={e => (e.currentTarget.style.background = colors.surface2)}
                onMouseLeave={e => (e.currentTarget.style.background = "transparent")}
              >
                <td style={{ padding: "11px 16px", fontFamily: fonts.mono, fontSize: 12, color: colors.blue }}>
                  {txid !== "—" ? shortHash(txid) : "—"}
                </td>
                <td style={{ padding: "11px 16px", fontFamily: fonts.mono, color: colors.purple }}>
                  {fee > 0 ? (fee / PACKETS_PER_PKT).toFixed(6) : "—"}
                </td>
                <td style={{ padding: "11px 16px" }}>
                  <span style={{
                    fontFamily: fonts.mono, fontSize: 12, fontWeight: 700,
                    color: rColor,
                    padding: "2px 7px", borderRadius: 4,
                    background: rColor + "18", border: `1px solid ${rColor}33`,
                  }}>
                    {rate > 0 ? rate.toFixed(1) : "—"}
                  </span>
                </td>
                <td style={{ padding: "11px 16px", fontFamily: fonts.mono, color: colors.muted }}>
                  {tx.size ? `${tx.size}B` : "—"}
                </td>
                <td style={{ padding: "11px 16px", fontFamily: fonts.mono, color: colors.muted }}>
                  {tx.inputs ?? "?"}↓ {tx.outputs ?? "?"}↑
                </td>
                <td style={{ padding: "11px 16px", color: colors.muted }}>
                  {tx.timestamp ? timeAgo(tx.timestamp) : "—"}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

// ── Summary chips ─────────────────────────────────────────────────────────────

function SummaryBar({
  holderCount, totalSupply, mempoolCount, totalFee,
}: {
  holderCount: number; totalSupply: number;
  mempoolCount: number; totalFee: number;
}) {
  return (
    <div style={{
      display: "grid", gridTemplateColumns: "repeat(4,1fr)", gap: 12, marginBottom: 20,
    }}>
      {[
        { label: "Top Holders",    value: fmtNum(holderCount),                       color: colors.accent },
        { label: "Total Supply",   value: fmtNum(totalSupply / PACKETS_PER_PKT) + " PKT", color: colors.blue },
        { label: "Mempool Count",  value: fmtNum(mempoolCount),                     color: colors.green },
        { label: "Mempool Fees",   value: (totalFee / PACKETS_PER_PKT).toFixed(4) + " PKT",     color: colors.purple },
      ].map(({ label, value, color }) => (
        <div key={label} style={{
          background: colors.surface, border: `1px solid ${colors.border}`,
          borderRadius: 12, padding: "16px 20px",
          position: "relative", overflow: "hidden",
        }}>
          <div style={{
            position: "absolute", top: 0, left: 0, right: 0, height: 2,
            background: `linear-gradient(90deg,transparent,${color}66,transparent)`,
          }} />
          <div style={{ fontSize: 10, color: colors.muted, fontWeight: 700,
            textTransform: "uppercase", letterSpacing: ".07em", marginBottom: 6 }}>
            {label}
          </div>
          <div style={{ fontFamily: fonts.mono, fontWeight: 700, fontSize: 20, color }}>
            {value}
          </div>
        </div>
      ))}
    </div>
  );
}

// ── Main ──────────────────────────────────────────────────────────────────────

export function RichList({ nodeUrl, onAddr }: RichListProps) {
  const [holders,     setHolders]     = useState<RichHolder[]>([]);
  const [totalSupply, setTotalSupply] = useState(0);
  const [mempoolTxs,  setMempoolTxs] = useState<MempoolTx[]>([]);
  const [mempoolCount,setMempoolCount]= useState(0);
  const [totalFee,    setTotalFee]    = useState(0);
  const [loadRich,    setLoadRich]    = useState(true);
  const [loadMp,      setLoadMp]      = useState(true);
  const [activeTab,   setActiveTab]   = useState<"richlist" | "mempool">("richlist");

  const loadRichList = useCallback(() => {
    setLoadRich(true);
    fetchRichList(nodeUrl, 100)
      .then(d => {
        setHolders(d.holders ?? []);
        setTotalSupply(d.total_supply ?? 0);
      })
      .catch(() => {})
      .finally(() => setLoadRich(false));
  }, [nodeUrl]);

  const loadMempool = useCallback(() => {
    setLoadMp(true);
    fetchMempool(nodeUrl, 50)
      .then(d => {
        setMempoolTxs(d.txs ?? []);
        setMempoolCount(d.count ?? d.txs?.length ?? 0);
        setTotalFee(d.total_fee ?? 0);
      })
      .catch(() => {})
      .finally(() => setLoadMp(false));
  }, [nodeUrl]);

  useEffect(() => { loadRichList(); loadMempool(); }, [loadRichList, loadMempool]);

  return (
    <div>
      <style>{`
        @keyframes fadeIn { from{opacity:0;transform:translateY(6px)} to{opacity:1;transform:translateY(0)} }
      `}</style>

      {/* Summary */}
      <SummaryBar
        holderCount={holders.length}
        totalSupply={totalSupply}
        mempoolCount={mempoolCount}
        totalFee={totalFee}
      />

      {/* Tab switcher */}
      <div style={{
        display: "flex", gap: 2,
        background: colors.surface2, border: `1px solid ${colors.border}`,
        borderRadius: 10, padding: 3, width: "fit-content", marginBottom: 16,
      }}>
        {([
          ["richlist", "Rich List"],
          ["mempool",  "Mempool"],
        ] as const).map(([id, label]) => (
          <button key={id} onClick={() => setActiveTab(id)} style={{
            padding: "7px 22px", border: "none", borderRadius: 8, cursor: "pointer",
            fontFamily: fonts.sans, fontWeight: 600, fontSize: 13,
            background: activeTab === id ? colors.surface : "transparent",
            color: activeTab === id ? colors.text : colors.muted,
            transition: "all .2s",
          }}>
            {label}
            {id === "richlist" && holders.length > 0 && (
              <span style={{
                marginLeft: 6, fontSize: 10, padding: "1px 6px",
                background: `${colors.accent}22`, color: colors.accent,
                border: `1px solid ${colors.accent}44`, borderRadius: 4,
              }}>{holders.length}</span>
            )}
            {id === "mempool" && mempoolCount > 0 && (
              <span style={{
                marginLeft: 6, fontSize: 10, padding: "1px 6px",
                background: `${colors.green}22`, color: colors.green,
                border: `1px solid ${colors.green}44`, borderRadius: 4,
              }}>{mempoolCount}</span>
            )}
          </button>
        ))}
      </div>

      {/* Rich List tab */}
      {activeTab === "richlist" && (
        <div style={{ animation: "fadeIn .3s ease" }}>
          <Panel icon="🏆" title="Top PKT Holders"
            right={
              <button onClick={loadRichList} style={{
                padding: "4px 12px", background: colors.surface2,
                border: `1px solid ${colors.border}`, borderRadius: 6,
                color: colors.muted, cursor: "pointer", fontSize: 12,
              }}>↻</button>
            }
          >
            <Leaderboard holders={holders} loading={loadRich} onAddr={onAddr} />
          </Panel>
        </div>
      )}

      {/* Mempool tab */}
      {activeTab === "mempool" && (
        <div style={{ animation: "fadeIn .3s ease" }}>
          {/* Histogram */}
          <Panel icon="📊" title="Fee Rate Histogram"
            right={
              <button onClick={loadMempool} style={{
                padding: "4px 12px", background: colors.surface2,
                border: `1px solid ${colors.border}`, borderRadius: 6,
                color: colors.muted, cursor: "pointer", fontSize: 12,
              }}>↻</button>
            }
          >
            <FeeHistogram txs={mempoolTxs} />
          </Panel>

          {/* TX table */}
          <div style={{ marginTop: 16 }}>
            <Panel icon="⏳" title={`Pending Transactions (${mempoolCount})`}>
              <MempoolTable txs={mempoolTxs} loading={loadMp} onTx={() => {}} />
            </Panel>
          </div>
        </div>
      )}
    </div>
  );
}
