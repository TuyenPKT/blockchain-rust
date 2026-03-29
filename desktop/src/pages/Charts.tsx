// Charts.tsx — v20.3 Analytics Charts
import { useState, useEffect, useCallback } from "react";
import { colors, fonts } from "../theme";
import { Panel } from "../components/Panel";
import { MiniChart, type ChartPoint } from "../components/MiniChart";
import { fetchAnalytics, fmtHashrate, type AnalyticsSeries } from "../api";

interface ChartsProps { nodeUrl: string; }

type Window = 50 | 100 | 200 | 500;

const WINDOWS: Window[] = [50, 100, 200, 500];

function toPoints(series: AnalyticsSeries): ChartPoint[] {
  return series.points.map(p => ({ x: p.height, y: p.value }));
}

function statOf(pts: ChartPoint[]) {
  if (!pts.length) return { min: 0, max: 0, avg: 0, last: 0 };
  const vals = pts.map(p => p.y);
  const min  = Math.min(...vals);
  const max  = Math.max(...vals);
  const avg  = vals.reduce((a, b) => a + b, 0) / vals.length;
  const last = vals[vals.length - 1];
  return { min, max, avg, last };
}

interface ChartCardProps {
  title:    string;
  icon:     string;
  series:   AnalyticsSeries | null;
  color:    string;
  fmt:      (v: number) => string;
  loading:  boolean;
  error:    string | null;
}

