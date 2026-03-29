// SearchBar.tsx — v20.4: Cmd+K global search overlay with result preview + keyboard nav
import { useEffect, useRef, useState, useCallback } from "react";
import { colors, fonts } from "../theme";
import { useSearch, detectType, QueryType, SearchResultItem } from "../hooks/useSearch";

type Tab = "explorer" | "blocks" | "charts" | "miner" | "node" | "terminal"
         | "address" | "block-detail" | "tx-detail";

export interface SearchBarProps {
  nodeUrl:    string;
  onNavigate: (tab: Tab, meta?: SearchResultItem) => void;
}

// ── helpers ───────────────────────────────────────────────────────────────────

const TYPE_LABEL: Record<QueryType, string> = {
  block: "Block", tx: "TX", address: "Addr", unknown: "?",
};

function tabForType(t: QueryType): Tab {
  if (t === "block")   return "blocks";
  if (t === "address") return "address";
  if (t === "tx")      return "explorer"; // App.tsx handles tx → tx-detail
  return "explorer";
}

function typeColor(type: QueryType): string {
  if (type === "block")   return colors.accent;
  if (type === "unknown") return colors.muted;
  if (type === "tx")      return "#a78bfa";
  return "#34d399"; // address
}

function TypeBadge({ type }: { type: QueryType }) {
  const c = typeColor(type);
  return (
    <span style={{
      fontSize: 10, fontFamily: fonts.mono, fontWeight: 700,
      color: c, background: c + "22", border: `1px solid ${c}55`,
      borderRadius: 4, padding: "1px 6px", flexShrink: 0,
    }}>
      {TYPE_LABEL[type]}
    </span>
  );
}

// ── SearchBar modal ───────────────────────────────────────────────────────────

