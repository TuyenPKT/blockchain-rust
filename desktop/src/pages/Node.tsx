import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { colors, fonts } from "../theme";
import { t } from "../i18n";
import { Panel } from "../components/Panel";
import { StatCard } from "../components/StatCard";
import { fetchSummary, fmtHashrate, fmtNum, fmtPkt, type NetworkSummary } from "../api";

interface NodeProps { nodeUrl: string; }

interface PeerInfo {
  addr:       string;
  latency_ms: number | null;
  height:     number | null;
  status:     "online" | "timeout" | "refused" | "invalid";
}

const DEFAULT_SEED = "seed.testnet.oceif.com:8333";

export function Node({ nodeUrl }: NodeProps) {
  const [summary, setSummary]   = useState<NetworkSummary>({});
  const [lastUpdate, setLastUpdate] = useState<Date | null>(null);
  const [error, setError]       = useState("");
  const [loaded, setLoaded]     = useState(false);
  const [peers, setPeers]         = useState<PeerInfo[]>([]);
  const [scanning, setScanning]   = useState(false);
  const [seedAddr, setSeedAddr]   = useState(DEFAULT_SEED);
  const [nodeRunning, setNodeRunning] = useState(false);
  const [nodeMsg, setNodeMsg]     = useState("");

  const refresh = useCallback(async () => {
    try {
      setSummary(await fetchSummary(nodeUrl));
      setLastUpdate(new Date());
      setError("");
    } catch (e) {
      setError(String(e));
    } finally {
      setLoaded(true);
    }
  }, [nodeUrl]);

  useEffect(() => {
    setLoaded(false);
    setSummary({});
    setError("");
    refresh();
    const id = setInterval(refresh, 10_000);
    return () => clearInterval(id);
  }, [refresh]);

  const scanPeers = useCallback(async () => {
    if (scanning) return;
    setScanning(true);
    try {
      const result = await invoke<PeerInfo[]>("peer_scan", { seedAddr: seedAddr.trim() });
      setPeers(result);
    } catch (_) {}
    setScanning(false);
  }, [scanning, seedAddr]);

  // Auto-scan on mount
  useEffect(() => { scanPeers(); }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Poll node_running status
  useEffect(() => {
    let mounted = true;
    const poll = async () => {
      try {
        const running = await invoke<boolean>("node_running");
        if (mounted) setNodeRunning(running);
      } catch (_) {}
    };
    poll();
    const id = setInterval(poll, 3_000);
    return () => { mounted = false; clearInterval(id); };
  }, []);

  const startNode = useCallback(async () => {
    try {
      const msg = await invoke<string>("start_node", { port: 8336, peer: seedAddr.trim() });
      setNodeRunning(true);
      setNodeMsg(msg);
    } catch (e) { setNodeMsg(String(e)); }
  }, [seedAddr]);

  const stopNode = useCallback(async () => {
    try {
      await invoke("stop_node");
      setNodeRunning(false);
      setNodeMsg("Node stopped");
    } catch (e) { setNodeMsg(String(e)); }
  }, []);

  const height   = summary.height ?? 0;
  const mempool  = summary.mempool_count ?? 0;
  const utxos    = summary.utxo_count ?? 0;
  const hashrate = summary.hashrate ?? 0;
  const diff     = summary.difficulty ?? 0;
  const avgTime  = (summary.avg_block_time_s ?? summary.block_time_avg) ?? 0;
  const synced   = height > 0;

  // Extra fields returned by /api/testnet/summary
  const tipHash     = (summary["tip_hash"]     as string | undefined) ?? "";
  const totalValue  = (summary["total_value_sat"] as number | undefined) ?? 0;
  const blockReward = summary.block_reward ?? 0;

  function fmtTime(s: number) {
    if (!s) return "—";
    if (s < 60) return `${s.toFixed(1)}s`;
    return `${Math.floor(s / 60)}m ${(s % 60).toFixed(0)}s`;
  }

function shortHash(h: string) {
    if (!h || h === "0".repeat(64)) return "—";
    return h.slice(0, 12) + "…" + h.slice(-8);
  }

  function fmtUpdate(d: Date | null) {
    if (!d) return "—";
    return d.toLocaleTimeString("vi-VN", { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>

      {/* P2P Node control */}
      <div style={{
        display: "flex", alignItems: "center", gap: 12,
        background: colors.surface, border: `1px solid ${colors.border}`,
        borderRadius: 12, padding: "14px 20px",
      }}>
        <div style={{
          width: 10, height: 10, borderRadius: "50%", flexShrink: 0,
          background: nodeRunning ? colors.green : colors.muted,
          boxShadow: nodeRunning ? `0 0 8px ${colors.green}` : "none",
        }} />
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ fontSize: 13, fontWeight: 700, color: colors.text }}>
            P2P Node — {nodeRunning ? "running" : "stopped"}
          </div>
          {nodeRunning && (
            <div style={{ fontSize: 11, color: colors.muted, marginTop: 3, display: "flex", gap: 16 }}>
              <span>⬇ sync from <span style={{ color: colors.blue }}>{seedAddr}</span></span>
              <span>⬆ listening <span style={{ color: colors.green }}>:8336</span></span>
            </div>
          )}
          {nodeMsg && <div style={{ fontSize: 11, color: colors.muted, marginTop: 2 }}>{nodeMsg}</div>}
        </div>
        <button
          onClick={nodeRunning ? stopNode : startNode}
          style={{
            padding: "6px 18px", borderRadius: 8, cursor: "pointer",
            fontWeight: 700, fontSize: 13, flexShrink: 0,
            background: nodeRunning ? `${colors.red}22` : `${colors.green}22`,
            color: nodeRunning ? colors.red : colors.green,
            border: `1px solid ${nodeRunning ? colors.red : colors.green}`,
          }}
        >
          {nodeRunning ? "Stop" : "Start"}
        </button>
      </div>

      {/* Stats row */}
      <div style={{ display: "grid", gridTemplateColumns: "repeat(4,1fr)", gap: 12 }}>
        <StatCard icon="🖧"  label={t.node_status}
          value={!loaded ? t.connecting : error ? t.offline : synced ? t.online : t.connecting}
          color={!loaded ? colors.accent : error ? colors.red : synced ? colors.green : colors.accent} />
        <StatCard icon="🧱" label={t.block_height2}
          value={fmtNum(height)}
          color={colors.blue} />
        <StatCard icon="⏳" label={t.mempool}
          value={fmtNum(mempool)}
          color={colors.purple} sub="pending txs" />
        <StatCard icon="⚡" label={t.avg_time}
          value={fmtTime(avgTime)}
          color={colors.accent} />
      </div>

      {error && (
        <div style={{
          background: "rgba(240,96,96,.08)", border: `1px solid rgba(240,96,96,.3)`,
          borderRadius: 10, padding: "12px 18px", fontSize: 13, color: colors.red,
        }}>
          ⚠️ {error}
        </div>
      )}


      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 16 }}>

        {/* Node Info */}
        <Panel icon="🖧" title={t.node_info}
          right={
            <button onClick={refresh} style={{
              padding: "4px 12px", background: colors.surface2, border: `1px solid ${colors.border}`,
              borderRadius: 6, color: colors.muted, cursor: "pointer", fontSize: 12,
            }}>{t.refresh}</button>
          }
        >
          <div style={{ padding: "16px 20px", display: "flex", flexDirection: "column", gap: 0 }}>
            {[
              ["Status",      !loaded ? t.connecting : error ? t.offline : synced ? t.synced : t.connecting],
              ["Node URL",    nodeUrl],
              ["Height",       fmtNum(height)],
              ["Tip Hash",     shortHash(tipHash)],
              ["Block Reward", fmtPkt(blockReward)],
              ["Total Supply", fmtPkt(totalValue)],
              ["Last Update",  fmtUpdate(lastUpdate)],
              ["Protocol",    "PKT Wire v1"],
            ].map(([label, val]) => (
              <div key={label} style={{
                display: "flex", justifyContent: "space-between", alignItems: "center",
                padding: "10px 0", borderBottom: `1px solid ${colors.border}`,
              }}>
                <span style={{ fontSize: 13, color: colors.muted, flexShrink: 0, marginRight: 12 }}>{label}</span>
                <span style={{
                  fontFamily: fonts.mono, fontSize: 12, fontWeight: 700, textAlign: "right",
                  color: String(val).includes("✓") ? colors.green
                    : label === "Node URL" ? colors.blue
                    : colors.text,
                  overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", maxWidth: 220,
                }}>{val}</span>
              </div>
            ))}
          </div>
        </Panel>

        {/* Network Stats */}
        <Panel icon="📊" title={t.net_stats}>
          <div style={{ padding: "16px 20px", display: "flex", flexDirection: "column", gap: 0 }}>
            {[
              [t.net_hashrate,  fmtHashrate(hashrate)],
              [t.difficulty,    diff ? fmtNum(diff) : "—"],
              [t.avg_time,      fmtTime(avgTime)],
              ["UTXO Count",    fmtNum(utxos)],
              [t.mempool,       fmtNum(mempool)],
              ["Total Supply",  fmtPkt(totalValue)],
              ["Block Reward",  fmtPkt(blockReward)],
            ].map(([label, val]) => (
              <div key={label} style={{
                display: "flex", justifyContent: "space-between", alignItems: "center",
                padding: "10px 0", borderBottom: `1px solid ${colors.border}`,
              }}>
                <span style={{ fontSize: 13, color: colors.muted }}>{label}</span>
                <span style={{ fontFamily: fonts.mono, fontSize: 13, fontWeight: 700 }}>{val}</span>
              </div>
            ))}

            {/* Hashrate bar */}
            {hashrate > 0 && (
              <div style={{ marginTop: 16 }}>
                <div style={{ fontSize: 12, color: colors.muted, marginBottom: 6 }}>
                  {t.net_hashrate}
                </div>
                <div style={{ height: 6, background: colors.surface3, borderRadius: 3 }}>
                  <div style={{
                    height: "100%", borderRadius: 3,
                    width: "100%",
                    background: `linear-gradient(90deg, ${colors.accent}, ${colors.blue})`,
                  }} />
                </div>
                <div style={{
                  fontFamily: fonts.mono, fontSize: 13, fontWeight: 700,
                  color: colors.accent, marginTop: 6, textAlign: "center",
                }}>
                  {fmtHashrate(hashrate)}
                </div>
              </div>
            )}
          </div>
        </Panel>

      </div>

      {/* Peers panel */}
      <Panel icon="🔗" title={t.peers_title}
        right={
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              value={seedAddr}
              onChange={e => setSeedAddr(e.target.value)}
              placeholder="seed:port"
              style={{
                background: colors.surface2, border: `1px solid ${colors.border}`,
                borderRadius: 6, padding: "4px 10px", color: colors.text,
                fontFamily: fonts.mono, fontSize: 11, outline: "none", width: 200,
              }}
            />
            <button onClick={scanPeers} disabled={scanning} style={{
              padding: "4px 14px", background: scanning ? colors.surface2 : `${colors.blue}22`,
              border: `1px solid ${scanning ? colors.border : colors.blue}`,
              borderRadius: 6, color: scanning ? colors.muted : colors.blue,
              cursor: scanning ? "wait" : "pointer", fontSize: 12, fontWeight: 600,
              transition: "all .2s",
            }}>
              {scanning ? t.peers_scanning : t.peers_scan}
            </button>
          </div>
        }
      >
        <div style={{ overflowX: "auto" }}>
          {peers.length === 0 && !scanning && (
            <div style={{ padding: "20px 20px", fontSize: 13, color: colors.muted, textAlign: "center" }}>
              {t.peers_none}
            </div>
          )}
          {scanning && peers.length === 0 && (
            <div style={{ padding: "20px 20px", fontSize: 13, color: colors.muted, textAlign: "center" }}>
              {t.peers_scanning}
            </div>
          )}
          {peers.length > 0 && (
            <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 13 }}>
              <thead>
                <tr style={{ borderBottom: `1px solid ${colors.border}` }}>
                  {[t.peers_addr, t.peers_latency, t.peers_height, t.peers_status].map(h => (
                    <th key={h} style={{
                      padding: "9px 18px", textAlign: "left",
                      fontSize: 11, fontWeight: 700, textTransform: "uppercase",
                      letterSpacing: ".07em", color: colors.muted,
                    }}>{h}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {peers.map((p, i) => {
                  const statusColor = p.status === "online" ? colors.green
                    : p.status === "refused" ? colors.accent : colors.red;
                  const statusLabel = p.status === "online" ? t.peers_online
                    : p.status === "refused" ? t.peers_refused
                    : p.status === "invalid" ? t.peers_invalid : t.peers_timeout;
                  const isSeed = p.addr === seedAddr.trim();
                  return (
                    <tr key={i} style={{ borderBottom: `1px solid ${colors.border}` }}>
                      <td style={{ padding: "10px 18px" }}>
                        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                          <div style={{
                            width: 7, height: 7, borderRadius: "50%", flexShrink: 0,
                            background: statusColor,
                            boxShadow: p.status === "online" ? `0 0 5px ${statusColor}` : "none",
                          }} />
                          <span style={{ fontFamily: fonts.mono, fontSize: 12, color: colors.text }}>
                            {p.addr}
                          </span>
                          {isSeed && (
                            <span style={{
                              fontSize: 10, padding: "1px 6px", borderRadius: 4,
                              background: `${colors.accent}22`, color: colors.accent,
                              border: `1px solid ${colors.accent}44`,
                            }}>{t.peers_seed}</span>
                          )}
                        </div>
                      </td>
                      <td style={{ padding: "10px 18px" }}>
                        <span style={{
                          fontFamily: fonts.mono, fontSize: 12,
                          color: p.latency_ms === null ? colors.muted
                            : p.latency_ms < 100 ? colors.green
                            : p.latency_ms < 300 ? colors.accent : colors.red,
                        }}>
                          {p.latency_ms !== null ? `${p.latency_ms} ms` : "—"}
                        </span>
                      </td>
                      <td style={{ padding: "10px 18px", fontFamily: fonts.mono, fontSize: 12 }}>
                        {p.height !== null ? fmtNum(p.height) : "—"}
                      </td>
                      <td style={{ padding: "10px 18px" }}>
                        <span style={{
                          fontSize: 11, fontWeight: 700, padding: "2px 8px", borderRadius: 4,
                          background: statusColor + "18",
                          color: statusColor,
                          border: `1px solid ${statusColor}44`,
                        }}>
                          {statusLabel}
                        </span>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
        </div>
      </Panel>

    </div>
  );
}
