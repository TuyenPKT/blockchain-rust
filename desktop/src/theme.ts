// theme.ts — v21.1: dark + light + auto color tokens
import type { ThemeMode } from "./hooks/useSettings";

const dark = {
  bg:       "#0b0f1a",
  surface:  "#111827",
  surface2: "#1a2235",
  surface3: "#1e2d42",
  border:   "#1e3a5f",
  accent:   "#f7a133",
  blue:     "#3b9eff",
  green:    "#3ed56e",
  red:      "#f06060",
  purple:   "#b07ff0",
  text:     "#e6edf3",
  muted:    "#5a7a9b",
  navBg:    "#0d1829",
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
