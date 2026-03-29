import { useState, useRef, useEffect } from "react";
import { colors, fonts } from "../theme";
import { fetchSummary, fetchBlocks, fetchBalance, searchQuery } from "../api";

interface TerminalProps {
  nodeUrl: string;
}

interface Line {
  type: "input" | "output" | "error" | "info";
  text: string;
}

const HELP = `PKTScan Terminal v20.1
Commands:
  summary          — network summary
  blocks [n]       — latest n blocks (default 5)
  balance <addr>   — PKT balance of address
  search <query>   — search block/tx/address
  clear            — clear terminal
  help             — show this help`;

export function Terminal({ nodeUrl }: TerminalProps) {
  const [lines, setLines]   = useState<Line[]>([{ type: "info", text: HELP }]);
  const [input, setInput]   = useState("");
  const [history, setHistory] = useState<string[]>([]);
  const [histIdx, setHistIdx] = useState(-1);
  const bottomRef = useRef<HTMLDivElement>(null);
  const inputRef  = useRef<HTMLInputElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [lines]);

  function addLine(type: Line["type"], text: string) {
    setLines(l => [...l, { type, text }]);
  }

  async function run(cmd: string) {
    const parts = cmd.trim().split(/\s+/);
    const command = parts[0].toLowerCase();

    switch (command) {
      case "help":
        addLine("info", HELP);
        break;
      case "clear":
        setLines([]);
        break;
      case "summary":
        try {
          const d = await fetchSummary(nodeUrl);
          addLine("output", JSON.stringify(d, null, 2));
        } catch (e) { addLine("error", String(e)); }
        break;
      case "blocks": {
        const n = parseInt(parts[1] ?? "5");
        try {
          const d = await fetchBlocks(nodeUrl, isNaN(n) ? 5 : n);
          addLine("output", JSON.stringify(d, null, 2));
        } catch (e) { addLine("error", String(e)); }
        break;
      }
      case "balance": {
        const addr = parts[1];
        if (!addr) { addLine("error", "Usage: balance <address>"); break; }
        try {
          const d = await fetchBalance(nodeUrl, addr);
          addLine("output", JSON.stringify(d, null, 2));
        } catch (e) { addLine("error", String(e)); }
        break;
      }
      case "search": {
        const q = parts.slice(1).join(" ");
        if (!q) { addLine("error", "Usage: search <query>"); break; }
        try {
          const d = await searchQuery(nodeUrl, q);
          addLine("output", JSON.stringify(d, null, 2));
        } catch (e) { addLine("error", String(e)); }
        break;
      }
      default:
        addLine("error", `Unknown command: ${command}. Type 'help' for list.`);
    }
  }

  async function submit() {
    if (!input.trim()) return;
    addLine("input", `[pktscan] > ${input}`);
    setHistory(h => [input, ...h.slice(0, 49)]);
    setHistIdx(-1);
    const cmd = input;
    setInput("");
    await run(cmd);
  }

  function onKeyDown(e: React.KeyboardEvent) {
    if (e.key === "Enter") { submit(); return; }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      const idx = Math.min(histIdx + 1, history.length - 1);
      setHistIdx(idx);
      setInput(history[idx] ?? "");
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      const idx = Math.max(histIdx - 1, -1);
      setHistIdx(idx);
      setInput(idx < 0 ? "" : history[idx] ?? "");
    }
  }

  function lineColor(type: Line["type"]) {
    switch (type) {
      case "input":  return colors.accent;
      case "output": return colors.green;
      case "error":  return colors.red;
      case "info":   return colors.blue;
    }
  }

  return (
    <div style={{
      background: colors.surface, border: `1px solid ${colors.border}`,
      borderRadius: 14, overflow: "hidden", display: "flex", flexDirection: "column",
      height: "calc(100vh - 140px)",
    }}>
      {/* Header */}
      <div style={{
        display: "flex", alignItems: "center", gap: 10, padding: "14px 18px",
        borderBottom: `1px solid ${colors.border}`,
      }}>
        <span style={{ fontSize: 16 }}>⬛</span>
        <span style={{ fontWeight: 700, fontSize: 14, color: colors.text, flex: 1 }}>Terminal</span>
        <span style={{ fontSize: 12, color: colors.muted, fontFamily: fonts.mono }}>
          {nodeUrl}
        </span>
        <button onClick={() => setLines([])} style={{
          padding: "4px 12px", background: colors.surface2, border: `1px solid ${colors.border}`,
          borderRadius: 6, color: colors.muted, cursor: "pointer", fontSize: 12,
        }}>Clear</button>
      </div>

      {/* Output */}
      <div
        onClick={() => inputRef.current?.focus()}
        style={{ flex: 1, overflow: "auto", padding: "14px 18px", cursor: "text" }}
      >
        {lines.map((l, i) => (
          <pre key={i} style={{
            margin: "0 0 6px", fontFamily: fonts.mono, fontSize: 12, lineHeight: 1.7,
            color: lineColor(l.type), whiteSpace: "pre-wrap", wordBreak: "break-all",
          }}>{l.text}</pre>
        ))}
        <div ref={bottomRef} />
      </div>

      {/* Input */}
      <div style={{
        display: "flex", alignItems: "center", gap: 10,
        padding: "10px 18px", borderTop: `1px solid ${colors.border}`,
        background: colors.surface2,
      }}>
        <span style={{ color: colors.accent, fontFamily: fonts.mono, fontSize: 13, flexShrink: 0 }}>
          [pktscan] &gt;
        </span>
        <input
          ref={inputRef}
          value={input}
          onChange={e => setInput(e.target.value)}
          onKeyDown={onKeyDown}
          autoFocus
          style={{
            flex: 1, background: "none", border: "none", outline: "none",
            color: colors.text, fontFamily: fonts.mono, fontSize: 13,
            caretColor: colors.accent,
          }}
          placeholder="Type a command…"
        />
      </div>
    </div>
  );
}
