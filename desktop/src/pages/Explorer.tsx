// Explorer.tsx — v20.2 Live Dashboard
import { colors, fonts } from "../theme";
import { Panel } from "../components/Panel";
import { useLiveDashboard, type LiveEvent } from "../hooks/useLiveDashboard";
import { useAnimatedNumber } from "../hooks/useAnimatedNumber";
import { fmtHashrate, shortHash, timeAgo, type BlockHeader } from "../api";

interface ExplorerProps { nodeUrl: string; }

// ── Animated stat card ──────────────────────────────────────────────────────

interface LiveStatProps {
  icon:    string;
  label:   string;
  value:   number;
  fmt?:    (n: number) => string;
  color?:  string;
  unit?:   string;
  pulse?:  boolean;
}

function LiveStat({ icon, label, value, fmt, color, unit, pulse }: LiveStatProps) {
  const animated = useAnimatedNumber(value);
  const display  = fmt ? fmt(animated) : animated.toLocaleString();

  return (
    <div style={{
      background: colors.surface, border: `1px solid ${colors.border}`,
      borderRadius: 12, padding: "18px 20px",
      display: "flex", flexDirection: "column", gap: 8,
      position: "relative", overflow: "hidden",
    }}>
      {/* Subtle glow line top */}
      <div style={{
        position: "absolute", top: 0, left: 0, right: 0, height: 2,
        background: `linear-gradient(90deg, transparent, ${color ?? colors.accent}66, transparent)`,
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
            background: colors.green,
            boxShadow: `0 0 6px ${colors.green}`,
            animation: "pulse 2s infinite",
          }} />
        )}
      </div>

      <div style={{
        fontSize: 26, fontWeight: 700,
        color: color ?? colors.text,
        fontFamily: fonts.mono, letterSpacing: "-.02em",
      }}>
        {display}
        {unit && <span style={{ fontSize: 14, color: colors.muted, marginLeft: 4 }}>{unit}</span>}
      </div>
    </div>
  );
}

// ── Connection badge ─────────────────────────────────────────────────────────

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

// ── Block row ────────────────────────────────────────────────────────────────

function BlockRow({ b, i, total }: { b: BlockHeader; i: number; total: number }) {
  const h       = b.index ?? b.height ?? 0;
  const txCount = b.tx_count ?? 0;
  const ts      = b.timestamp ?? 0;

  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 12,
      padding: "11px 18px",
      borderBottom: i < total - 1 ? `1px solid ${colors.border}` : "none",
      animation: i === 0 ? "slideIn .4s ease" : "none",
    }}>
      <div style={{
        width: 38, height: 38, borderRadius: 8, flexShrink: 0,
        background: colors.surface2, border: `1px solid ${colors.border}`,
        display: "flex", alignItems: "center", justifyContent: "center",
        fontFamily: fonts.mono, fontWeight: 700, fontSize: 11, color: colors.accent,
      }}>
        {h.toString().slice(-4)}
      </div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontFamily: fonts.mono, fontWeight: 700, fontSize: 13, color: colors.text }}>
          #{h.toLocaleString()}
        </div>
        <div style={{ fontSize: 11, color: colors.muted, marginTop: 2, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {b.hash ? shortHash(b.hash) : "—"}
        </div>
      </div>
      <div style={{ textAlign: "right", flexShrink: 0 }}>
        <span style={{
          fontFamily: fonts.mono, fontSize: 11, fontWeight: 700,
          padding: "2px 8px", borderRadius: 4,
          background: `${colors.blue}22`, color: colors.blue, border: `1px solid ${colors.blue}44`,
        }}>{txCount} txs</span>
        <div style={{ fontSize: 11, color: colors.muted, marginTop: 4 }}>
          {ts ? timeAgo(ts) : "—"}
        </div>
      </div>
    </div>
  );
}

// ── Live event feed ──────────────────────────────────────────────────────────