function ChartCard({ title, icon, series, color, fmt, loading, error }: ChartCardProps) {
  const pts   = series ? toPoints(series) : [];
  const stats = statOf(pts);

  return (
    <Panel icon={icon} title={title}>
      <div style={{ padding: "14px 18px" }}>

        {/* Big value */}
        <div style={{ marginBottom: 12 }}>
          <div style={{
            fontFamily: fonts.mono, fontWeight: 700, fontSize: 24,
            color, letterSpacing: "-.02em",
          }}>
            {loading ? "…" : error ? "—" : fmt(stats.last)}
          </div>
          <div style={{ fontSize: 11, color: colors.muted, marginTop: 2 }}>
            Current · {series?.unit ?? ""}
          </div>
        </div>

        {/* Chart */}
        <div style={{
          background: colors.surface2, border: `1px solid ${colors.border}`,
          borderRadius: 10, overflow: "hidden", marginBottom: 12,
        }}>
          {loading ? (
            <div style={{ height: 120, display: "flex", alignItems: "center", justifyContent: "center", color: colors.muted, fontSize: 13 }}>
              Loading…
            </div>
          ) : error ? (
            <div style={{ height: 120, display: "flex", alignItems: "center", justifyContent: "center", color: colors.red, fontSize: 13 }}>
              {error}
            </div>
          ) : pts.length < 2 ? (
            <div style={{ height: 120, display: "flex", alignItems: "center", justifyContent: "center", color: colors.muted, fontSize: 13 }}>
              Not enough data
            </div>
          ) : (
            <MiniChart points={pts} color={color} height={120} filled />
          )}
        </div>

        {/* Min / Avg / Max */}
        {!loading && !error && pts.length > 0 && (
          <div style={{ display: "grid", gridTemplateColumns: "repeat(3,1fr)", gap: 8 }}>
            {[
              ["Min", fmt(stats.min)],
              ["Avg", fmt(stats.avg)],
              ["Max", fmt(stats.max)],
            ].map(([label, val]) => (
              <div key={label} style={{
                background: colors.surface2, border: `1px solid ${colors.border}`,
                borderRadius: 8, padding: "8px 12px", textAlign: "center",
              }}>
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

export function Charts({ nodeUrl }: ChartsProps) {
  const [window, setWindow]   = useState<Window>(100);
  const [hashrate, setHashrate] = useState<AnalyticsSeries | null>(null);
  const [blockTime, setBlockTime] = useState<AnalyticsSeries | null>(null);
  const [difficulty, setDifficulty] = useState<AnalyticsSeries | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError]     = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [hr, bt, diff] = await Promise.all([
        fetchAnalytics(nodeUrl, "hashrate",   window),
        fetchAnalytics(nodeUrl, "block_time", window),
        fetchAnalytics(nodeUrl, "difficulty", window),
      ]);
      setHashrate(hr);
      setBlockTime(bt);
      setDifficulty(diff);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [nodeUrl, window]);

  useEffect(() => { load(); }, [load]);

  // Sparkline for difficulty as bar-like
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
            <button key={w} onClick={() => setWindow(w)} style={{
              padding: "5px 14px", borderRadius: 7, border: "none", cursor: "pointer",
              fontFamily: fonts.mono, fontSize: 13, fontWeight: 700,
              background: window === w ? colors.accent : colors.surface2,
              color: window === w ? "#000" : colors.muted,
              transition: "all .2s",
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
        <div style={{
          padding: "10px 16px", borderRadius: 8,
          background: "rgba(240,96,96,.08)", border: `1px solid rgba(240,96,96,.2)`,
          color: colors.red, fontSize: 13,
        }}>⚠ {error}</div>
      )}

      {/* 2-col top */}
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 16 }}>
        <ChartCard
          icon="⚡" title="Hashrate"
          series={hashrate} color={colors.blue}
          fmt={v => fmtHashrate(v)}
          loading={loading} error={error}
        />
        <ChartCard
          icon="⏱" title="Block Time"
          series={blockTime} color={colors.green}
          fmt={v => v.toFixed(1) + "s"}
          loading={loading} error={error}
        />
      </div>

      {/* Difficulty full-width */}
      <Panel icon="⚙" title="Difficulty">
        <div style={{ padding: "14px 18px" }}>
          <div style={{ display: "flex", alignItems: "baseline", gap: 16, marginBottom: 12 }}>
            <div style={{
              fontFamily: fonts.mono, fontWeight: 700, fontSize: 24,
              color: colors.purple,
            }}>
              {loading ? "…" : error ? "—" : diffStats.last.toFixed(2)}
            </div>
            <div style={{ fontSize: 11, color: colors.muted }}>Current difficulty</div>
            {!loading && !error && diffStats.last > 0 && (
              <div style={{ marginLeft: "auto", display: "flex", gap: 16 }}>
                {[["Min", diffStats.min.toFixed(2)], ["Avg", diffStats.avg.toFixed(2)], ["Max", diffStats.max.toFixed(2)]].map(([l, v]) => (
                  <div key={l} style={{ textAlign: "center" }}>
                    <div style={{ fontSize: 10, color: colors.muted, textTransform: "uppercase", letterSpacing: ".06em" }}>{l}</div>
                    <div style={{ fontFamily: fonts.mono, fontWeight: 700, fontSize: 14, color: colors.purple }}>{v}</div>
                  </div>
                ))}
              </div>
            )}
          </div>

          <div style={{
            background: colors.surface2, border: `1px solid ${colors.border}`,
            borderRadius: 10, overflow: "hidden",
          }}>
            {loading ? (
              <div style={{ height: 100, display: "flex", alignItems: "center", justifyContent: "center", color: colors.muted, fontSize: 13 }}>Loading…</div>
            ) : diffPts.length >= 2 ? (
              <MiniChart points={diffPts} color={colors.purple} height={100} filled />
            ) : (
              <div style={{ height: 100, display: "flex", alignItems: "center", justifyContent: "center", color: colors.muted, fontSize: 13 }}>Not enough data</div>
            )}
          </div>

          {/* X-axis labels */}
          {!loading && diffPts.length > 1 && (
            <div style={{ display: "flex", justifyContent: "space-between", marginTop: 6, paddingLeft: 4, paddingRight: 4 }}>
              <span style={{ fontFamily: fonts.mono, fontSize: 10, color: colors.muted }}>
                #{diffPts[0]?.x.toLocaleString()}
              </span>
              <span style={{ fontFamily: fonts.mono, fontSize: 10, color: colors.muted }}>
                #{diffPts[diffPts.length - 1]?.x.toLocaleString()}
              </span>
            </div>
          )}
        </div>
      </Panel>
    </div>
  );
}
