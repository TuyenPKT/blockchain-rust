// Blocks.tsx — v20.6: click row → BlockDetail
import { useState, useEffect, useCallback } from "react";
import { colors, fonts } from "../theme";
import { Panel } from "../components/Panel";
import { fetchBlocks, fmtNum, shortHash, timeAgo, type BlockHeader } from "../api";

interface BlocksProps {
  nodeUrl: string;
  onBlock: (height: number) => void;
}

export function Blocks({ nodeUrl, onBlock }: BlocksProps) {
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
                    style={{
                      borderBottom: `1px solid ${colors.border}`,
                      cursor: "pointer", transition: "background .15s",
                    }}
                    onMouseEnter={e => (e.currentTarget.style.background = colors.surface2)}
                    onMouseLeave={e => (e.currentTarget.style.background = "transparent")}
                  >
                    <td style={{ padding: "12px 18px", fontFamily: fonts.mono, fontWeight: 700, color: colors.accent }}>
                      #{fmtNum(height)}
                    </td>
                    <td style={{ padding: "12px 18px", fontFamily: fonts.mono, fontSize: 12, color: colors.blue }}>
                      {b.hash ? shortHash(b.hash) : "—"}
                    </td>
                    <td style={{ padding: "12px 18px", color: colors.text }}>
                      {b.tx_count ?? "—"}
                    </td>
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
