import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { colors, fonts } from "../theme";
import { t } from "../i18n";
import { Panel } from "../components/Panel";
import { StatCard } from "../components/StatCard";
import { fetchSummary, fmtHashrate, type NetworkSummary } from "../api";

interface MinerProps { nodeUrl: string; }

type MineStatus = "stopped" | "running";

interface MineStats {
  hashrate:     number;
  total_hashes: number;
  blocks_mined: number;
  uptime_secs:  number;
}

const DEFAULT_NODE = "seed.testnet.oceif.com:8337"; // pool (8337); direct node: :8334
const DEFAULT_THREADS = navigator.hardwareConcurrency
  ? Math.max(1, Math.floor(navigator.hardwareConcurrency / 3))
  : 2;

export function Miner({ nodeUrl }: MinerProps) {
  const [summary, setSummary]   = useState<NetworkSummary>({});
  const [status, setStatus]     = useState<MineStatus>("stopped");
  const [stats, setStats]       = useState<MineStats>({ hashrate: 0, total_hashes: 0, blocks_mined: 0, uptime_secs: 0 });
  const [logs, setLogs]         = useState<string[]>([t.miner_ready]);
  const [address, setAddress]   = useState("");
  const [mineNode, setMineNode] = useState(DEFAULT_NODE);
  const [threads, setThreads]   = useState(DEFAULT_THREADS);
  const [error, setError]       = useState("");
  const logsEndRef = useRef<HTMLDivElement>(null);

  // Auto-scroll logs
  useEffect(() => {
    logsEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logs]);

  // Network summary (for display)
  const refresh = useCallback(async () => {
    try { setSummary(await fetchSummary(nodeUrl)); } catch (_) {}
  }, [nodeUrl]);
  useEffect(() => {
    refresh();
    const t = setInterval(refresh, 15_000);
    return () => clearInterval(t);
  }, [refresh]);

  // Listen to miner events
  useEffect(() => {
    let unlistenLog: UnlistenFn | undefined;
    let unlistenStats: UnlistenFn | undefined;

    listen<string>("mine_log", (e) => {
      setLogs(prev => [...prev, `[${ts()}] ${e.payload}`]);
    }).then(fn => { unlistenLog = fn; });

    listen<MineStats>("mine_stats", (e) => {
      setStats(e.payload);
    }).then(fn => { unlistenStats = fn; });

    // Sync running state on mount
    invoke<boolean>("mine_status").then(running => {
      if (running) setStatus("running");
    }).catch(() => {});

    return () => {
      unlistenLog?.();
      unlistenStats?.();
    };
  }, []);

  async function startMining() {
    setError("");
    if (!address.trim()) {
      setError("Nhập địa chỉ PKT nhận reward (tpkt1... hoặc hex pubkey_hash)");
      return;
    }
    try {
      await invoke("start_mine", {
        address: address.trim(),
        nodeAddr: mineNode.trim(),
        threads,
      });
      setStatus("running");
      setStats({ hashrate: 0, total_hashes: 0, blocks_mined: 0, uptime_secs: 0 });
    } catch (e) {
      setError(String(e));
    }
  }

  function stopMining() {
    invoke("stop_mine").catch(() => {});
    setStatus("stopped");
  }

  function ts() {
    return new Date().toLocaleTimeString("vi-VN", { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  }

  function fmtElapsed(s: number) {
    const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), sec = s % 60;
    return [h, m, sec].map(v => String(v).padStart(2, "0")).join(":");
  }

  const netHashrate = summary.hashrate ?? 0;
  const shareOfNet  = netHashrate > 0 && stats.hashrate > 0
    ? ((stats.hashrate / netHashrate) * 100).toFixed(4) + "%" : "—";

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>

      {/* Stats row */}
      <div style={{ display: "grid", gridTemplateColumns: "repeat(4,1fr)", gap: 12 }}>
        <StatCard icon="⛏" label={t.miner_status}
          value={status === "running" ? t.miner_active : t.miner_stopped}
          color={status === "running" ? colors.green : colors.muted} />
        <StatCard icon="⚡" label={t.miner_hashrate}
          value={status === "running" ? fmtHashrate(stats.hashrate) : "0 H/s"}
          color={colors.blue} />
        <StatCard icon="🧱" label={t.miner_blocks}
          value={stats.blocks_mined}
          color={colors.accent} />
        <StatCard icon="⏱" label={t.miner_session}
          value={fmtElapsed(stats.uptime_secs)}
          color={colors.purple} />
      </div>

      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 16 }}>

        {/* Control panel */}
        <Panel icon="⛏" title={t.miner_control}>
          <div style={{ padding: 20, display: "flex", flexDirection: "column", gap: 14 }}>

            {/* Status dot */}
            <div style={{
              display: "flex", alignItems: "center", gap: 12,
              background: colors.surface2, border: `1px solid ${colors.border}`,
              borderRadius: 10, padding: "12px 16px",
            }}>
              <div style={{
                width: 12, height: 12, borderRadius: "50%", flexShrink: 0,
                background: status === "running" ? colors.green : colors.muted,
                boxShadow: status === "running" ? `0 0 8px ${colors.green}` : "none",
                animation: status === "running" ? "pulse 1.5s infinite" : "none",
              }} />
              <div style={{ flex: 1 }}>
                <div style={{ fontWeight: 700, fontSize: 14 }}>
                  {status === "running" ? t.miner_active : t.miner_stopped}
                </div>
                <div style={{ fontSize: 12, color: colors.muted, marginTop: 2 }}>
                  {status === "running"
                    ? `Elapsed: ${fmtElapsed(stats.uptime_secs)}`
                    : t.miner_start_hint}
                </div>
              </div>
            </div>

            {/* Address input */}
            <div>
              <label style={{ fontSize: 12, color: colors.muted, display: "block", marginBottom: 6 }}>
                {t.miner_address}
              </label>
              <input
                value={address}
                onChange={e => setAddress(e.target.value)}
                disabled={status === "running"}
                placeholder="tpkt1q… / 1… / hex pubkey_hash"
                style={{
                  width: "100%", boxSizing: "border-box",
                  background: colors.surface2, border: `1px solid ${error ? colors.red : colors.border}`,
                  borderRadius: 8, padding: "10px 12px", color: colors.text,
                  fontFamily: fonts.mono, fontSize: 12, outline: "none",
                  opacity: status === "running" ? 0.5 : 1,
                }}
              />
              {error && <div style={{ fontSize: 11, color: colors.red, marginTop: 4 }}>{error}</div>}
            </div>

            {/* Node address */}
            <div>
              <label style={{ fontSize: 12, color: colors.muted, display: "block", marginBottom: 6 }}>
                {t.miner_node}
              </label>
              <input
                value={mineNode}
                onChange={e => setMineNode(e.target.value)}
                disabled={status === "running"}
                placeholder="seed.testnet.oceif.com:8337"
                style={{
                  width: "100%", boxSizing: "border-box",
                  background: colors.surface2, border: `1px solid ${colors.border}`,
                  borderRadius: 8, padding: "10px 12px", color: colors.text,
                  fontFamily: fonts.mono, fontSize: 12, outline: "none",
                  opacity: status === "running" ? 0.5 : 1,
                }}
              />
            </div>

            {/* Threads slider */}
            <div>
              <label style={{ fontSize: 12, color: colors.muted, display: "block", marginBottom: 6 }}>
                {t.miner_threads}: <span style={{ color: colors.text, fontWeight: 700 }}>{threads}</span>
                {" "}
                <span style={{ fontSize: 11, color: colors.muted }}>
                  (cores: {navigator.hardwareConcurrency ?? "?"})
                </span>
              </label>
              <input
                type="range" min={1} max={navigator.hardwareConcurrency ?? 8}
                value={threads}
                onChange={e => setThreads(Number(e.target.value))}
                disabled={status === "running"}
                style={{ width: "100%", accentColor: colors.accent, opacity: status === "running" ? 0.5 : 1 }}
              />
            </div>

            {/* Network stats */}
            {[
              [t.net_hashrate,  fmtHashrate(netHashrate)],
              [t.net_share,     shareOfNet],
              [t.difficulty,    String(summary.difficulty ?? "—")],
              [t.block_height,  (summary.height ?? 0).toLocaleString()],
            ].map(([label, val]) => (
              <div key={label} style={{
                display: "flex", justifyContent: "space-between", alignItems: "center",
                padding: "6px 0", borderBottom: `1px solid ${colors.border}`,
              }}>
                <span style={{ fontSize: 13, color: colors.muted }}>{label}</span>
                <span style={{ fontFamily: fonts.mono, fontSize: 13, fontWeight: 700 }}>{val}</span>
              </div>
            ))}

            {/* Start / Stop */}
            <button
              onClick={status === "running" ? stopMining : startMining}
              style={{
                padding: "13px 0", borderRadius: 10, border: "none",
                fontWeight: 700, fontSize: 15, cursor: "pointer",
                background: status === "running"
                  ? "rgba(240,96,96,.15)"
                  : `linear-gradient(135deg, ${colors.accent}, #e07b10)`,
                color: status === "running" ? colors.red : "#000",
                outline: status === "running" ? `1px solid rgba(240,96,96,.3)` : "none",
                transition: "all .2s",
                marginTop: 4,
              }}
            >
              {status === "running" ? t.miner_stop : t.miner_start}
            </button>
          </div>
        </Panel>

        {/* Mine log */}
        <Panel icon="📋" title={t.miner_log}
          right={
            <button onClick={() => setLogs([])} style={{
              padding: "4px 12px", background: colors.surface2, border: `1px solid ${colors.border}`,
              borderRadius: 6, color: colors.muted, cursor: "pointer", fontSize: 12,
            }}>{t.clear}</button>
          }
        >
          <div style={{
            padding: 14, fontFamily: fonts.mono, fontSize: 12,
            lineHeight: 1.8, height: 380, overflow: "auto",
            display: "flex", flexDirection: "column",
          }}>
            {logs.map((l, i) => (
              <div key={i} style={{
                color: l.includes("🎉") || l.includes("✅") ? colors.accent
                  : l.includes("⚠️") || l.includes("stopped") ? colors.red
                  : l.includes("⏹") ? colors.muted
                  : colors.green,
                wordBreak: "break-all",
              }}>{l}</div>
            ))}
            <div ref={logsEndRef} />
          </div>
        </Panel>

      </div>

      <style>{`
        @keyframes pulse {
          0%, 100% { opacity: 1; }
          50% { opacity: .4; }
        }
        input[type="range"]::-webkit-slider-thumb { cursor: pointer; }
      `}</style>
    </div>
  );
}
