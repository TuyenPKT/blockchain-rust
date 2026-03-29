// App.tsx — PKTScan Desktop v21.1
import { useState, useCallback, useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { colors, applyTheme } from "./theme";
import { setLang } from "./i18n";
import { useSettings } from "./hooks/useSettings";
import { Nav }         from "./components/Nav";
import { SearchBar, SearchBarProps } from "./components/SearchBar";
import { Explorer }    from "./pages/Explorer";
import { Blocks }      from "./pages/Blocks";
import { Charts }      from "./pages/Charts";
import { Miner }       from "./pages/Miner";
import { Node }        from "./pages/Node";
import { Wallet }      from "./pages/Wallet";
import { Address }     from "./pages/Address";
import { BlockDetail } from "./pages/BlockDetail";
import { TxDetail }    from "./pages/TxDetail";
import { RichList }    from "./pages/RichList";
import { Settings }    from "./pages/Settings";

// Detail tabs are hidden from Nav
type Tab     = "explorer" | "blocks" | "charts" | "richlist" | "miner" | "node" | "wallet"
             | "address" | "block-detail" | "tx-detail";
type NavTab  = "explorer" | "blocks" | "charts" | "richlist" | "miner" | "node" | "wallet";
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
  const [blockBackTab,    setBlockBackTab]    = useState<Tab>("blocks");
  const [txBackTab,       setTxBackTab]       = useState<Tab>("blocks");
  const [, setThemeKey] = useState(0); // force re-render on system theme change

  // Apply theme + language synchronously each render (before children render)
  applyTheme(settings.theme);
  setLang(settings.language);

  // Auto theme: use Tauri native window event (reliable in WKWebView)
  useEffect(() => {
    if (settings.theme !== "auto") return;
    let unlisten: (() => void) | undefined;
    getCurrentWindow().onThemeChanged(() => {
      setThemeKey(k => k + 1);
    }).then(fn => { unlisten = fn; }).catch(() => {
      // Fallback: matchMedia for non-Tauri / older WebKit
      const mq = window.matchMedia("(prefers-color-scheme: dark)");
      const handler = () => setThemeKey(k => k + 1);
      mq.addEventListener("change", handler);
      unlisten = () => mq.removeEventListener("change", handler);
    });
    return () => unlisten?.();
  }, [settings.theme]);

  // NodeUrl comes from settings (not hard-coded)
  const nodeUrl = network === "mainnet" ? settings.nodeUrlMainnet : settings.nodeUrlTestnet;

  const goBlock = useCallback((height: number, backTab: Tab = "blocks") => {
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
      if (h !== undefined) goBlock(h, "blocks");
      else setTab("blocks");
    } else if (newTab === "explorer" && meta?.type === "tx" && meta.label) {
      goTx(meta.label, "explorer");
    } else {
      setTab(newTab as Tab);
    }
  }, [goAddr, goBlock, goTx]);

  const handleSearchOpen = useCallback(() => {
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
  }, []);

  const bg = colors.bg; // already resolved by applyTheme() above

  return (
    <div style={{
      background: bg, minHeight: "100vh", color: colors.text,
      fontFamily: "'Inter', 'Segoe UI', system-ui, sans-serif",
    }}>
      <Nav
        tab={toNavTab(tab)} onTab={t => { setTab(t); setSettingsOpen(false); }}
        network={network} onNetwork={setNetwork}
        onSearchOpen={handleSearchOpen}
        onSettings={() => setSettingsOpen(o => !o)}
        settingsOpen={settingsOpen}
      />

      <SearchBar nodeUrl={nodeUrl} onNavigate={handleNavigate} />

      <main style={{ padding: "72px 24px 24px" }}>
        {settingsOpen ? (
          <Settings settings={settings} onUpdate={updateSettings} onReset={resetSettings} />
        ) : (
          <>
            {tab === "explorer"     && <Explorer  nodeUrl={nodeUrl} />}
            {tab === "blocks"       && <Blocks    nodeUrl={nodeUrl} onBlock={h => goBlock(h, "blocks")} />}
            {tab === "charts"       && <Charts    nodeUrl={nodeUrl} />}
            {tab === "richlist"     && <RichList  nodeUrl={nodeUrl} onAddr={goAddr} />}
            <div style={{ display: tab === "miner" ? "block" : "none" }}>
              <Miner nodeUrl={nodeUrl} />
            </div>
            {tab === "node"         && <Node      nodeUrl={nodeUrl} />}
            {tab === "wallet"       && <Wallet    nodeUrl={nodeUrl} network={network} onViewAddr={goAddr} />}
            {tab === "address"      && (
              <Address
                nodeUrl={nodeUrl}
                address={selectedAddress}
                onBack={() => setTab("explorer")}
              />
            )}
            {tab === "block-detail" && (
              <BlockDetail
                nodeUrl={nodeUrl}
                height={selectedBlock}
                onBack={() => setTab(blockBackTab)}
                onTx={txid => goTx(txid, "block-detail")}
              />
            )}
            {tab === "tx-detail" && (
              <TxDetail
                nodeUrl={nodeUrl}
                txid={selectedTxid}
                onBack={() => setTab(txBackTab)}
                onAddr={goAddr}
              />
            )}
          </>
        )}
      </main>
    </div>
  );
}
