import { colors, fonts } from "../theme";
import { t } from "../i18n";
import { SearchTrigger } from "./SearchBar";

type Tab = "explorer" | "blocks" | "charts" | "miner" | "node" | "wallet" | "richlist";

interface NavProps {
  tab:           Tab;
  onTab:         (t: Tab) => void;
  network:       "mainnet" | "testnet";
  onNetwork:     (n: "mainnet" | "testnet") => void;
  onSearchOpen:  () => void;
  onSettings:    () => void;
  settingsOpen:  boolean;
}

function getTabs(): { id: Tab; label: string }[] {
  return [
    { id: "explorer", label: t.tab_explorer },
    { id: "blocks",   label: t.tab_blocks   },
    { id: "charts",   label: t.tab_charts   },
    { id: "richlist", label: t.tab_richlist  },
    { id: "miner",  label: t.tab_miner  },
    { id: "node",   label: t.tab_node   },
    { id: "wallet", label: t.tab_wallet },
  ];
}

export function Nav({ tab, onTab, network, onNetwork, onSearchOpen, onSettings, settingsOpen }: NavProps) {
  return (
    <nav style={{
      background: colors.navBg,
      borderBottom: `1px solid ${colors.border}`,
      display: "flex", alignItems: "center",
      padding: "0 24px", height: 56, gap: 0,
      position: "fixed", top: 0, left: 0, right: 0, zIndex: 100,
    }}>
      {/* Logo */}
      <div style={{ display: "flex", alignItems: "center", gap: 10, marginRight: 32 }}>
        <img
          src="/logo.png"
          alt="OCEIF"
          style={{ width: 32, height: 32, borderRadius: 8, objectFit: "cover" }}
          onError={(e) => {
            const t = e.currentTarget;
            t.style.display = "none";
            const fb = document.createElement("div");
            fb.style.cssText = `width:32px;height:32px;border-radius:8px;background:linear-gradient(135deg,${colors.accent},#e07b10);display:flex;align-items:center;justify-content:center;font-weight:700;font-size:14px;color:#000;font-family:monospace`;
            fb.textContent = "P";
            t.parentNode?.insertBefore(fb, t);
          }}
        />
        <span style={{ fontWeight: 700, fontSize: 17, color: colors.text }}>
          PKT<span style={{ color: colors.accent }}>Scan</span>
        </span>
        <span style={{
          fontSize: 11, color: colors.muted, fontFamily: fonts.mono,
          background: colors.surface2, border: `1px solid ${colors.border}`,
          borderRadius: 4, padding: "2px 6px",
        }}>v20.3</span>
      </div>

      {/* Tabs */}
      <div style={{ display: "flex", flex: 1, gap: 2 }}>
        {getTabs().map(t => (
          <button key={t.id} onClick={() => onTab(t.id)} style={{
            background: "none", border: "none", cursor: "pointer",
            padding: "0 16px", height: 56,
            color: tab === t.id ? colors.accent : colors.muted,
            fontFamily: fonts.sans, fontWeight: 600, fontSize: 14,
            borderBottom: tab === t.id ? `2px solid ${colors.accent}` : "2px solid transparent",
            transition: "all .2s",
          }}>{t.label}</button>
        ))}
      </div>

      {/* Search trigger */}
      <SearchTrigger onClick={onSearchOpen} />

      {/* Settings button */}
      <button
        onClick={onSettings}
        title="Settings"
        style={{
          width: 36, height: 36, borderRadius: 8, border: "none",
          background: settingsOpen ? `${colors.accent}22` : "transparent",
          color: settingsOpen ? colors.accent : colors.muted,
          cursor: "pointer", display: "flex", alignItems: "center", justifyContent: "center",
          marginRight: 10, transition: "all .2s",
        }}
        onMouseEnter={e => { if (!settingsOpen) e.currentTarget.style.background = colors.surface2; }}
        onMouseLeave={e => { if (!settingsOpen) e.currentTarget.style.background = "transparent"; }}
      >
        <svg width="17" height="17" viewBox="0 0 24 24" fill="none"
          stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <circle cx="12" cy="12" r="3"/>
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>
        </svg>
      </button>

      {/* Network toggle */}
      <div style={{
        display: "flex", background: colors.surface2,
        border: `1px solid ${colors.border}`, borderRadius: 8, padding: 3, gap: 3,
      }}>
        {(["mainnet", "testnet"] as const).map(n => (
          <button key={n} onClick={() => onNetwork(n)} style={{
            padding: "5px 14px", borderRadius: 6, border: "none", cursor: "pointer",
            fontFamily: fonts.sans, fontWeight: 600, fontSize: 13,
            background: network === n ? (n === "mainnet" ? colors.blue : colors.accent) : "transparent",
            color: network === n ? "#000" : colors.muted,
            transition: "all .2s",
          }}>
            {n === "mainnet" ? "Mainnet" : "Testnet"}
          </button>
        ))}
      </div>
    </nav>
  );
}
