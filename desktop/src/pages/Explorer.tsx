// Explorer.tsx — v23.x: Unified Explorer (Overview + Blocks + Charts)
import { useState, useEffect, useCallback } from "react";
import { colors, fonts } from "../theme";
import { Panel } from "../components/Panel";
import { MiniChart, type ChartPoint } from "../components/MiniChart";
import { useLiveDashboard, type LiveEvent } from "../hooks/useLiveDashboard";
import { useAnimatedNumber } from "../hooks/useAnimatedNumber";
import {
  fetchBlocks, fetchAnalytics,
  fmtHashrate, fmtNum, shortHash, timeAgo,
  type BlockHeader, type AnalyticsSeries,
} from "../api";

// ─────────────────────────────────────────────────────────────────────────────
// Props
// ─────────────────────────────────────────────────────────────────────────────

export type ExplorerSubTab = "overview" | "blocks" | "charts";

interface ExplorerProps {
  nodeUrl:    string;
  onBlock:    (height: number) => void;
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
  { id: "overview", label: "Overview", icon: "📊" },
  { id: "blocks",   label: "Blocks",   icon: "🧱" },
  { id: "charts",   label: "Charts",   icon: "📈" },
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

function LiveStat({ icon, label, value, fmt, color, unit, pulse }: {
  icon: string; label: string; value: number;
  fmt?: (n: number) => string; color?: string; unit?: string; pulse?: boolean;
}) {
  const animated = useAnimatedNumber(value);
  const display  = fmt ? fmt(animated) : fmtNum(animated);
  return (
    <div style={{
      background: colors.surface, border: `1px solid ${colors.border}`,
      borderRadius: 12, padding: "18px 20px",
      display: "flex", flexDirection: "column", gap: 8,
      position: "relative", overflow: "hidden",
    }}>
      <div style={{
        position: "absolute", top: 0, left: 0, right: 0, height: 2,
        background: `linear-gradient(90deg,transparent,${color ?? colors.accent}66,transparent)`,
      }} />
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span style={{
          width: 32, height: 32, borderRadius: 8, fontSize: 16,
          background: colors.surface2, display: "flex",
          alignItems: "center", justifyContent: "center",
        }}>{icon}</span>
        <span style={{
          fontSize: 11, color: colors.muted, fontWeight: 700,
          textTransform: "uppercase", letterSpacing: ".07em",
        }}>{label}</span>
        {pulse && (
          <span style={{
            marginLeft: "auto", width: 7, height: 7, borderRadius: "50%",
            background: colors.green, boxShadow: `0 0 6px ${colors.green}`,
            animation: "pulse 2s infinite",
          }} />
        )}
      </div>
      <div style={{
        fontSize: 26, fontWeight: 700, color: color ?? colors.text,
        fontFamily: fonts.mono, letterSpacing: "-.02em",
      }}>
        {display}
        {unit && <span style={{ fontSize: 14, color: colors.muted, marginLeft: 4 }}>{unit}</span>}
      </div>
    </div>
  );
}

function BlockRowSmall({ b, i, total }: { b: BlockHeader; i: number; total: number }) {
  const h       = b.index ?? b.height ?? 0;
  const txCount = b.tx_count ?? 0;
  const ts      = b.timestamp ?? 0;
  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 12, padding: "11px 18px",
      borderBottom: i < total - 1 ? `1px solid ${colors.border}` : "none",
      animation: i === 0 ? "slideIn .4s ease" : "none",
    }}>
      <div style={{
        width: 38, height: 38, borderRadius: 8, flexShrink: 0,
        background: colors.surface2, border: `1px solid ${colors.border}`,
        display: "flex", alignItems: "center", justifyContent: "center",
        fontFamily: fonts.mono, fontWeight: 700, fontSize: 11, color: colors.accent,
      }}>{h.toString().slice(-4)}</div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontFamily: fonts.mono, fontWeight: 700, fontSize: 13, color: colors.text }}>
          #{fmtNum(h)}
        </div>
        <div style={{ fontSize: 11, color: colors.muted, marginTop: 2, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {b.hash ? shortHash(b.hash) : "—"}
        </div>
      </div>
      <div style={{ textAlign: "right", flexShrink: 0 }}>
        <span style={{
          fontFamily: fonts.mono, fontSize: 11, fontWeight: 700, padding: "2px 8px", borderRadius: 4,
          background: `${colors.blue}22`, color: colors.blue, border: `1px solid ${colors.blue}44`,
        }}>{txCount} txs</span>
        <div style={{ fontSize: 11, color: colors.muted, marginTop: 4 }}>{ts ? timeAgo(ts) : "—"}</div>
      </div>
    </div>
  );
}

