// useSearch.ts — v20.4: global search hook with debounce + localStorage recents
import { useState, useRef, useCallback, useEffect } from "react";
import { searchQuery } from "../api";

const RECENT_KEY = "pktscan_recent_searches";
const MAX_RECENTS = 8;
const DEBOUNCE_MS = 320;

export type QueryType = "block" | "tx" | "address" | "unknown";

export interface SearchResultItem {
  raw:   unknown;
  type:  QueryType;
  label: string;
  sub:   string;
}

export function detectType(q: string): QueryType {
  const t = q.trim();
  if (/^\d+$/.test(t)) return "block";
  if (/^[0-9a-fA-F]{64}$/.test(t)) return "tx";
  if (/^p[a-zA-Z0-9]{24,34}$/.test(t)) return "address";
  return "unknown";
}

function loadRecents(): string[] {
  try { return JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]"); }
  catch { return []; }
}

function persistRecent(q: string): string[] {
  const next = [q, ...loadRecents().filter(r => r !== q)].slice(0, MAX_RECENTS);
  localStorage.setItem(RECENT_KEY, JSON.stringify(next));
  return next;
}

function parseResults(raw: unknown, q: string): SearchResultItem[] {
  if (!raw || typeof raw !== "object") return [];
  const r = raw as Record<string, unknown>;
  const items: SearchResultItem[] = [];

  // Block array
  if (Array.isArray(r["blocks"])) {
    for (const b of r["blocks"] as Record<string, unknown>[]) {
      items.push({
        raw: b, type: "block",
        label: `Block #${b["height"] ?? b["index"] ?? "?"}`,
        sub:   String(b["hash"] ?? "").slice(0, 20) + "…",
      });
    }
  }
  // Single block
  if ((r["height"] !== undefined || r["index"] !== undefined) && r["hash"] !== undefined && !r["address"]) {
    items.push({
      raw: r, type: "block",
      label: `Block #${r["height"] ?? r["index"]}`,
      sub:   String(r["hash"]).slice(0, 20) + "…",
    });
  }
  // Address / balance
  if (r["address"] !== undefined) {
    items.push({
      raw: r, type: "address",
      label: String(r["address"]),
      sub:   r["balance"] !== undefined ? `${r["balance"]} PKT` : "address",
    });
  }
  // TX
  if (r["txid"] !== undefined) {
    items.push({
      raw: r, type: "tx",
      label: String(r["txid"]).slice(0, 20) + "…",
      sub:   r["amount"] !== undefined ? `${r["amount"]} PKT` : "transaction",
    });
  }
  // Fallback
  if (items.length === 0) {
    items.push({ raw: r, type: detectType(q), label: q.trim(), sub: JSON.stringify(r).slice(0, 60) });
  }
  return items;
}

export interface UseSearchReturn {
  query:        string;
  setQuery:     (q: string) => void;
  results:      SearchResultItem[];
  loading:      boolean;
  error:        string | null;
  recents:      string[];
  commitSearch: (q: string) => void;
  clearRecents: () => void;
}

export function useSearch(nodeUrl: string): UseSearchReturn {
  const [query,   setQuery]   = useState("");
  const [results, setResults] = useState<SearchResultItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [error,   setError]   = useState<string | null>(null);
  const [recents, setRecents] = useState<string[]>(() => loadRecents());

  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const doSearch = useCallback(async (q: string) => {
    setLoading(true);
    setError(null);
    try {
      const raw = await searchQuery(nodeUrl, q.trim());
      setResults(parseResults(raw, q));
    } catch (e) {
      setError(String(e));
      setResults([]);
    } finally {
      setLoading(false);
    }
  }, [nodeUrl]);

  useEffect(() => {
    if (timerRef.current) clearTimeout(timerRef.current);
    const q = query.trim();
    if (!q) { setResults([]); return; }
    timerRef.current = setTimeout(() => doSearch(q), DEBOUNCE_MS);
    return () => { if (timerRef.current) clearTimeout(timerRef.current); };
  }, [query, doSearch]);

  const commitSearch = useCallback((q: string) => {
    setRecents(persistRecent(q));
  }, []);

  const clearRecents = useCallback(() => {
    localStorage.removeItem(RECENT_KEY);
    setRecents([]);
  }, []);

  return { query, setQuery, results, loading, error, recents, commitSearch, clearRecents };
}
