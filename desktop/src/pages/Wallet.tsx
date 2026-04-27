import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { colors, fonts } from "../theme";
import { t } from "../i18n";
import { Panel } from "../components/Panel";
import { StatCard } from "../components/StatCard";
import { fetchBalance, fetchAddressUtxos, fetchAddressTxs, AddressUtxo, AddressTx, PACKETS_PER_PKT, fmtPkt, timeAgo } from "../api";

interface WalletProps {
  nodeUrl: string;
  network: "mainnet" | "testnet";
  onViewAddr: (addr: string) => void;
}

interface WalletData {
  address:     string;
  pubkey_hex:  string;
  privkey_hex: string; // stored in localStorage, shown only on reveal
  watch_only?: boolean; // true = address-only import, cannot sign
}

const STORAGE_KEY = "pktscan_wallet";

function loadWallet(): WalletData | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? JSON.parse(raw) : null;
  } catch { return null; }
}

function saveWallet(w: WalletData) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(w));
}

function clearWallet() {
  localStorage.removeItem(STORAGE_KEY);
}

// ── Copy button ───────────────────────────────────────────────────────────────

function CopyBtn({ text, label }: { text: string; label: string }) {
  const [copied, setCopied] = useState(false);
  function copy() {
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1800);
    });
  }
  return (
    <button onClick={copy} style={{
      padding: "4px 12px", borderRadius: 6, border: `1px solid ${copied ? colors.green : colors.border}`,
      background: copied ? `${colors.green}18` : colors.surface2,
      color: copied ? colors.green : colors.muted,
      cursor: "pointer", fontSize: 12, fontWeight: 600, transition: "all .2s",
      fontFamily: fonts.sans,
    }}>
      {copied ? t.wallet_copied : label}
    </button>
  );
}

// ── Main ──────────────────────────────────────────────────────────────────────

