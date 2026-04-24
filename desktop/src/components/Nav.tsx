import { colors, fonts } from "../theme";

type Tab = "explorer" | "miner" | "node" | "wallet" | "richlist";

interface NavProps {
  tab:           Tab;
  onTab:         (t: Tab) => void;
  network:       "mainnet" | "testnet";
  onNetwork:     (n: "mainnet" | "testnet") => void;
  onSearchOpen:  () => void;
  onSettings:    () => void;
  settingsOpen:  boolean;
}

const SIDEBAR_W = 220;

const NAV_ITEMS: { id: Tab; label: string; icon: JSX.Element }[] = [
  {
    id: "explorer", label: "Explorer",
    icon: <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="10"/><polygon points="16.24 7.76 14.12 14.12 7.76 16.24 9.88 9.88 16.24 7.76"/></svg>,
  },
  {
    id: "miner", label: "Miner",
    icon: <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/></svg>,
  },
  {
    id: "node", label: "Node",
    icon: <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><rect x="2" y="2" width="20" height="8" rx="2"/><rect x="2" y="14" width="20" height="8" rx="2"/><line x1="6" y1="6" x2="6.01" y2="6"/><line x1="6" y1="18" x2="6.01" y2="18"/></svg>,
  },
  {
    id: "wallet", label: "Wallet",
    icon: <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M20 12V22H4V12"/><path d="M22 7H2v5h20V7z"/><path d="M12 22V7"/><path d="M12 7H7.5a2.5 2.5 0 0 1 0-5C11 2 12 7 12 7z"/><path d="M12 7h4.5a2.5 2.5 0 0 0 0-5C13 2 12 7 12 7z"/></svg>,
  },
];

export function Nav({ tab, onTab, network, onNetwork, onSettings, settingsOpen }: NavProps) {
  return (
    <aside style={{
      width: SIDEBAR_W, minWidth: SIDEBAR_W,
      background: colors.navBg,
      borderRight: `1px solid ${colors.border}`,
      display: "flex", flexDirection: "column",
      height: "100vh", position: "fixed", left: 0, top: 0, zIndex: 100,
    }}>
      {/* Logo */}
      <div style={{
        padding: "20px 20px 16px",
        borderBottom: `1px solid ${colors.border}`,
        display: "flex", alignItems: "center", gap: 10,
      }}>
        <img
          src="/logo.png" alt="Oceif Core"
          style={{ width: 36, height: 36, borderRadius: 10, objectFit: "cover", flexShrink: 0 }}
        />
        <span style={{ fontWeight: 700, fontSize: 16, color: colors.text, fontFamily: fonts.sans }}>
          Oceif Core
        </span>
      </div>

      {/* Navigation items */}
      <nav style={{ flex: 1, padding: "12px 12px 0", display: "flex", flexDirection: "column", gap: 2 }}>
        {NAV_ITEMS.map(item => {
          const active = tab === item.id;
          return (
            <button
              key={item.id}
              onClick={() => onTab(item.id)}
              style={{
                display: "flex", alignItems: "center", gap: 10,
                padding: "10px 12px", borderRadius: 8, border: "none", cursor: "pointer",
                background: active ? `${colors.accent}22` : "transparent",
                color: active ? colors.accent : colors.muted,
                fontFamily: fonts.sans, fontWeight: 600, fontSize: 14,
                textAlign: "left", width: "100%",
                transition: "all .15s",
                borderLeft: active ? `3px solid ${colors.accent}` : "3px solid transparent",
              }}
              onMouseEnter={e => { if (!active) e.currentTarget.style.background = `${colors.surface2}`; }}
              onMouseLeave={e => { if (!active) e.currentTarget.style.background = "transparent"; }}
            >
              {item.icon}
              {item.label}
            </button>
          );
        })}
      </nav>

      {/* Network selector */}
      <div style={{ padding: "16px 12px", borderTop: `1px solid ${colors.border}` }}>
        <div style={{ fontSize: 11, color: colors.muted, fontWeight: 600, letterSpacing: ".06em", textTransform: "uppercase", marginBottom: 8 }}>
          Network
        </div>
        <div style={{ display: "flex", gap: 6 }}>
          {(["mainnet", "testnet"] as const).map(n => {
            const active = network === n;
            return (
              <button key={n} onClick={() => onNetwork(n)} style={{
                flex: 1, padding: "6px 0", borderRadius: 6, border: "none", cursor: "pointer",
                fontFamily: fonts.sans, fontWeight: 700, fontSize: 12,
                background: active ? colors.accent : colors.surface2,
                color: active ? "#fff" : colors.muted,
                display: "flex", alignItems: "center", justifyContent: "center", gap: 5,
                transition: "all .15s",
              }}>
                <span style={{ width: 6, height: 6, borderRadius: "50%", background: active ? "#fff" : colors.muted, display: "inline-block" }} />
                {n === "mainnet" ? "Mainnet" : "Testnet"}
              </button>
            );
          })}
        </div>
      </div>

      {/* Settings */}
      <div style={{ padding: "0 12px 16px" }}>
        <button
          onClick={onSettings}
          style={{
            display: "flex", alignItems: "center", gap: 10,
            padding: "10px 12px", borderRadius: 8, border: "none", cursor: "pointer",
            background: settingsOpen ? `${colors.accent}22` : "transparent",
            color: settingsOpen ? colors.accent : colors.muted,
            fontFamily: fonts.sans, fontWeight: 600, fontSize: 14,
            width: "100%", textAlign: "left", transition: "all .15s",
          }}
          onMouseEnter={e => { if (!settingsOpen) e.currentTarget.style.background = colors.surface2; }}
          onMouseLeave={e => { if (!settingsOpen) e.currentTarget.style.background = "transparent"; }}
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <circle cx="12" cy="12" r="3"/>
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>
          </svg>
          Settings
        </button>
      </div>
    </aside>
  );
}

export { SIDEBAR_W };
