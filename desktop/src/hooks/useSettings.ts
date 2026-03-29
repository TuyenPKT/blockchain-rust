// useSettings.ts — v20.8: persistent app settings via localStorage
import { useState, useCallback } from "react";

const SETTINGS_KEY = "pktscan_settings";

export type Currency = "PKT" | "USD";
export type Language = "en" | "vi";
export type ThemeMode = "dark" | "light" | "auto";

export interface AppSettings {
  nodeUrlTestnet: string;
  nodeUrlMainnet: string;
  currency:       Currency;
  language:       Language;
  theme:          ThemeMode;
  pollInterval:   number;   // seconds, for live dashboard
}

export const DEFAULT_SETTINGS: AppSettings = {
  nodeUrlTestnet: "https://oceif.com/blockchain-rust",
  nodeUrlMainnet: "https://oceif.com/blockchain-rust",
  currency:       "PKT",
  language:       "en",
  theme:          "auto",
  pollInterval:   8,
};

function load(): AppSettings {
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    if (!raw) return { ...DEFAULT_SETTINGS };
    return { ...DEFAULT_SETTINGS, ...JSON.parse(raw) };
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

function save(s: AppSettings): void {
  localStorage.setItem(SETTINGS_KEY, JSON.stringify(s));
}

export interface UseSettingsReturn {
  settings: AppSettings;
  update:   (patch: Partial<AppSettings>) => void;
  reset:    () => void;
}

export function useSettings(): UseSettingsReturn {
  const [settings, setSettings] = useState<AppSettings>(() => load());

  const update = useCallback((patch: Partial<AppSettings>) => {
    setSettings(prev => {
      const next = { ...prev, ...patch };
      save(next);
      return next;
    });
  }, []);

  const reset = useCallback(() => {
    save({ ...DEFAULT_SETTINGS });
    setSettings({ ...DEFAULT_SETTINGS });
  }, []);

  return { settings, update, reset };
}
