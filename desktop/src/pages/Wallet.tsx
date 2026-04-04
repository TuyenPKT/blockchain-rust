import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { colors, fonts } from "../theme";
import { t } from "../i18n";
import { Panel } from "../components/Panel";
import { StatCard } from "../components/StatCard";
import { fetchBalance, fetchAddressUtxos, AddressUtxo, PACKETS_PER_PKT } from "../api";

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
  const [removing, setRemoving]   = useState(false);
  const [generating, setGenerating] = useState(false);
  const [isNew, setIsNew]         = useState(false);

  // Send form
  const [sendTo, setSendTo]       = useState("");
  const [sendAmt, setSendAmt]     = useState("");
  const [sendFee, setSendFee]     = useState("0.001");
  const [sending, setSending]     = useState(false);
  const [sendResult, setSendResult] = useState<{ ok: boolean; msg: string } | null>(null);

  // Fetch balance whenever wallet/network changes
  useEffect(() => {
    if (!wallet) { setBalance(null); return; }
    fetchBalance(nodeUrl, wallet.address)
      .then(data => {
        const d = data as Record<string, unknown>;
      const sat = (d["balance"] ?? d["confirmed"] ?? d["total"] ?? 0) as number;
        setBalance(Number(sat));
      })
      .catch(() => setBalance(null));
  }, [wallet, nodeUrl]);

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
        // refresh balance
        fetchBalance(nodeUrl, wallet.address).then(data => {
          const d = data as Record<string, unknown>;
          setBalance(Number((d["balance"] ?? d["confirmed"] ?? d["total"] ?? 0)));
        }).catch(() => {});
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

        {/* Import */}
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
