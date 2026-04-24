// theme.ts — v21.1: dark + light + auto color tokens
import type { ThemeMode } from "./hooks/useSettings";

const dark = {
  bg:       "#070c14",
  surface:  "#0d1117",
  surface2: "#111827",
  surface3: "#1a2235",
  border:   "rgba(255,255,255,0.08)",
  accent:   "#6366F1",
  blue:     "#3B82F6",
  green:    "#22C55E",
  red:      "#EF4444",
  purple:   "#8B5CF6",
  text:     "#E2E8F0",
  muted:    "#64748B",
  navBg:    "#0d1117",
};

const light = {
  bg:       "#f4f6fb",
  surface:  "#ffffff",
  surface2: "#eaeff8",
  surface3: "#dde5f4",
  border:   "#ccd6eb",
  accent:   "#d97706",
  blue:     "#1d6fd8",
  green:    "#15a043",
  red:      "#dc3030",
  purple:   "#7c3fc0",
  text:     "#0d1829",
  muted:    "#64748b",
  navBg:    "#ffffff",
};

export let colors = dark;

/** Resolve "auto" → "dark" | "light" using system preference. */
export function resolveTheme(mode: ThemeMode): "dark" | "light" {
  if (mode === "auto") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
  return mode;
}

/** Apply theme: mutates `colors` export + sets document background. */
export function applyTheme(mode: ThemeMode) {
  const resolved = resolveTheme(mode);
  colors = resolved === "light" ? light : dark;
  // Sync document background to avoid flash
  const bg = colors.bg;
  document.documentElement.style.background = bg;
  document.body.style.background = bg;
}

export const fonts = {
  sans: "'Inter', 'Segoe UI', system-ui, sans-serif",
  mono: "'JetBrains Mono', 'Fira Code', monospace",
};
