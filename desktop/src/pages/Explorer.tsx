// Explorer.tsx — v23.x: Unified Explorer (Overview + Blocks + Charts)
import { useState, useEffect, useCallback } from "react";
import { colors, fonts } from "../theme";
import { Panel } from "../components/Panel";
import { MiniChart, type ChartPoint } from "../components/MiniChart";
import { useLiveDashboard } from "../hooks/useLiveDashboard";
import {
  fetchBlocks, fetchAnalytics, fetchMempool,
  fmtHashrate, fmtNum, fmtPkt, shortHash, timeAgo,
  type BlockHeader, type AnalyticsSeries, type MempoolTx,
} from "../api";

// ─────────────────────────────────────────────────────────────────────────────
// Props
// ─────────────────────────────────────────────────────────────────────────────

export type ExplorerSubTab = "overview" | "blocks" | "charts" | "transactions";

interface ExplorerProps {
  nodeUrl:    string;
  onBlock:    (height: number) => void;
  onTx?:      (txid: string) => void;
  subTab?:    ExplorerSubTab;
  onSubTab?:  (t: ExplorerSubTab) => void;
}

// ─────────────────────────────────────────────────────────────────────────────
// Shared helpers
// ─────────────────────────────────────────────────────────────────────────────

function ConnBadge({ connected }: { connected: boolean }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
      <div style={{
        width: 7, height: 7, borderRadius: "50%",
        background: connected ? colors.green : colors.red,
        boxShadow: connected ? `0 0 6px ${colors.green}` : "none",
        animation: connected ? "pulse 2s infinite" : "none",
      }} />
      <span style={{ fontSize: 12, color: connected ? colors.green : colors.red, fontWeight: 600 }}>
        {connected ? "Live" : "Offline"}
      </span>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Sub-tab bar
// ─────────────────────────────────────────────────────────────────────────────

const SUB_TABS: { id: ExplorerSubTab; label: string; icon: string }[] = [
  { id: "overview",      label: "Overview",     icon: "📊" },
  { id: "blocks",        label: "Blocks",       icon: "🧱" },
  { id: "charts",        label: "Charts",       icon: "📈" },
  { id: "transactions",  label: "Transactions", icon: "↔" },
];

function SubTabBar({
  active, onChange,
}: { active: ExplorerSubTab; onChange: (t: ExplorerSubTab) => void }) {
  return (
    <div style={{
      display: "flex", gap: 4, marginBottom: 18,
      background: colors.surface, border: `1px solid ${colors.border}`,
      borderRadius: 10, padding: 4, width: "fit-content",
    }}>
      {SUB_TABS.map(t => (
        <button key={t.id} onClick={() => onChange(t.id)} style={{
          padding: "7px 20px", borderRadius: 7, border: "none", cursor: "pointer",
          fontWeight: 600, fontSize: 13,
          display: "flex", alignItems: "center", gap: 6,
          background: active === t.id ? colors.accent : "transparent",
          color:      active === t.id ? "#000" : colors.muted,
          transition: "all .18s",
        }}>
          <span style={{ fontSize: 14 }}>{t.icon}</span>
          {t.label}
        </button>
      ))}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// OVERVIEW
// ─────────────────────────────────────────────────────────────────────────────


function StatCard({ label, value, sub, icon, color, pulse }: {
  label: string; value: string; sub?: string; icon: JSX.Element; color: string; pulse?: boolean;
}) {
  return (
    <div style={{
      background: colors.surface2, border: `1px solid ${colors.border}`,
      borderRadius: 12, padding: "18px 20px",
      display: "flex", flexDirection: "column", gap: 12,
    }}>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
        <span style={{ fontSize: 13, color: colors.muted, fontWeight: 500 }}>{label}</span>
        <div style={{
          width: 36, height: 36, borderRadius: 10,
          background: color + "22", border: `1px solid ${color}33`,
          display: "flex", alignItems: "center", justifyContent: "center", color, flexShrink: 0,
        }}>{icon}</div>
      </div>
      <div>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{ fontSize: 26, fontWeight: 700, color: colors.text, fontFamily: fonts.mono, letterSpacing: "-.02em" }}>
            {value}
          </span>
          {pulse && <span style={{ width: 7, height: 7, borderRadius: "50%", background: colors.green, boxShadow: `0 0 6px ${colors.green}`, animation: "pulse 2s infinite", display: "inline-block" }} />}
        </div>
        {sub && <div style={{ fontSize: 12, color: colors.green, marginTop: 4, fontWeight: 600 }}>{sub}</div>}
      </div>
    </div>
  );
}

function DashBlockRow({ b, i, total, onBlock }: { b: BlockHeader; i: number; total: number; onBlock: (h: number) => void }) {
  const h = b.index ?? b.height ?? 0;
  return (
    <div onClick={() => onBlock(h)}
      style={{
        display: "flex", alignItems: "center", gap: 12, padding: "11px 16px",
        borderBottom: i < total - 1 ? `1px solid ${colors.border}` : "none",
        cursor: "pointer", transition: "background .12s",
        animation: i === 0 ? "slideIn .4s ease" : "none",
      }}
      onMouseEnter={e => (e.currentTarget.style.background = colors.surface3)}
      onMouseLeave={e => (e.currentTarget.style.background = "transparent")}
    >
      <div style={{
        width: 32, height: 32, borderRadius: 8, flexShrink: 0,
        background: colors.surface3, display: "flex", alignItems: "center", justifyContent: "center",
      }}>
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke={colors.accent} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <rect x="2" y="7" width="20" height="14" rx="2"/><path d="M16 7V5a2 2 0 0 0-2-2h-4a2 2 0 0 0-2 2v2"/>
        </svg>
      </div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontFamily: fonts.mono, fontWeight: 700, fontSize: 13, color: colors.accent }}>
          #{fmtNum(h)}
        </div>
        <div style={{ fontSize: 11, color: colors.muted, marginTop: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {b.hash ? shortHash(b.hash) : "—"}
        </div>
      </div>
      <div style={{ textAlign: "right", flexShrink: 0 }}>
        <div style={{ fontSize: 12, color: colors.text, fontWeight: 600 }}>{b.tx_count ?? 0} tx</div>
        <div style={{ fontSize: 11, color: colors.muted, marginTop: 1 }}>{b.timestamp ? timeAgo(b.timestamp) : "—"}</div>
      </div>
    </div>
  );
}

function OverviewPanel({ nodeUrl, onBlock }: { nodeUrl: string; onBlock: (h: number) => void }) {
  const { summary, blocks, connected, error, refresh } = useLiveDashboard(nodeUrl);
  const [hrSeries, setHrSeries] = useState<AnalyticsSeries | null>(null);

  const height    = summary.height ?? 0;
  const hashrate  = summary.hashrate ?? 0;
  const mempool   = summary.mempool_count ?? 0;
  const blockTime = (summary.avg_block_time_s ?? summary.block_time_avg) ?? 0;

  useEffect(() => {
    fetchAnalytics(nodeUrl, "hashrate", 50)
      .then(s => setHrSeries(s))
      .catch(() => {});
  }, [nodeUrl]);

  const hrPts: ChartPoint[] = hrSeries
    ? hrSeries.points.map(p => ({ x: p.height, y: p.value }))
    : [];

  const latestBlocks = blocks.slice(0, 5);

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
      {error && (
        <div style={{
          padding: "10px 16px", borderRadius: 8,
          background: "rgba(239,68,68,.08)", border: `1px solid rgba(239,68,68,.2)`,
          color: colors.red, fontSize: 13,
        }}>⚠ {error}</div>
      )}

      {/* 4 stat cards */}
      <div style={{ display: "grid", gridTemplateColumns: "repeat(4,1fr)", gap: 12 }}>
        <StatCard
          label="Latest Block" value={fmtNum(height)} pulse={connected} color={colors.accent}
          icon={<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><rect x="2" y="7" width="20" height="14" rx="2"/><path d="M16 7V5a2 2 0 0 0-2-2h-4a2 2 0 0 0-2 2v2"/></svg>}
        />
        <StatCard
          label="Network Hashrate" value={fmtHashrate(hashrate)} color={colors.blue}
          icon={<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><polyline points="22 12 18 12 15 21 9 3 6 12 2 12"/></svg>}
        />
        <StatCard
          label="Block Time" value={blockTime.toFixed(1) + "s"} color={colors.green}
          icon={<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>}
        />
        <StatCard
          label="Mempool" value={fmtNum(mempool)} sub={mempool > 0 ? `${mempool} pending` : "empty"} color={colors.purple}
          icon={<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/></svg>}
        />
      </div>

      {/* Row 2: Latest Blocks + Network Overview */}
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 16 }}>
        {/* Latest Blocks */}
        <div style={{
          background: colors.surface, border: `1px solid ${colors.border}`,
          borderRadius: 12, overflow: "hidden",
        }}>
          <div style={{
            display: "flex", alignItems: "center", justifyContent: "space-between",
            padding: "14px 16px", borderBottom: `1px solid ${colors.border}`,
          }}>
            <span style={{ fontWeight: 700, fontSize: 15, color: colors.text }}>Latest Blocks</span>
            <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
              <ConnBadge connected={connected} />
              <button onClick={refresh} style={{
                padding: "4px 10px", background: colors.surface2,
                border: `1px solid ${colors.border}`, borderRadius: 6,
                color: colors.muted, cursor: "pointer", fontSize: 12,
              }}>↻</button>
            </div>
          </div>
          {latestBlocks.length === 0 ? (
            <div style={{ padding: 24, textAlign: "center", color: colors.muted, fontSize: 13 }}>
              {connected ? "Loading…" : "Connecting…"}
            </div>
          ) : latestBlocks.map((b, i) => (
            <DashBlockRow key={b.hash ?? i} b={b} i={i} total={latestBlocks.length} onBlock={onBlock} />
          ))}
        </div>

        {/* Network Overview chart */}
        <div style={{
          background: colors.surface, border: `1px solid ${colors.border}`,
          borderRadius: 12, overflow: "hidden",
        }}>
          <div style={{
            display: "flex", alignItems: "center", justifyContent: "space-between",
            padding: "14px 16px", borderBottom: `1px solid ${colors.border}`,
          }}>
            <span style={{ fontWeight: 700, fontSize: 15, color: colors.text }}>Network Hashrate</span>
            <span style={{ fontSize: 12, color: colors.muted, background: colors.surface2, border: `1px solid ${colors.border}`, borderRadius: 6, padding: "3px 10px" }}>50 blocks</span>
          </div>
          <div style={{ padding: "12px 16px" }}>
            <div style={{ fontFamily: fonts.mono, fontSize: 22, fontWeight: 700, color: colors.text, marginBottom: 4 }}>
              {fmtHashrate(hashrate)}
            </div>
          </div>
          <div style={{ height: 120 }}>
            {hrPts.length >= 2
              ? <MiniChart points={hrPts} color={colors.blue} height={120} filled />
              : <div style={{ height: 120, display: "flex", alignItems: "center", justifyContent: "center", color: colors.muted, fontSize: 13 }}>Loading chart…</div>
            }
          </div>
          {hrPts.length > 1 && (
            <div style={{ display: "flex", justifyContent: "space-between", padding: "6px 16px 12px", fontSize: 11, color: colors.muted, fontFamily: fonts.mono }}>
              <span>#{fmtNum(hrPts[0]?.x ?? 0)}</span>
              <span>#{fmtNum(hrPts[hrPts.length - 1]?.x ?? 0)}</span>
            </div>
          )}
        </div>
      </div>

      {/* Row 3: Node Status */}
      <div style={{
        background: colors.surface, border: `1px solid ${colors.border}`,
        borderRadius: 12, padding: "14px 20px",
        display: "flex", gap: 32, alignItems: "center", flexWrap: "wrap",
      }}>
        <ConnBadge connected={connected} />
        {[
          ["Block Height", fmtNum(height)],
          ["Difficulty", summary.difficulty !== undefined ? (summary.difficulty as number).toFixed(2) : "—"],
          ["UTXOs", summary.utxo_count !== undefined ? fmtNum(summary.utxo_count as number) : "—"],
          ["Node", nodeUrl.replace(/https?:\/\//, "")],
        ].map(([label, val]) => (
          <div key={label}>
            <div style={{ fontSize: 11, color: colors.muted, fontWeight: 600, textTransform: "uppercase", letterSpacing: ".05em", marginBottom: 3 }}>{label}</div>
            <div style={{ fontFamily: fonts.mono, fontSize: 13, fontWeight: 700, color: colors.text }}>{val}</div>
          </div>
        ))}
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// BLOCKS
// ─────────────────────────────────────────────────────────────────────────────

function BlocksPanel({ nodeUrl, onBlock }: { nodeUrl: string; onBlock: (h: number) => void }) {
  const [blocks, setBlocks] = useState<BlockHeader[]>([]);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const d = await fetchBlocks(nodeUrl, 25);
      setBlocks(d.blocks ?? d.headers ?? []);
    } catch (_) {}
    setLoading(false);
  }, [nodeUrl]);

  useEffect(() => { load(); }, [load]);

  return (
    <Panel icon="🧱" title="All Blocks"
      right={
        <button onClick={load} style={{
          padding: "5px 14px", background: colors.surface2,
          border: `1px solid ${colors.border}`, borderRadius: 7,
          color: colors.muted, cursor: "pointer", fontSize: 12,
        }}>↻ Refresh</button>
      }
    >
      {loading && (
        <div style={{ padding: 24, textAlign: "center", color: colors.muted, fontSize: 13 }}>Loading…</div>
      )}
      {!loading && blocks.length === 0 && (
        <div style={{ padding: 24, textAlign: "center", color: colors.muted, fontSize: 13 }}>No blocks found</div>
      )}
      {blocks.length > 0 && (
        <div style={{ overflowX: "auto" }}>
          <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 13 }}>
            <thead>
              <tr style={{ borderBottom: `1px solid ${colors.border}` }}>
                {["Height", "Hash", "Txs", "Time"].map(h => (
                  <th key={h} style={{
                    padding: "10px 18px", textAlign: "left",
                    fontSize: 11, fontWeight: 700, textTransform: "uppercase",
                    letterSpacing: ".07em", color: colors.muted,
                  }}>{h}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {blocks.map((b, i) => {
                const height = b.index ?? b.height ?? 0;
                return (
                  <tr key={i}
                    onClick={() => onBlock(height)}
                    style={{ borderBottom: `1px solid ${colors.border}`, cursor: "pointer", transition: "background .15s" }}
                    onMouseEnter={e => (e.currentTarget.style.background = colors.surface2)}
                    onMouseLeave={e => (e.currentTarget.style.background = "transparent")}
                  >
                    <td style={{ padding: "12px 18px", fontFamily: fonts.mono, fontWeight: 700, color: colors.accent }}>
                      #{fmtNum(height)}
                    </td>
                    <td style={{ padding: "12px 18px", fontFamily: fonts.mono, fontSize: 12, color: colors.blue }}>
                      {b.hash ? shortHash(b.hash) : "—"}
                    </td>
                    <td style={{ padding: "12px 18px", color: colors.text }}>{b.tx_count ?? "—"}</td>
                    <td style={{ padding: "12px 18px", color: colors.muted }}>
                      {b.timestamp ? timeAgo(b.timestamp) : "—"}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </Panel>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// CHARTS
// ─────────────────────────────────────────────────────────────────────────────

type ChartWindow = 50 | 100 | 200 | 500;
const WINDOWS: ChartWindow[] = [50, 100, 200, 500];

function toPoints(series: AnalyticsSeries): ChartPoint[] {
  if (!series?.points) return [];
  return series.points.map(p => ({ x: p.height, y: p.value }));
}

function statOf(pts: ChartPoint[]) {
  if (!pts.length) return { min: 0, max: 0, avg: 0, last: 0 };
  const vals = pts.map(p => p.y);
  return {
    min:  Math.min(...vals),
    max:  Math.max(...vals),
    avg:  vals.reduce((a, b) => a + b, 0) / vals.length,
    last: vals[vals.length - 1],
  };
}

function ChartCard({ title, icon, series, color, fmt, loading, error }: {
  title: string; icon: string; series: AnalyticsSeries | null;
  color: string; fmt: (v: number) => string; loading: boolean; error: string | null;
}) {
  const pts   = series ? toPoints(series) : [];
  const stats = statOf(pts);
  return (
    <Panel icon={icon} title={title}>
      <div style={{ padding: "14px 18px" }}>
        <div style={{ marginBottom: 12 }}>
          <div style={{ fontFamily: fonts.mono, fontWeight: 700, fontSize: 24, color, letterSpacing: "-.02em" }}>
            {loading ? "…" : error ? "—" : fmt(stats.last)}
          </div>
          <div style={{ fontSize: 11, color: colors.muted, marginTop: 2 }}>Current · {series?.unit ?? ""}</div>
        </div>
        <div style={{ background: colors.surface2, border: `1px solid ${colors.border}`, borderRadius: 10, overflow: "hidden", marginBottom: 12 }}>
          {loading ? (
            <div style={{ height: 120, display: "flex", alignItems: "center", justifyContent: "center", color: colors.muted, fontSize: 13 }}>Loading…</div>
          ) : error ? (
            <div style={{ height: 120, display: "flex", alignItems: "center", justifyContent: "center", color: colors.red, fontSize: 13 }}>{error}</div>
          ) : pts.length < 2 ? (
            <div style={{ height: 120, display: "flex", alignItems: "center", justifyContent: "center", color: colors.muted, fontSize: 13 }}>Not enough data</div>
          ) : (
            <MiniChart points={pts} color={color} height={120} filled />
          )}
        </div>
        {!loading && !error && pts.length > 0 && (
          <div style={{ display: "grid", gridTemplateColumns: "repeat(3,1fr)", gap: 8 }}>
            {([["Min", fmt(stats.min)], ["Avg", fmt(stats.avg)], ["Max", fmt(stats.max)]] as const).map(([label, val]) => (
              <div key={label} style={{ background: colors.surface2, border: `1px solid ${colors.border}`, borderRadius: 8, padding: "8px 12px", textAlign: "center" }}>
                <div style={{ fontSize: 10, color: colors.muted, textTransform: "uppercase", letterSpacing: ".06em", marginBottom: 4 }}>{label}</div>
                <div style={{ fontFamily: fonts.mono, fontWeight: 700, fontSize: 13, color }}>{val}</div>
              </div>
            ))}
          </div>
        )}
      </div>
    </Panel>
  );
}

function ChartsPanel({ nodeUrl }: { nodeUrl: string }) {
  const [win, setWin]         = useState<ChartWindow>(100);
  const [hashrate, setHR]     = useState<AnalyticsSeries | null>(null);
  const [blockTime, setBT]    = useState<AnalyticsSeries | null>(null);
  const [difficulty, setDiff] = useState<AnalyticsSeries | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError]     = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true); setError(null);
    try {
      const [hr, bt, diff] = await Promise.all([
        fetchAnalytics(nodeUrl, "hashrate",   win),
        fetchAnalytics(nodeUrl, "block_time", win),
        fetchAnalytics(nodeUrl, "difficulty", win),
      ]);
      setHR(Array.isArray(hr?.points) ? hr : null);
      setBT(Array.isArray(bt?.points) ? bt : null);
      setDiff(Array.isArray(diff?.points) ? diff : null);
    } catch (e) { setError(String(e)); }
    finally     { setLoading(false); }
  }, [nodeUrl, win]);

  useEffect(() => { load(); }, [load]);

  const diffPts   = difficulty ? toPoints(difficulty) : [];
  const diffStats = statOf(diffPts);

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
      {/* Toolbar */}
      <div style={{
        display: "flex", alignItems: "center", gap: 12,
        background: colors.surface, border: `1px solid ${colors.border}`,
        borderRadius: 10, padding: "12px 18px",
      }}>
        <span style={{ fontSize: 13, color: colors.muted, fontWeight: 600 }}>Window:</span>
        <div style={{ display: "flex", gap: 6 }}>
          {WINDOWS.map(w => (
            <button key={w} onClick={() => setWin(w)} style={{
              padding: "5px 14px", borderRadius: 7, border: "none", cursor: "pointer",
              fontFamily: fonts.mono, fontSize: 13, fontWeight: 700,
              background: win === w ? colors.accent : colors.surface2,
              color: win === w ? "#000" : colors.muted, transition: "all .2s",
            }}>{w}</button>
          ))}
        </div>
        <span style={{ fontSize: 12, color: colors.muted, marginLeft: 8 }}>blocks</span>
        <div style={{ flex: 1 }} />
        <button onClick={load} style={{
          padding: "5px 14px", background: colors.surface2,
          border: `1px solid ${colors.border}`, borderRadius: 7,
          color: colors.muted, cursor: "pointer", fontSize: 12,
        }}>↻ Refresh</button>
      </div>

      {error && (
        <div style={{ padding: "10px 16px", borderRadius: 8, background: "rgba(240,96,96,.08)", border: `1px solid rgba(240,96,96,.2)`, color: colors.red, fontSize: 13 }}>
          ⚠ {error}
        </div>
      )}

      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 16 }}>
        <ChartCard icon="⚡" title="Hashrate"   series={hashrate}   color={colors.blue}  fmt={v => fmtHashrate(v)}     loading={loading} error={error} />
        <ChartCard icon="⏱" title="Block Time" series={blockTime}  color={colors.green} fmt={v => v.toFixed(1) + "s"} loading={loading} error={error} />
      </div>

      <Panel icon="⚙" title="Difficulty">
        <div style={{ padding: "14px 18px" }}>
          <div style={{ display: "flex", alignItems: "baseline", gap: 16, marginBottom: 12 }}>
            <div style={{ fontFamily: fonts.mono, fontWeight: 700, fontSize: 24, color: colors.purple }}>
              {loading ? "…" : error ? "—" : diffStats.last.toFixed(2)}
            </div>
            <div style={{ fontSize: 11, color: colors.muted }}>Current difficulty</div>
            {!loading && !error && diffStats.last > 0 && (
              <div style={{ marginLeft: "auto", display: "flex", gap: 16 }}>
                {([["Min", diffStats.min.toFixed(2)], ["Avg", diffStats.avg.toFixed(2)], ["Max", diffStats.max.toFixed(2)]] as const).map(([l, v]) => (
                  <div key={l} style={{ textAlign: "center" }}>
                    <div style={{ fontSize: 10, color: colors.muted, textTransform: "uppercase", letterSpacing: ".06em" }}>{l}</div>
                    <div style={{ fontFamily: fonts.mono, fontWeight: 700, fontSize: 14, color: colors.purple }}>{v}</div>
                  </div>
                ))}
              </div>
            )}
          </div>
          <div style={{ background: colors.surface2, border: `1px solid ${colors.border}`, borderRadius: 10, overflow: "hidden" }}>
            {loading ? (
              <div style={{ height: 100, display: "flex", alignItems: "center", justifyContent: "center", color: colors.muted, fontSize: 13 }}>Loading…</div>
            ) : diffPts.length >= 2 ? (
              <MiniChart points={diffPts} color={colors.purple} height={100} filled />
            ) : (
              <div style={{ height: 100, display: "flex", alignItems: "center", justifyContent: "center", color: colors.muted, fontSize: 13 }}>Not enough data</div>
            )}
          </div>
          {!loading && diffPts.length > 1 && (
            <div style={{ display: "flex", justifyContent: "space-between", marginTop: 6, paddingLeft: 4, paddingRight: 4 }}>
              <span style={{ fontFamily: fonts.mono, fontSize: 10, color: colors.muted }}>#{fmtNum(diffPts[0]?.x ?? 0)}</span>
              <span style={{ fontFamily: fonts.mono, fontSize: 10, color: colors.muted }}>#{fmtNum(diffPts[diffPts.length - 1]?.x ?? 0)}</span>
            </div>
          )}
        </div>
      </Panel>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// TRANSACTIONS (Mempool)
// ─────────────────────────────────────────────────────────────────────────────

function TransactionsPanel({ nodeUrl, onTx }: { nodeUrl: string; onTx?: (txid: string) => void }) {
  const [txs,     setTxs]     = useState<MempoolTx[]>([]);
  const [count,   setCount]   = useState(0);
  const [loading, setLoading] = useState(true);
  const [error,   setError]   = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true); setError(null);
    try {
      const d = await fetchMempool(nodeUrl, 50);
      setTxs(d.txs ?? []);
      setCount(d.count ?? d.txs?.length ?? 0);
    } catch (e) { setError(String(e)); }
    setLoading(false);
  }, [nodeUrl]);

  useEffect(() => { load(); }, [load]);

  const cols = ["TXID", "Fee Rate", "Size", "Inputs", "Outputs", "Fee", "Time"];

  return (
    <div style={{
      background: colors.surface, border: `1px solid ${colors.border}`,
      borderRadius: 12, overflow: "hidden",
    }}>
      {/* Header */}
      <div style={{
        display: "flex", alignItems: "center", justifyContent: "space-between",
        padding: "14px 20px", borderBottom: `1px solid ${colors.border}`,
      }}>
        <div>
          <span style={{ fontWeight: 700, fontSize: 15, color: colors.text }}>Mempool Transactions</span>
          {!loading && !error && (
            <span style={{
              marginLeft: 10, fontSize: 12, color: colors.muted,
              background: colors.surface2, border: `1px solid ${colors.border}`,
              borderRadius: 6, padding: "2px 8px",
            }}>{fmtNum(count)} pending</span>
          )}
        </div>
        <button onClick={load} style={{
          padding: "5px 14px", background: colors.surface2,
          border: `1px solid ${colors.border}`, borderRadius: 7,
          color: colors.muted, cursor: "pointer", fontSize: 12,
        }}>↻ Refresh</button>
      </div>

      {/* Error */}
      {error && (
        <div style={{ padding: "12px 20px", color: colors.red, fontSize: 13 }}>⚠ {error}</div>
      )}

      {/* Empty */}
      {!loading && !error && txs.length === 0 && (
        <div style={{ padding: 40, textAlign: "center", color: colors.muted, fontSize: 13 }}>
          Mempool is empty — no pending transactions
        </div>
      )}

      {/* Table */}
      {txs.length > 0 && (
        <div style={{ overflowX: "auto" }}>
          <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 13 }}>
            <thead>
              <tr style={{ borderBottom: `1px solid ${colors.border}` }}>
                {cols.map(c => (
                  <th key={c} style={{
                    padding: "10px 16px", textAlign: "left",
                    fontSize: 11, fontWeight: 700, textTransform: "uppercase",
                    letterSpacing: ".06em", color: colors.muted, whiteSpace: "nowrap",
                  }}>{c}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {txs.map((tx, i) => {
                const txid = tx.txid ?? tx.hash ?? "";
                const ts   = tx.timestamp;
                return (
                  <tr key={i}
                    onClick={() => txid && onTx?.(txid)}
                    style={{
                      borderBottom: `1px solid ${colors.border}`,
                      cursor: onTx && txid ? "pointer" : "default",
                      transition: "background .12s",
                    }}
                    onMouseEnter={e => (e.currentTarget.style.background = colors.surface2)}
                    onMouseLeave={e => (e.currentTarget.style.background = "transparent")}
                  >
                    <td style={{ padding: "12px 16px", fontFamily: fonts.mono, color: colors.accent, fontSize: 12 }}>
                      {txid ? shortHash(txid) : "—"}
                    </td>
                    <td style={{ padding: "12px 16px", fontFamily: fonts.mono, color: colors.text }}>
                      {tx.fee_rate != null ? fmtNum(tx.fee_rate, 1) + " sat/vB" : "—"}
                    </td>
                    <td style={{ padding: "12px 16px", color: colors.muted }}>
                      {tx.size != null ? fmtNum(tx.size) + " B" : "—"}
                    </td>
                    <td style={{ padding: "12px 16px", color: colors.text }}>{tx.inputs ?? "—"}</td>
                    <td style={{ padding: "12px 16px", color: colors.text }}>{tx.outputs ?? "—"}</td>
                    <td style={{ padding: "12px 16px", fontFamily: fonts.mono, color: colors.green }}>
                      {tx.fee != null ? fmtPkt(tx.fee) : "—"}
                    </td>
                    <td style={{ padding: "12px 16px", color: colors.muted, whiteSpace: "nowrap" }}>
                      {ts ? timeAgo(ts) : "—"}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}

      {loading && (
        <div style={{ padding: 40, textAlign: "center", color: colors.muted, fontSize: 13 }}>Loading…</div>
      )}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Main export
// ─────────────────────────────────────────────────────────────────────────────

export function Explorer({ nodeUrl, onBlock, onTx, subTab: subTabProp, onSubTab }: ExplorerProps) {
  const [localSub, setLocalSub] = useState<ExplorerSubTab>(subTabProp ?? "overview");
  const active = subTabProp ?? localSub;

  const handleChange = (t: ExplorerSubTab) => {
    setLocalSub(t);
    onSubTab?.(t);
  };

  return (
    <div>
      <style>{`
        @keyframes pulse   { 0%,100%{opacity:1} 50%{opacity:.3} }
        @keyframes slideIn { from{opacity:0;transform:translateY(-8px)} to{opacity:1;transform:translateY(0)} }
      `}</style>

      <SubTabBar active={active} onChange={handleChange} />

      {active === "overview"      && <OverviewPanel      nodeUrl={nodeUrl} onBlock={onBlock} />}
      {active === "blocks"        && <BlocksPanel        nodeUrl={nodeUrl} onBlock={onBlock} />}
      {active === "charts"        && <ChartsPanel        nodeUrl={nodeUrl} />}
      {active === "transactions"  && <TransactionsPanel  nodeUrl={nodeUrl} onTx={onTx} />}
    </div>
  );
}