export function Wallet({ nodeUrl, network, onViewAddr }: WalletProps) {
  const [wallet, setWallet]       = useState<WalletData | null>(loadWallet);
  const [balance, setBalance]     = useState<number | null>(null);
  const [revealKey, setRevealKey] = useState(false);
  const [importKey, setImportKey] = useState("");
  const [importErr, setImportErr] = useState("");
  const [removing, setRemoving]     = useState(false);
  const [generating, setGenerating] = useState(false);
  const [isNew, setIsNew]           = useState(false);
  const [mnemonic, setMnemonic]     = useState("");
  const [restoring, setRestoring]   = useState(false);
  const [restoreErr, setRestoreErr] = useState("");

  // Send form
  const [sendTo, setSendTo]       = useState("");
  const [sendAmt, setSendAmt]     = useState("");
  const [sendFee, setSendFee]     = useState("0.001");
  const [sending, setSending]     = useState(false);
  const [sendResult, setSendResult] = useState<{ ok: boolean; msg: string } | null>(null);

  // TX history
  const [txHistory, setTxHistory]   = useState<AddressTx[]>([]);
  const [txLoading, setTxLoading]   = useState(false);
  const [txPage, setTxPage]         = useState(0);
  const [txHasMore, setTxHasMore]   = useState(false);
  const TX_PAGE_SIZE = 20;

  const refreshTxHistory = useCallback(async (address: string, page = 0) => {
    setTxLoading(true);
    try {
      const data = await fetchAddressTxs(nodeUrl, address, page, TX_PAGE_SIZE);
      const rows = data.txs ?? [];
      setTxHistory(prev => page === 0 ? rows : [...prev, ...rows]);
      setTxHasMore(rows.length === TX_PAGE_SIZE);
      setTxPage(page);
    } catch { /* ignore */ }
    setTxLoading(false);
  }, [nodeUrl]);

  // Fetch balance whenever wallet/network changes
  useEffect(() => {
    if (!wallet) { setBalance(null); setTxHistory([]); return; }
    fetchBalance(nodeUrl, wallet.address)
      .then(data => {
        const d = data as Record<string, unknown>;
      const sat = (d["balance"] ?? d["confirmed"] ?? d["total"] ?? 0) as number;
        setBalance(Number(sat));
      })
      .catch(() => setBalance(null));
    refreshTxHistory(wallet.address, 0);
  }, [wallet, nodeUrl, refreshTxHistory]);

  async function handleGenerate() {
    setGenerating(true);
    setImportErr("");
    try {
      const result = await invoke<WalletData>("wallet_generate", { network });
      saveWallet(result);
      setWallet(result);
      setIsNew(true);
      setRevealKey(true);
    } catch (e) { setImportErr(String(e)); }
    setGenerating(false);
  }

  async function handleImport() {
    setImportErr("");
    const input = importKey.trim();

    // Watch-only: looks like a Base58 address (not 64-char hex privkey)
    const isAddress = input.length >= 25 && input.length <= 62
      && !/^[0-9a-fA-F]{64}$/.test(input);

    if (isAddress) {
      const w: WalletData = {
        address:    input,
        pubkey_hex: "",
        privkey_hex: "",
        watch_only: true,
      };
      saveWallet(w);
      setWallet(w);
      setImportKey("");
      setIsNew(false);
      return;
    }

    if (input.length !== 64 || !/^[0-9a-fA-F]+$/.test(input)) {
      setImportErr(t.wallet_invalid_key);
      return;
    }
    try {
      const result = await invoke<{ address: string; pubkey_hex: string }>(
        "wallet_from_privkey", { privkeyHex: input, network }
      );
      const w: WalletData = { ...result, privkey_hex: input };
      saveWallet(w);
      setWallet(w);
      setImportKey("");
      setIsNew(false);
    } catch (e) { setImportErr(String(e)); }
  }

  async function handleRestore() {
    setRestoreErr("");
    const words = mnemonic.trim().split(/\s+/);
    if (words.length !== 12 && words.length !== 24) {
      setRestoreErr(t.wallet_restore_err_words);
      return;
    }
    setRestoring(true);
    try {
      const result = await invoke<WalletData>("wallet_from_mnemonic", {
        mnemonic: words.join(" "),
        passphrase: "",
      });
      const w: WalletData = { ...result };
      saveWallet(w);
      setWallet(w);
      setMnemonic("");
      setIsNew(false);
    } catch (e) { setRestoreErr(String(e)); }
    setRestoring(false);
  }

  function handleRemove() {
    clearWallet();
    setWallet(null);
    setRevealKey(false);
    setRemoving(false);
    setIsNew(false);
    setBalance(null);
  }

  async function handleSend() {
    if (!wallet) return;
    setSending(true);
    setSendResult(null);
    try {
      const amtSat  = Math.round(parseFloat(sendAmt)  * PACKETS_PER_PKT);
      const feeSat  = Math.round(parseFloat(sendFee)  * PACKETS_PER_PKT);
      if (!amtSat || amtSat <= 0) { setSendResult({ ok: false, msg: "Amount không hợp lệ" }); setSending(false); return; }

      const utxoData = await fetchAddressUtxos(nodeUrl, wallet.address);
      if (utxoData.error) {
        setSendResult({ ok: false, msg: `${t.wallet_node_error}: ${utxoData.error}` });
        setSending(false);
        return;
      }
      const utxos: AddressUtxo[] = utxoData.utxos ?? [];
      if (!utxos.length) { setSendResult({ ok: false, msg: t.wallet_no_utxos }); setSending(false); return; }

      const need = amtSat + feeSat;
      const selected: AddressUtxo[] = [];
      let total = 0;
      for (const u of utxos) {
        selected.push(u);
        total += (u.amount ?? 0);
        if (total >= need) break;
      }
      if (total < need) { setSendResult({ ok: false, msg: t.wallet_insufficient }); setSending(false); return; }

      const inputs = selected.map(u => ({
        txid:          u.txid ?? "",
        vout:          u.vout ?? 0,
        value:         u.amount ?? 0,
        script_pubkey: u.script_pubkey as string ?? "",
      }));

      const built = await invoke<{ raw_hex: string; txid: string }>("wallet_tx_build", {
        privkeyHex:  wallet.privkey_hex,
        inputs,
        toAddr:      sendTo.trim(),
        amountSat:   amtSat,
        feeSat:      feeSat,
        changeAddr:  wallet.address,
        network,
      });

      const result = await invoke<{ txid?: string; error?: string }>("tx_broadcast", {
        nodeUrl,
        rawHex: built.raw_hex,
      });

      if (result.error) {
        setSendResult({ ok: false, msg: `${t.wallet_send_err} ${result.error}` });
      } else {
        setSendResult({ ok: true, msg: `${t.wallet_send_ok} ${result.txid ?? built.txid}` });
        setSendTo(""); setSendAmt(""); setSendFee("0.001");
        fetchBalance(nodeUrl, wallet.address).then(data => {
          const d = data as Record<string, unknown>;
          setBalance(Number((d["balance"] ?? d["confirmed"] ?? d["total"] ?? 0)));
        }).catch(() => {});
        refreshTxHistory(wallet.address, 0);
      }
    } catch (e) { setSendResult({ ok: false, msg: String(e) }); }
    setSending(false);
  }

  const pkt = balance !== null ? (balance / PACKETS_PER_PKT).toLocaleString(undefined, { maximumFractionDigits: 4 }) : "—";

  // ── No wallet ──

  if (!wallet) {
    return (
      <div style={{ maxWidth: 560, margin: "0 auto", display: "flex", flexDirection: "column", gap: 16 }}>
        <div style={{ marginBottom: 8 }}>
          <h2 style={{ margin: 0, fontSize: 22, fontWeight: 800, color: colors.text }}>{t.wallet_title}</h2>
          <p style={{ margin: "4px 0 0", fontSize: 13, color: colors.muted }}>{t.wallet_no_wallet}</p>
        </div>

        {/* Create */}
        <Panel icon="✨" title={t.wallet_create}>
          <div style={{ padding: "20px 24px", display: "flex", flexDirection: "column", gap: 14 }}>
            <div style={{ fontSize: 13, color: colors.muted }}>{t.wallet_warning}</div>
            {importErr && <div style={{ fontSize: 12, color: colors.red }}>{importErr}</div>}
            <button onClick={handleGenerate} disabled={generating} style={{
              padding: "13px 0", borderRadius: 10, border: "none",
              background: generating ? colors.surface2 : `linear-gradient(135deg, ${colors.accent}, #e07b10)`,
              color: generating ? colors.muted : "#000",
              fontWeight: 700, fontSize: 15, cursor: generating ? "wait" : "pointer",
              transition: "all .2s",
            }}>
              {generating ? t.wallet_generating : t.wallet_create}
            </button>
          </div>
        </Panel>

        {/* Import private key */}
        <Panel icon="🔑" title={t.wallet_import}>
          <div style={{ padding: "20px 24px", display: "flex", flexDirection: "column", gap: 12 }}>
            <input
              value={importKey}
              onChange={e => { setImportKey(e.target.value); setImportErr(""); }}
              placeholder={t.wallet_import_hint_full}
              style={{
                width: "100%", boxSizing: "border-box",
                background: colors.surface2, border: `1px solid ${importErr ? colors.red : colors.border}`,
                borderRadius: 8, padding: "10px 12px", color: colors.text,
                fontFamily: fonts.mono, fontSize: 12, outline: "none",
              }}
            />
            {importErr && <div style={{ fontSize: 12, color: colors.red }}>{importErr}</div>}
            <button onClick={handleImport} style={{
              padding: "11px 0", borderRadius: 10, border: "none",
              background: `${colors.blue}22`, color: colors.blue,
              fontWeight: 700, fontSize: 14, cursor: "pointer",
              outline: `1px solid ${colors.blue}44`, transition: "all .2s",
            }}>
              {t.wallet_import_btn}
            </button>
          </div>
        </Panel>

        {/* Restore from seed phrase */}
        <Panel icon="🌱" title={t.wallet_restore}>
          <div style={{ padding: "20px 24px", display: "flex", flexDirection: "column", gap: 12 }}>
            <textarea
              value={mnemonic}
              onChange={e => { setMnemonic(e.target.value); setRestoreErr(""); }}
              placeholder={t.wallet_restore_hint}
              rows={3}
              style={{
                width: "100%", boxSizing: "border-box", resize: "vertical",
                background: colors.surface2,
                border: `1px solid ${restoreErr ? colors.red : colors.border}`,
                borderRadius: 8, padding: "10px 12px", color: colors.text,
                fontFamily: fonts.mono, fontSize: 12, outline: "none",
                lineHeight: 1.6,
              }}
            />
            {restoreErr && <div style={{ fontSize: 12, color: colors.red }}>{restoreErr}</div>}
            <button onClick={handleRestore} disabled={restoring || !mnemonic.trim()} style={{
              padding: "11px 0", borderRadius: 10, border: "none",
              background: restoring ? colors.surface2 : `${colors.green}22`,
              color: restoring ? colors.muted : colors.green,
              fontWeight: 700, fontSize: 14,
              cursor: (restoring || !mnemonic.trim()) ? "not-allowed" : "pointer",
              outline: `1px solid ${colors.green}44`, transition: "all .2s",
            }}>
              {restoring ? t.wallet_restoring : t.wallet_restore_btn}
            </button>
          </div>
        </Panel>
      </div>
    );
  }

  // ── Wallet loaded ──

  const shortAddr = wallet.address.slice(0, 16) + "…" + wallet.address.slice(-8);

  return (
    <div style={{ maxWidth: 720, margin: "0 auto", display: "flex", flexDirection: "column", gap: 16 }}>

      {/* Stats */}
      <div style={{ display: "grid", gridTemplateColumns: "repeat(3,1fr)", gap: 12 }}>
        <StatCard icon="💼" label={t.wallet_title}
          value={shortAddr} color={colors.accent} />
        <StatCard icon="💰" label={t.wallet_balance}
          value={`${pkt} PKT`} color={colors.green} />
        <StatCard icon="🌐" label={t.wallet_network}
          value={network === "mainnet" ? "Mainnet" : "Testnet"} color={colors.blue} />
      </div>

      {/* Address panel */}
      <Panel icon="📬" title={t.wallet_address}
        right={
          <div style={{ display: "flex", gap: 8 }}>
            {isNew && (
              <span style={{
                fontSize: 10, padding: "2px 8px", borderRadius: 4, fontWeight: 700,
                background: `${colors.green}22`, color: colors.green,
                border: `1px solid ${colors.green}44`,
              }}>{t.wallet_new_badge}</span>
            )}
            {wallet.watch_only && (
              <span style={{
                fontSize: 10, padding: "2px 8px", borderRadius: 4, fontWeight: 700,
                background: `${colors.blue}22`, color: colors.blue,
                border: `1px solid ${colors.blue}44`,
              }}>{t.wallet_watch_only_badge}</span>
            )}
            <CopyBtn text={wallet.address} label={t.wallet_copy} />
            <button onClick={() => onViewAddr(wallet.address)} style={{
              padding: "4px 12px", borderRadius: 6, border: `1px solid ${colors.border}`,
              background: colors.surface2, color: colors.blue,
              cursor: "pointer", fontSize: 12, fontWeight: 600,
              fontFamily: fonts.sans,
            }}>
              {t.wallet_view_addr}
            </button>
          </div>
        }
      >
        <div style={{ padding: "16px 24px" }}>
          <div style={{
            fontFamily: fonts.mono, fontSize: 13, color: colors.text,
            background: colors.surface2, border: `1px solid ${colors.border}`,
            borderRadius: 8, padding: "12px 16px", wordBreak: "break-all", lineHeight: 1.7,
          }}>
            {wallet.address}
          </div>

          <div style={{ marginTop: 14, display: "flex", alignItems: "center",
            justifyContent: "space-between", padding: "10px 0",
            borderTop: `1px solid ${colors.border}` }}>
            <span style={{ fontSize: 13, color: colors.muted, fontWeight: 600 }}>{t.wallet_balance}</span>
            <span style={{ fontFamily: fonts.mono, fontSize: 18, fontWeight: 800, color: colors.green }}>
              {pkt} <span style={{ fontSize: 13, color: colors.muted }}>PKT</span>
            </span>
          </div>
        </div>
      </Panel>

      {/* Keys panel — hidden for watch-only wallets */}
      {!wallet.watch_only && <Panel icon="🔐" title="Keys">
        <div style={{ padding: "16px 24px", display: "flex", flexDirection: "column", gap: 14 }}>

          {/* Public key */}
          <div>
            <div style={{ fontSize: 12, color: colors.muted, marginBottom: 6, fontWeight: 600 }}>
              {t.wallet_pubkey}
            </div>
            <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
              <div style={{
                flex: 1, fontFamily: fonts.mono, fontSize: 11, color: colors.text,
                background: colors.surface2, border: `1px solid ${colors.border}`,
                borderRadius: 8, padding: "8px 12px",
                overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
              }}>
                {wallet.pubkey_hex}
              </div>
              <CopyBtn text={wallet.pubkey_hex} label={t.wallet_copy} />
            </div>
          </div>

          {/* Private key */}
          <div>
            <div style={{ fontSize: 12, color: colors.muted, marginBottom: 6, fontWeight: 600 }}>
              {t.wallet_privkey}
            </div>
            <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
              <div style={{
                flex: 1, fontFamily: fonts.mono, fontSize: 11,
                color: revealKey ? colors.red : colors.muted,
                background: colors.surface2, border: `1px solid ${revealKey ? colors.red + "44" : colors.border}`,
                borderRadius: 8, padding: "8px 12px",
                overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
              }}>
                {revealKey ? wallet.privkey_hex : "•".repeat(64)}
              </div>
              <button onClick={() => setRevealKey(v => !v)} style={{
                padding: "4px 12px", borderRadius: 6,
                border: `1px solid ${revealKey ? colors.red + "44" : colors.border}`,
                background: revealKey ? `${colors.red}12` : colors.surface2,
                color: revealKey ? colors.red : colors.muted,
                cursor: "pointer", fontSize: 12, fontWeight: 600,
                fontFamily: fonts.sans,
              }}>
                {revealKey ? t.wallet_hide : t.wallet_reveal}
              </button>
              {revealKey && <CopyBtn text={wallet.privkey_hex} label={t.wallet_copy} />}
            </div>
            {revealKey && (
              <div style={{
                marginTop: 8, fontSize: 12, color: colors.red,
                padding: "8px 12px", borderRadius: 8,
                background: `${colors.red}0c`, border: `1px solid ${colors.red}22`,
              }}>
                {t.wallet_warning}
              </div>
            )}
          </div>
        </div>
      </Panel>}

      {/* Send panel — hidden for watch-only wallets */}
      {wallet.watch_only && (
        <div style={{
          padding: "16px 24px", borderRadius: 12,
          background: `${colors.blue}0a`, border: `1px solid ${colors.blue}33`,
          fontSize: 13, color: colors.blue,
        }}>
          {t.wallet_watch_only_no_send}
        </div>
      )}
      {!wallet.watch_only && <>
      <Panel icon="➤" title={t.wallet_send}>
        <div style={{ padding: "16px 24px", display: "flex", flexDirection: "column", gap: 12 }}>
          <input
            value={sendTo}
            onChange={e => { setSendTo(e.target.value); setSendResult(null); }}
            placeholder={t.wallet_send_to}
            style={{
              width: "100%", boxSizing: "border-box",
              background: colors.surface2, border: `1px solid ${colors.border}`,
              borderRadius: 8, padding: "10px 12px", color: colors.text,
              fontFamily: fonts.mono, fontSize: 12, outline: "none",
            }}
          />
          <div style={{ display: "flex", gap: 10 }}>
            <input
              value={sendAmt}
              onChange={e => { setSendAmt(e.target.value); setSendResult(null); }}
              placeholder={t.wallet_send_amount}
              type="number" min="0" step="any"
              style={{
                flex: 2, boxSizing: "border-box",
                background: colors.surface2, border: `1px solid ${colors.border}`,
                borderRadius: 8, padding: "10px 12px", color: colors.text,
                fontFamily: fonts.mono, fontSize: 12, outline: "none",
              }}
            />
            <input
              value={sendFee}
              onChange={e => { setSendFee(e.target.value); setSendResult(null); }}
              placeholder={t.wallet_send_fee}
              type="number" min="0" step="any"
              style={{
                flex: 1, boxSizing: "border-box",
                background: colors.surface2, border: `1px solid ${colors.border}`,
                borderRadius: 8, padding: "10px 12px", color: colors.text,
                fontFamily: fonts.mono, fontSize: 12, outline: "none",
              }}
            />
          </div>
          {sendResult && (
            <div style={{
              fontSize: 12, padding: "8px 12px", borderRadius: 8,
              color:      sendResult.ok ? colors.green : colors.red,
              background: sendResult.ok ? `${colors.green}0c` : `${colors.red}0c`,
              border:     `1px solid ${sendResult.ok ? colors.green : colors.red}22`,
              wordBreak:  "break-all",
            }}>{sendResult.msg}</div>
          )}
          <button onClick={handleSend} disabled={sending || !sendTo || !sendAmt} style={{
            padding: "12px 0", borderRadius: 10, border: "none",
            background: sending ? colors.surface2 : `linear-gradient(135deg, ${colors.blue}, #5b4fcf)`,
            color: sending ? colors.muted : "#fff",
            fontWeight: 700, fontSize: 14, cursor: (sending || !sendTo || !sendAmt) ? "not-allowed" : "pointer",
            transition: "all .2s",
          }}>
            {sending ? t.wallet_sending : t.wallet_send_btn}
          </button>
        </div>
      </Panel>
      </>}

      {/* TX History */}
      <Panel icon="📋" title="Lịch sử giao dịch"
        right={
          <button onClick={() => wallet && refreshTxHistory(wallet.address, 0)}
            disabled={txLoading}
            style={{
              padding: "4px 12px", background: colors.surface2,
              border: `1px solid ${colors.border}`, borderRadius: 6,
              color: colors.muted, cursor: txLoading ? "wait" : "pointer", fontSize: 12,
            }}>
            {txLoading ? "…" : t.refresh}
          </button>
        }
      >
        <div style={{ overflowX: "auto" }}>
          {!txLoading && txHistory.length === 0 && (
            <div style={{ padding: "24px 20px", fontSize: 13, color: colors.muted, textAlign: "center" }}>
              Chưa có giao dịch nào
            </div>
          )}
          {txHistory.length > 0 && (
            <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 12 }}>
              <thead>
                <tr style={{ borderBottom: `1px solid ${colors.border}` }}>
                  {["Tx Hash", "Method", "Block", "Age", "From", "", "To", "Amount", "Txn Fee"].map(h => (
                    <th key={h} style={{
                      padding: "9px 12px", textAlign: "left",
                      fontSize: 10, fontWeight: 700, textTransform: "uppercase",
                      letterSpacing: ".07em", color: colors.muted, whiteSpace: "nowrap",
                    }}>{h}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {txHistory.map((tx, i) => {
                  const txid     = (tx.txid ?? tx.hash ?? "") as string;
                  const netSat   = (tx.net_sat ?? 0) as number;
                  const isRecv   = netSat > 0;
                  const isSent   = netSat < 0;
                  const amtStr   = netSat === 0
                    ? "—"
                    : `${isRecv ? "+" : ""}${(netSat / PACKETS_PER_PKT).toLocaleString(undefined, { maximumFractionDigits: 4 })} PKT`;
                  const amtColor = isRecv ? colors.green : isSent ? colors.red : colors.muted;
                  const shortTx  = txid.length >= 14 ? txid.slice(0, 8) + "…" + txid.slice(-6) : txid;
                  const from     = (tx.from ?? "") as string;
                  const to       = (tx.to   ?? "") as string;
                  const feeSat   = (tx.fee_sat ?? 0) as number;
                  const ts       = (tx.timestamp ?? 0) as number;
                  const height   = tx.height as number | undefined;
                  const isCoinbase = !from;
                  const isSelf   = from && to && from === to;
                  const method   = isCoinbase ? "Coinbase" : isSelf ? "Transfer*" : "Transfer";
                  const shortAddr = (a: string) => a.length >= 12 ? a.slice(0, 8) + "…" + a.slice(-4) : a || "—";
                  return (
                    <tr key={i} style={{ borderBottom: `1px solid ${colors.border}` }}>
                      {/* Tx Hash */}
                      <td style={{ padding: "9px 12px" }}>
                        <span title={txid} onClick={() => txid && onViewAddr(txid)} style={{
                          fontFamily: fonts.mono, fontSize: 11, color: colors.blue,
                          cursor: txid ? "pointer" : "default",
                          textDecoration: "underline", textDecorationColor: `${colors.blue}55`,
                        }}>
                          {shortTx || "—"}
                        </span>
                      </td>
                      {/* Method */}
                      <td style={{ padding: "9px 12px" }}>
                        <span style={{
                          fontSize: 10, fontWeight: 600, padding: "2px 10px", borderRadius: 4,
                          border: `1px solid ${colors.border}`,
                          background: colors.surface2, color: colors.text,
                        }}>
                          {method}
                        </span>
                      </td>
                      {/* Block */}
                      <td style={{ padding: "9px 12px" }}>
                        <span onClick={() => height !== undefined && onViewAddr(String(height))}
                          style={{ fontFamily: fonts.mono, fontSize: 11, color: colors.accent, cursor: "pointer" }}>
                          {height !== undefined ? height.toLocaleString() : "—"}
                        </span>
                      </td>
                      {/* Age */}
                      <td style={{ padding: "9px 12px", color: colors.muted, fontSize: 11, whiteSpace: "nowrap" }}>
                        {ts > 0 ? timeAgo(ts) : "—"}
                      </td>
                      {/* From */}
                      <td style={{ padding: "9px 12px" }}>
                        <span title={from} onClick={() => from && onViewAddr(from)} style={{
                          fontFamily: fonts.mono, fontSize: 11,
                          color: from ? colors.blue : colors.muted,
                          cursor: from ? "pointer" : "default",
                        }}>
                          {shortAddr(from)}
                        </span>
                      </td>
                      {/* Arrow */}
                      <td style={{ padding: "9px 4px" }}>
                        <span style={{
                          display: "inline-flex", alignItems: "center", justifyContent: "center",
                          width: 20, height: 20, borderRadius: "50%",
                          background: `${colors.green}20`, color: colors.green, fontSize: 10,
                        }}>→</span>
                      </td>
                      {/* To */}
                      <td style={{ padding: "9px 12px" }}>
                        <span title={to} onClick={() => to && onViewAddr(to)} style={{
                          fontFamily: fonts.mono, fontSize: 11,
                          color: to ? colors.blue : colors.muted,
                          cursor: to ? "pointer" : "default",
                        }}>
                          {isSelf ? <span style={{
                            fontSize: 10, padding: "1px 6px", borderRadius: 4,
                            border: `1px solid ${colors.border}`, color: colors.muted,
                          }}>SELF</span> : shortAddr(to)}
                        </span>
                      </td>
                      {/* Amount */}
                      <td style={{ padding: "9px 12px" }}>
                        <span style={{ fontFamily: fonts.mono, fontSize: 11, fontWeight: 600, color: amtColor }}>
                          {amtStr}
                        </span>
                      </td>
                      {/* Txn Fee */}
                      <td style={{ padding: "9px 12px", fontFamily: fonts.mono, fontSize: 11, color: colors.muted }}>
                        {feeSat > 0 ? fmtPkt(feeSat) : "—"}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
          {txHasMore && (
            <div style={{ padding: "12px 18px", borderTop: `1px solid ${colors.border}` }}>
              <button
                onClick={() => wallet && refreshTxHistory(wallet.address, txPage + 1)}
                disabled={txLoading}
                style={{
                  width: "100%", padding: "8px 0", borderRadius: 8, cursor: "pointer",
                  background: colors.surface2, border: `1px solid ${colors.border}`,
                  color: colors.muted, fontSize: 12, fontWeight: 600,
                }}
              >
                {txLoading ? "Đang tải…" : "Xem thêm"}
              </button>
            </div>
          )}
        </div>
      </Panel>

      {/* Danger zone */}
      <div style={{
        background: colors.surface, border: `1px solid ${colors.border}`,
        borderRadius: 14, overflow: "hidden",
      }}>
        <div style={{ padding: "14px 24px", display: "flex", alignItems: "center", justifyContent: "space-between" }}>
          <div>
            <div style={{ fontSize: 14, fontWeight: 600, color: colors.text }}>{t.wallet_remove}</div>
            <div style={{ fontSize: 12, color: colors.muted, marginTop: 2 }}>
              {t.wallet_warning}
            </div>
          </div>
          {!removing ? (
            <button onClick={() => setRemoving(true)} style={{
              padding: "7px 18px", borderRadius: 8, cursor: "pointer",
              border: `1px solid ${colors.red}55`, background: `${colors.red}10`,
              color: colors.red, fontFamily: fonts.sans, fontSize: 13, fontWeight: 600,
            }}>{t.wallet_remove}</button>
          ) : (
            <div style={{ display: "flex", gap: 8 }}>
              <button onClick={() => setRemoving(false)} style={{
                padding: "7px 14px", borderRadius: 8, cursor: "pointer",
                border: `1px solid ${colors.border}`, background: colors.surface2,
                color: colors.muted, fontFamily: fonts.sans, fontSize: 13,
              }}>{t.cancel}</button>
              <button onClick={handleRemove} style={{
                padding: "7px 18px", borderRadius: 8, cursor: "pointer",
                border: "none", background: colors.red,
                color: "#fff", fontFamily: fonts.sans, fontSize: 13, fontWeight: 700,
              }}>{t.wallet_remove_confirm}</button>
            </div>
          )}
        </div>
      </div>

    </div>
  );
}
