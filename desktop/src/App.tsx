// App.tsx — Oceif Core v23.x
import { useState, useCallback, useEffect, type CSSProperties } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { colors, applyTheme } from "./theme";
import { setLang } from "./i18n";
import { useSettings } from "./hooks/useSettings";
import { Nav, SIDEBAR_W } from "./components/Nav";
import { SearchBar, SearchTrigger, SearchBarProps } from "./components/SearchBar";
import { Explorer, type ExplorerSubTab } from "./pages/Explorer";
import { Miner }       from "./pages/Miner";
import { Node }        from "./pages/Node";
import { Wallet }      from "./pages/Wallet";
import { Address }     from "./pages/Address";
import { BlockDetail } from "./pages/BlockDetail";
import { TxDetail }    from "./pages/TxDetail";
import { RichList }    from "./pages/RichList";
import { Settings }    from "./pages/Settings";

// Detail tabs are hidden from Nav
type Tab    = "explorer" | "richlist" | "miner" | "node" | "wallet"
            | "address" | "block-detail" | "tx-detail";
type NavTab = "explorer" | "richlist" | "miner" | "node" | "wallet";
type Network = "mainnet" | "testnet";

function toNavTab(tab: Tab): NavTab {
  if (tab === "address" || tab === "block-detail" || tab === "tx-detail") return "explorer";
  return tab as NavTab;
}

