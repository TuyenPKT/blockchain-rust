// useLiveDashboard.ts — v20.2
// Poll PKTScan API mỗi POLL_MS, detect block mới, emit live feed events.
import { useState, useEffect, useRef, useCallback } from "react";
import { fetchSummary, fetchBlocks, type NetworkSummary, type BlockHeader } from "../api";

export interface LiveEvent {
  id:        string;
  type:      "block" | "tx";
  height?:   number;
  hash?:     string;
  txCount?:  number;
  ts:        number;   // unix seconds
}

const POLL_MS    = 8_000;
const MAX_EVENTS = 20;

function genId() {
  return Math.random().toString(36).slice(2);
}

export function useLiveDashboard(nodeUrl: string) {
  const [summary,   setSummary]   = useState<NetworkSummary>({});
  const [blocks,    setBlocks]    = useState<BlockHeader[]>([]);
  const [events,    setEvents]    = useState<LiveEvent[]>([]);
  const [connected, setConnected] = useState(false);
  const [error,     setError]     = useState<string | null>(null);
  const lastHeightRef = useRef<number>(-1);

  const poll = useCallback(async () => {
    try {
      const [sum, blkData] = await Promise.all([
        fetchSummary(nodeUrl),
        fetchBlocks(nodeUrl, 15),
      ]);

      setSummary(sum);
      const list: BlockHeader[] = blkData.blocks ?? blkData.headers ?? [];
      setBlocks(list);
      setConnected(true);
      setError(null);

      // Detect new blocks since last poll
      const newBlocks = list.filter(b => {
        const h = b.index ?? b.height ?? 0;
        return h > lastHeightRef.current;
      });

      if (newBlocks.length > 0) {
        const maxH = Math.max(...newBlocks.map(b => b.index ?? b.height ?? 0));
        lastHeightRef.current = maxH;

        const newEvents: LiveEvent[] = newBlocks.map(b => ({
          id:      genId(),
          type:    "block",
          height:  b.index ?? b.height ?? 0,
          hash:    b.hash,
          txCount: b.tx_count ?? 0,
          ts:      b.timestamp ?? Math.floor(Date.now() / 1000),
        }));

        setEvents(prev => [...newEvents, ...prev].slice(0, MAX_EVENTS));
      } else if (lastHeightRef.current === -1 && list.length > 0) {
        // First load — seed lastHeight without emitting events
        lastHeightRef.current = Math.max(...list.map(b => b.index ?? b.height ?? 0));
      }
    } catch (e) {
      setConnected(false);
      setError(String(e));
    }
  }, [nodeUrl]);

  useEffect(() => {
    poll();
    const t = setInterval(poll, POLL_MS);
    return () => clearInterval(t);
  }, [poll]);

  return { summary, blocks, events, connected, error, refresh: poll };
}