function EventRow({ ev, i }: { ev: LiveEvent; i: number }) {
  const ts = new Date(ev.ts * 1000).toLocaleTimeString("vi-VN", {
    hour: "2-digit", minute: "2-digit", second: "2-digit",
  });

  return (
    <div style={{
      display: "flex", gap: 12, alignItems: "flex-start",
      padding: "10px 18px",
      borderBottom: `1px solid ${colors.border}`,
      animation: i === 0 ? "slideIn .4s ease" : "none",
    }}>
      <span style={{
        fontFamily: fonts.mono, fontSize: 11, color: colors.muted,
        flexShrink: 0, marginTop: 2, minWidth: 56,
      }}>{ts}</span>
      <div style={{ fontSize: 13, color: colors.text }}>
        {ev.type === "block" ? (
          <>
            New block{" "}
            <span style={{ color: colors.accent, fontFamily: fonts.mono, fontSize: 12 }}>
              #{(ev.height ?? 0).toLocaleString()}
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

// ── Main component ───────────────────────────────────────────────────────────

export function Explorer({ nodeUrl }: ExplorerProps) {
  const { summary, blocks, events, connected, error, refresh } = useLiveDashboard(nodeUrl);

  const height    = summary.height     ?? 0;
  const hashrate  = summary.hashrate   ?? 0;
  const mempool   = summary.mempool_count ?? 0;
  const blockTime = Math.round(summary.avg_block_time_s ?? 0);

  return (
    <div>
      <style>{`
        @keyframes pulse { 0%,100%{opacity:1} 50%{opacity:.3} }
        @keyframes slideIn { from{opacity:0;transform:translateY(-8px)} to{opacity:1;transform:translateY(0)} }
      `}</style>

      {error && (
        <div style={{
          marginBottom: 16, padding: "10px 16px", borderRadius: 8,
          background: "rgba(240,96,96,.08)", border: `1px solid rgba(240,96,96,.2)`,
          color: colors.red, fontSize: 13,
        }}>⚠ {error}</div>
      )}

      {/* Live stats */}
      <div style={{ display: "grid", gridTemplateColumns: "repeat(4,1fr)", gap: 12, marginBottom: 20 }}>
        <LiveStat icon="🧱" label="Block Height"
          value={height} color={colors.accent} pulse={connected} />
        <LiveStat icon="⚡" label="Hashrate"
          value={Math.round(hashrate / 1e9)}
          fmt={n => fmtHashrate(n * 1e9)} color={colors.blue} />
        <LiveStat icon="⏱" label="Block Time"
          value={blockTime} unit="s" color={colors.green} />
        <LiveStat icon="⏳" label="Mempool"
          value={mempool} color={colors.purple} unit="txs" />
      </div>

      {/* Two-column */}
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 16 }}>

        {/* Latest Blocks */}
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
            <BlockRow key={b.hash ?? i} b={b} i={i} total={Math.min(blocks.length, 10)} />
          ))}
        </Panel>

        {/* Live Event Feed */}
        <Panel icon="📡" title="Live Feed"
          right={<ConnBadge connected={connected} />}
        >
          {events.length === 0 ? (
            <div style={{ padding: 24, textAlign: "center", color: colors.muted, fontSize: 13 }}>
              {connected ? "Waiting for new blocks…" : "Connecting…"}
            </div>
          ) : events.map((ev, i) => (
            <EventRow key={ev.id} ev={ev} i={i} />
          ))}
        </Panel>
      </div>

      {/* Status bar */}
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
          <span>⚙ Difficulty: <span style={{ fontFamily: fonts.mono, color: colors.text }}>{summary.difficulty}</span></span>
        )}
        {summary.utxo_count !== undefined && (
          <span>🗃 UTXOs: <span style={{ fontFamily: fonts.mono, color: colors.text }}>{(summary.utxo_count as number).toLocaleString()}</span></span>
        )}
      </div>
    </div>
  );
}