export function SearchBar({ nodeUrl, onNavigate }: SearchBarProps) {
  const [open,   setOpen]   = useState(false);
  const [cursor, setCursor] = useState(-1);
  const inputRef = useRef<HTMLInputElement>(null);

  const kbdStyle: React.CSSProperties = {
    background: colors.surface2,
    border: `1px solid ${colors.border}`,
    borderRadius: 3, padding: "1px 5px",
    fontFamily: "inherit",
  };

  const {
    query, setQuery, results, loading, error,
    recents, commitSearch, clearRecents,
  } = useSearch(nodeUrl);

  // Global Cmd+K / Ctrl+K
  useEffect(() => {
    function onGlobalKey(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        setOpen(o => !o);
      }
      if (e.key === "Escape") setOpen(false);
    }
    window.addEventListener("keydown", onGlobalKey);
    return () => window.removeEventListener("keydown", onGlobalKey);
  }, []);

  // Focus & reset on open/close
  useEffect(() => {
    if (open) {
      setTimeout(() => inputRef.current?.focus(), 40);
      setCursor(-1);
    } else {
      setQuery("");
    }
  }, [open, setQuery]);

  const close = useCallback(() => setOpen(false), []);

  // Build flat list for keyboard nav
  const showRecents = query.trim() === "" && recents.length > 0;
  type ListItem = { label: string; sub: string; isRecent: boolean; item?: SearchResultItem };
  const listItems: ListItem[] = showRecents
    ? recents.map(r => ({ label: r, sub: "recent", isRecent: true }))
    : results.map(r => ({ label: r.label, sub: r.sub, isRecent: false, item: r }));

  const selectIdx = useCallback((i: number) => {
    const entry = listItems[i];
    if (!entry) return;
    if (entry.isRecent) {
      setQuery(entry.label);
      return;
    }
    if (entry.item) {
      commitSearch(query.trim());
      onNavigate(tabForType(entry.item.type), entry.item);
      close();
    }
  }, [listItems, query, commitSearch, onNavigate, close, setQuery]);

  function onInputKey(e: React.KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setCursor(c => Math.min(c + 1, listItems.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setCursor(c => Math.max(c - 1, -1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (cursor >= 0) {
        selectIdx(cursor);
      } else if (query.trim()) {
        commitSearch(query.trim());
        onNavigate(tabForType(detectType(query.trim())));
        close();
      }
    }
  }

  if (!open) return null;

  const hasContent = listItems.length > 0 || !!error || loading;

  return (
    <>
      {/* Backdrop */}
      <div onClick={close} style={{
        position: "fixed", inset: 0, zIndex: 999,
        background: "rgba(0,0,0,0.55)",
        backdropFilter: "blur(4px)",
      }} />

      {/* Modal */}
      <div style={{
        position: "fixed", top: "14%", left: "50%",
        transform: "translateX(-50%)",
        zIndex: 1000, width: "min(640px, 90vw)",
        background: colors.surface,
        border: `1px solid ${colors.border}`,
        borderRadius: 16,
        boxShadow: "0 24px 80px rgba(0,0,0,0.65)",
        overflow: "hidden",
      }}>

        {/* Input row */}
        <div style={{
          display: "flex", alignItems: "center", gap: 12,
          padding: "14px 20px",
          borderBottom: hasContent ? `1px solid ${colors.border}` : "none",
        }}>
          <svg width="17" height="17" viewBox="0 0 24 24" fill="none"
            stroke={loading ? colors.accent : colors.muted}
            strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
            <circle cx="11" cy="11" r="8" />
            <line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>
          <input
            ref={inputRef}
            value={query}
            onChange={e => { setQuery(e.target.value); setCursor(-1); }}
            onKeyDown={onInputKey}
            placeholder="Block height, txid, PKT address…"
            style={{
              flex: 1, background: "none", border: "none", outline: "none",
              color: colors.text, fontFamily: fonts.sans, fontSize: 15,
            }}
            spellCheck={false} autoComplete="off"
          />
          <kbd style={kbdStyle}>ESC</kbd>
        </div>

        {/* Dropdown */}
        {hasContent && (
          <div style={{ maxHeight: 360, overflowY: "auto" }}>

            {/* Section header */}
            {listItems.length > 0 && (
              <div style={{
                display: "flex", justifyContent: "space-between", alignItems: "center",
                padding: "8px 20px 4px",
              }}>
                <span style={{
                  fontSize: 10, color: colors.muted, fontFamily: fonts.mono,
                  fontWeight: 700, letterSpacing: "0.07em", textTransform: "uppercase",
                }}>
                  {showRecents ? "Recent" : `${listItems.length} result${listItems.length !== 1 ? "s" : ""}`}
                </span>
                {showRecents && (
                  <button onClick={clearRecents} style={{
                    background: "none", border: "none", cursor: "pointer",
                    fontSize: 11, color: colors.muted, fontFamily: fonts.sans, padding: 0,
                  }}>Clear</button>
                )}
              </div>
            )}

            {/* Error */}
            {error && (
              <div style={{ padding: "10px 20px", fontSize: 13, color: "#f87171", fontFamily: fonts.mono }}>
                {error}
              </div>
            )}

            {/* Loading */}
            {loading && listItems.length === 0 && (
              <div style={{ padding: "16px 20px", fontSize: 13, color: colors.muted, fontFamily: fonts.mono }}>
                Searching…
              </div>
            )}

            {/* Items */}
            {listItems.map((item, i) => (
              <div
                key={i}
                onClick={() => selectIdx(i)}
                onMouseEnter={() => setCursor(i)}
                style={{
                  display: "flex", alignItems: "center", gap: 12,
                  padding: "10px 20px", cursor: "pointer",
                  background: cursor === i ? colors.surface2 : "transparent",
                  transition: "background 0.1s",
                }}>
                {item.isRecent
                  ? <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                      stroke={colors.muted} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                      <polyline points="1 4 1 10 7 10" />
                      <path d="M3.51 15a9 9 0 1 0 .49-4" />
                    </svg>
                  : item.item && <TypeBadge type={item.item.type} />
                }
                <span style={{
                  fontFamily: fonts.mono, fontSize: 13, color: colors.text,
                  flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                }}>
                  {item.label}
                </span>
                {item.sub && (
                  <span style={{ fontSize: 12, color: colors.muted, fontFamily: fonts.sans, flexShrink: 0 }}>
                    {item.sub}
                  </span>
                )}
                {cursor === i && (
                  <svg width="13" height="13" viewBox="0 0 24 24" fill="none"
                    stroke={colors.muted} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <polyline points="9 18 15 12 9 6" />
                  </svg>
                )}
              </div>
            ))}
          </div>
        )}

        {/* Footer hints */}
        <div style={{
          display: "flex", gap: 16, padding: "8px 20px",
          borderTop: `1px solid ${colors.border}`,
          fontSize: 11, color: colors.muted, fontFamily: fonts.mono,
        }}>
          <span><kbd style={kbdStyle}>↑↓</kbd> navigate</span>
          <span><kbd style={kbdStyle}>↵</kbd> select</span>
          <span><kbd style={kbdStyle}>⌘K</kbd> toggle</span>
        </div>
      </div>
    </>
  );
}

// ── SearchTrigger — compact button shown in Nav ───────────────────────────────

export function SearchTrigger({ onClick }: { onClick: () => void }) {
  const isMac = typeof navigator !== "undefined"
    && navigator.platform.toUpperCase().includes("MAC");
  return (
    <button
      onClick={onClick}
      style={{
        display: "flex", alignItems: "center", gap: 8,
        background: colors.surface2, border: `1px solid ${colors.border}`,
        borderRadius: 8, padding: "5px 12px", cursor: "pointer",
        color: colors.muted, fontFamily: fonts.sans, fontSize: 13,
        marginRight: 12, transition: "border-color .2s",
      }}
      onMouseEnter={e => (e.currentTarget.style.borderColor = colors.accent)}
      onMouseLeave={e => (e.currentTarget.style.borderColor = colors.border)}
    >
      <svg width="13" height="13" viewBox="0 0 24 24" fill="none"
        stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
        <circle cx="11" cy="11" r="8" /><line x1="21" y1="21" x2="16.65" y2="16.65" />
      </svg>
      <span>Search</span>
      <kbd style={{
        fontSize: 10, background: colors.surface,
        border: `1px solid ${colors.border}`,
        borderRadius: 3, padding: "1px 5px", fontFamily: fonts.mono,
      }}>{isMac ? "⌘K" : "Ctrl+K"}</kbd>
    </button>
  );
}