export default function App() {
  const { settings, update: updateSettings, reset: resetSettings } = useSettings();
  const [tab,             setTab]             = useState<Tab>("explorer");
  const [network,         setNetwork]         = useState<Network>("testnet");
  const [settingsOpen,    setSettingsOpen]    = useState(false);
  const [selectedAddress, setSelectedAddress] = useState("");
  const [selectedBlock,   setSelectedBlock]   = useState(0);
  const [selectedTxid,    setSelectedTxid]    = useState("");
  const [blockBackTab,    setBlockBackTab]    = useState<Tab>("explorer");
  const [txBackTab,       setTxBackTab]       = useState<Tab>("explorer");
  const [explorerSubTab,  setExplorerSubTab]  = useState<ExplorerSubTab>("overview");
  const [, setThemeKey] = useState(0);

  applyTheme(settings.theme);
  setLang(settings.language);

  useEffect(() => {
    if (settings.theme !== "auto") return;
    let unlisten: (() => void) | undefined;
    getCurrentWindow().onThemeChanged(() => {
      setThemeKey(k => k + 1);
    }).then(fn => { unlisten = fn; }).catch(() => {
      const mq = window.matchMedia("(prefers-color-scheme: dark)");
      const handler = () => setThemeKey(k => k + 1);
      mq.addEventListener("change", handler);
      unlisten = () => mq.removeEventListener("change", handler);
    });
    return () => unlisten?.();
  }, [settings.theme]);

  const nodeUrl = network === "mainnet" ? settings.nodeUrlMainnet : settings.nodeUrlTestnet;

  const goBlock = useCallback((height: number, backTab: Tab = "explorer") => {
    setSelectedBlock(height);
    setBlockBackTab(backTab);
    setTab("block-detail");
    setSettingsOpen(false);
  }, []);

  const goTx = useCallback((txid: string, backTab: Tab = "block-detail") => {
    setSelectedTxid(txid);
    setTxBackTab(backTab);
    setTab("tx-detail");
    setSettingsOpen(false);
  }, []);

  const goAddr = useCallback((addr: string) => {
    setSelectedAddress(addr);
    setTab("address");
    setSettingsOpen(false);
  }, []);

  const handleNavigate = useCallback<SearchBarProps["onNavigate"]>((newTab, meta) => {
    setSettingsOpen(false);
    if (newTab === "address" && meta?.label) {
      goAddr(meta.label);
    } else if (newTab === "blocks" && meta?.type === "block" && meta.raw) {
      const r = meta.raw as Record<string, unknown>;
      const h = (r["height"] ?? r["index"]) as number | undefined;
      if (h !== undefined) goBlock(h, "explorer");
      else { setTab("explorer"); setExplorerSubTab("blocks"); }
    } else if (newTab === "blocks") {
      setTab("explorer");
      setExplorerSubTab("blocks");
    } else if (newTab === "charts") {
      setTab("explorer");
      setExplorerSubTab("charts");
    } else if (newTab === "explorer" && meta?.type === "tx" && meta.label) {
      goTx(meta.label, "explorer");
    } else {
      setTab(newTab as Tab);
    }
  }, [goAddr, goBlock, goTx]);

  const handleSearchOpen = useCallback(() => {
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
  }, []);

  const PAGE_TITLES: Record<Tab, { title: string; sub: string }> = {
    explorer:     { title: "Explorer",    sub: "Real-time overview of the Oceif Core blockchain" },
    richlist:     { title: "Rich List",   sub: "Top PKT addresses by balance" },
    miner:        { title: "Miner",       sub: "CPU mining control and statistics" },
    node:         { title: "Node",        sub: "Full node status and peer connections" },
    wallet:       { title: "Wallet",      sub: "Send and receive PKT" },
    address:      { title: "Address",     sub: selectedAddress },
    "block-detail": { title: "Block",     sub: `#${selectedBlock}` },
    "tx-detail":  { title: "Transaction", sub: selectedTxid.slice(0, 16) + "…" },
  };
  const { title: pageTitle, sub: pageSub } = PAGE_TITLES[tab] ?? { title: "", sub: "" };

  return (
    <div style={{
      background: colors.bg, height: "100vh", color: colors.text,
      fontFamily: "'Inter', 'Segoe UI', system-ui, sans-serif",
      display: "flex", overflow: "hidden",
    }}>
      <SearchBar nodeUrl={nodeUrl} onNavigate={handleNavigate} />
      {/* Sidebar */}
      <Nav
        tab={toNavTab(tab)} onTab={t => { setTab(t as Tab); setSettingsOpen(false); }}
        network={network} onNetwork={setNetwork}
        onSearchOpen={handleSearchOpen}
        onSettings={() => setSettingsOpen(o => !o)}
        settingsOpen={settingsOpen}
      />

      {/* Main area */}
      <div style={{ marginLeft: SIDEBAR_W, flex: 1, display: "flex", flexDirection: "column", minWidth: 0, height: "100vh" }}>

        {/* Top header */}
        <header style={{
          height: 64, flexShrink: 0,
          borderBottom: `1px solid ${colors.border}`,
          display: "flex", alignItems: "center",
          padding: "0 24px", gap: 16,
          background: colors.surface,
        }}>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontWeight: 700, fontSize: 18, color: colors.text, lineHeight: 1.2 }}>{pageTitle}</div>
            <div style={{ fontSize: 12, color: colors.muted, marginTop: 2, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{pageSub}</div>
          </div>

          {/* Search trigger */}
          <div style={{ flex: "0 0 260px" }}>
            <SearchTrigger onClick={handleSearchOpen} />
          </div>

          {/* Action icons */}
          <div style={{ display: "flex", alignItems: "center", gap: 8, flexShrink: 0 }}>
            {/* Theme toggle */}
            <button onClick={() => updateSettings({ theme: settings.theme === "dark" ? "light" : settings.theme === "light" ? "auto" : "dark" })}
              style={{ ...iconBtn }}>
              <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/>
              </svg>
            </button>
          </div>
        </header>

        {/* Scrollable content */}
        <main style={{ flex: 1, overflowY: "auto", padding: "24px" }}>
          {settingsOpen ? (
            <Settings settings={settings} onUpdate={updateSettings} onReset={resetSettings} />
          ) : (
            <>
              {tab === "explorer" && (
                <Explorer
                  nodeUrl={nodeUrl}
                  onBlock={h => goBlock(h, "explorer")}
                  onTx={txid => goTx(txid, "explorer")}
                  subTab={explorerSubTab}
                  onSubTab={setExplorerSubTab}
                />
              )}
              {tab === "richlist" && <RichList nodeUrl={nodeUrl} onAddr={goAddr} />}
              <div style={{ display: tab === "miner" ? "block" : "none" }}>
                <Miner nodeUrl={nodeUrl} />
              </div>
              {tab === "node"   && <Node   nodeUrl={nodeUrl} />}
              {tab === "wallet" && <Wallet nodeUrl={nodeUrl} network={network} onViewAddr={goAddr} />}
              {tab === "address" && (
                <Address nodeUrl={nodeUrl} address={selectedAddress} onBack={() => setTab("explorer")} />
              )}
              {tab === "block-detail" && (
                <BlockDetail nodeUrl={nodeUrl} height={selectedBlock}
                  onBack={() => setTab(blockBackTab)} onTx={txid => goTx(txid, "block-detail")} />
              )}
              {tab === "tx-detail" && (
                <TxDetail nodeUrl={nodeUrl} txid={selectedTxid}
                  onBack={() => setTab(txBackTab)} onAddr={goAddr} />
              )}
            </>
          )}
        </main>

        {/* Status bar */}
        <footer style={{
          height: 36, flexShrink: 0,
          borderTop: `1px solid ${colors.border}`,
          background: colors.surface,
          display: "flex", alignItems: "center",
          padding: "0 20px", gap: 20,
          fontSize: 12, color: colors.muted,
        }}>
          <span style={{ display: "flex", alignItems: "center", gap: 5 }}>
            <span style={{ width: 6, height: 6, borderRadius: "50%", background: colors.green, display: "inline-block" }} />
            Connected to {network === "mainnet" ? "Mainnet" : "Testnet"}
          </span>
          <span>Version v1.0.0</span>
        </footer>
      </div>
    </div>
  );
}

const iconBtn: CSSProperties = {
  width: 32, height: 32, borderRadius: 8, border: "none",
  background: "transparent", cursor: "pointer",
  display: "flex", alignItems: "center", justifyContent: "center",
  color: "#64748B",
};