function EventRow({ ev, i }: { ev: LiveEvent; i: number }) {
  const ts = new Date(ev.ts * 1000).toLocaleTimeString("vi-VN", {
    hour: "2-digit", minute: "2-digit", second: "2-digit",
  });
  return (
    <div style={{
      display: "flex", gap: 12, alignItems: "flex-start", padding: "10px 18px",
      borderBottom: `1px solid ${colors.border}`,
      animation: i === 0 ? "slideIn .4s ease" : "none",
    }}>
      <span style={{ fontFamily: fonts.mono, fontSize: 11, color: colors.muted, flexShrink: 0, marginTop: 2, minWidth: 56 }}>
        {ts}
      </span>
      <div style={{ fontSize: 13, color: colors.text }}>
        {ev.type === "block" ? (
          <>
            New block{" "}
            <span style={{ color: colors.accent, fontFamily: fonts.mono, fontSize: 12 }}>
              #{fmtNum(ev.height ?? 0)}
            </span>
            {ev.txCount !== undefined && (
              <span style={{ color: colors.muted, fontSize: 12 }}> · {ev.txCount} txs</span>
            )}
          </>
        ) : (
          <span style={{ color: colors.green }}>New transaction</span>
        )}
      </div>
    </div>
  );
}

function OverviewPanel({ nodeUrl }: { nodeUrl: string }) {
  const { summary, blocks, events, connected, error, refresh } = useLiveDashboard(nodeUrl);
  const height    = summary.height ?? 0;
  const hashrate  = summary.hashrate ?? 0;
  const mempool   = summary.mempool_count ?? 0;
  const blockTime = Math.round((summary.avg_block_time_s ?? summary.block_time_avg) ?? 0);

  return (
    <div>
      {error && (
        <div style={{
          marginBottom: 16, padding: "10px 16px", borderRadius: 8,
          background: "rgba(240,96,96,.08)", border: `1px solid rgba(240,96,96,.2)`,
          color: colors.red, fontSize: 13,
        }}>⚠ {error}</div>
      )}

      <div style={{ display: "grid", gridTemplateColumns: "repeat(4,1fr)", gap: 12, marginBottom: 20 }}>
        <LiveStat icon="🧱" label="Block Height"  value={height}     color={colors.accent} pulse={connected} />
        <LiveStat icon="⚡" label="Hashrate"      value={Math.round(hashrate / 1e9)} fmt={n => fmtHashrate(n * 1e9)} color={colors.blue} />
        <LiveStat icon="⏱" label="Block Time"    value={blockTime}  unit="s" color={colors.green} />
        <LiveStat icon="⏳" label="Mempool"       value={mempool}    color={colors.purple} unit="txs" />
      </div>

      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 16 }}>
        <Panel icon="🧱" title="Latest Blocks"
          right={
            <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
              <ConnBadge connected={connected} />
              <button onClick={refresh} style={{
                padding: "4px 12px", background: colors.surface2,
                border: `1px solid ${colors.border}`, borderRadius: 6,
                color: colors.muted, cursor: "pointer", fontSize: 12,
              }}>↻</button>
            </div>
          }
        >
          {blocks.length === 0 ? (
            <div style={{ padding: 24, textAlign: "center", color: colors.muted, fontSize: 13 }}>
              {connected ? "Loading…" : "Connecting…"}
            </div>
          ) : blocks.slice(0, 10).map((b, i) => (
            <BlockRowSmall key={b.hash ?? i} b={b} i={i} total={Math.min(blocks.length, 10)} />
          ))}
        </Panel>

        <Panel icon="📡" title="Live Feed" right={<ConnBadge connected={connected} />}>
          {events.length === 0 ? (
            <div style={{ padding: 24, textAlign: "center", color: colors.muted, fontSize: 13 }}>
              {connected ? "Waiting for new blocks…" : "Connecting…"}
            </div>
          ) : events.map((ev, i) => (
            <EventRow key={ev.id} ev={ev} i={i} />
          ))}
        </Panel>
      </div>

      <div style={{
        marginTop: 16, padding: "10px 20px",
        background: colors.surface, border: `1px solid ${colors.border}`,
        borderRadius: 10, display: "flex", gap: 32, alignItems: "center",
        fontSize: 12, color: colors.muted,
      }}>
        <ConnBadge connected={connected} />
        <span>🌐 <span style={{ color: colors.blue, fontFamily: fonts.mono }}>{nodeUrl}</span></span>
        <span>🔄 Poll: <span style={{ color: colors.green }}>8s</span></span>
        {summary.difficulty !== undefined && (
          <span>⚙ Difficulty: <span style={{ fontFamily: fonts.mono, color: colors.text }}>{(summary.difficulty as number).toFixed(2)}</span></span>
        )}
        {summary.utxo_count !== undefined && (
          <span>🗃 UTXOs: <span style={{ fontFamily: fonts.mono, color: colors.text }}>{fmtNum(summary.utxo_count as number)}</span></span>
        )}
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
// Main export
// ─────────────────────────────────────────────────────────────────────────────

export function Explorer({ nodeUrl, onBlock, subTab: subTabProp, onSubTab }: ExplorerProps) {
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

      {active === "overview" && <OverviewPanel nodeUrl={nodeUrl} />}
      {active === "blocks"   && <BlocksPanel   nodeUrl={nodeUrl} onBlock={onBlock} />}
      {active === "charts"   && <ChartsPanel   nodeUrl={nodeUrl} />}
    </div>
  );
}
