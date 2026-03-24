// v14.7 — PKT Address Detail Page
// Hash-router: #addr/ADDRESS → balance + UTXO list + tx history
// Tự inject vào trang, không cần build step.

(function () {
  'use strict';

  const PAKLETS  = 1_073_741_824; // 2^30
  const MAX_TXS  = 50;            // giới hạn hiển thị

  // ── Hash-router ────────────────────────────────────────────────────────────

  function route(hash) {
    const m = hash.match(/^#addr\/(.+)$/);
    if (m) showAddress(m[1]);
    else   hidePanel();
  }

  window.addEventListener('hashchange', () => route(location.hash));
  if (location.hash) route(location.hash);

  // ── Base58Check decode (PKT/Bitcoin P2PKH) ────────────────────────────────

  const B58_ALPHA = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';

  function b58Decode(s) {
    var n = BigInt(0);
    for (var i = 0; i < s.length; i++) {
      var c = B58_ALPHA.indexOf(s[i]);
      if (c < 0) return null;
      n = n * BigInt(58) + BigInt(c);
    }
    var bytes = [];
    while (n > BigInt(0)) { bytes.unshift(Number(n & BigInt(0xff))); n >>= BigInt(8); }
    var leading = 0;
    for (var j = 0; j < s.length && s[j] === '1'; j++) leading++;
    while (leading--) bytes.unshift(0);
    return bytes;
  }

  function addrToScriptHex(addr) {
    addr = (addr || '').trim();
    if (!addr) return null;
    // Already hex script_pubkey
    if (/^[0-9a-fA-F]{40,}$/.test(addr)) return addr.toLowerCase();
    // Base58Check P2PKH
    if (/^[1mnpP]/.test(addr)) {
      var dec = b58Decode(addr);
      if (!dec || dec.length !== 25) return null;
      var hash = dec.slice(1, 21).map(function(b) { return ('0'+b.toString(16)).slice(-2); }).join('');
      return '76a914' + hash + '88ac';
    }
    return null;
  }

  // ── Address detail ─────────────────────────────────────────────────────────

  async function showAddress(addr) {
    showLoading(truncAddr(addr));

    // Use the dedicated Base58 address endpoint (backend converts → script key internally)
    var enc = encodeURIComponent(addr);
    const data = await fetchJson('api/testnet/addr/' + enc + '?limit=50');

    const balance   = data?.balance  ?? 0;
    const txHistory = data?.txs      ?? [];
    const txCount   = data?.count    ?? txHistory.length;

    getPanel().innerHTML = renderAddress(addr, balance, txCount, txHistory);
    injectStyles();
  }

  function renderAddress(addr, balance, txCount, history) {
    const balPkt  = (balance / PAKLETS).toFixed(4);
    const txsShow = history.slice(0, MAX_TXS);

    const txRows = txsShow.map(tx => {
      const txid     = tx.txid || tx.tx_id || '';
      const height   = tx.block_height ?? tx.height ?? null;
      const blockLnk = height !== null
        ? `<a href="#block/${height}" class="pka-link pka-mono">#${height.toLocaleString()}</a>` : '—';

      return `<tr>
        <td><a href="#tx/${txid}" class="pka-link pka-mono">${shortH(txid)}</a></td>
        <td>${blockLnk}</td>
      </tr>`;
    }).join('');

    const moreNote = history.length > MAX_TXS
      ? `<p class="pka-note">Showing ${MAX_TXS} of ${history.length} transactions</p>`
      : '';

    return `
      <div class="pka-back" onclick="history.back()">&#8592; Back</div>
      <div class="pka-title">
        Address <span class="pka-title-hash">${addr}</span>
      </div>
      <div class="pka-panel">
        <div class="pka-hero">
          <div class="pka-hero-label">Balance</div>
          <div class="pka-hero-amount">${balPkt} <span class="pka-pkt">PKT</span></div>
          <div class="pka-hero-sub">${txCount.toLocaleString()} transaction${txCount !== 1 ? 's' : ''}</div>
        </div>
        <div class="pka-kv">
          <div class="pka-kv-row">
            <span class="pka-kv-key">Address</span>
            <span class="pka-kv-val pka-mono">${addr}</span>
          </div>
          <div class="pka-kv-row">
            <span class="pka-kv-key">Balance</span>
            <span class="pka-kv-val pka-green">${balPkt} PKT</span>
          </div>
          <div class="pka-kv-row">
            <span class="pka-kv-key">Transactions</span>
            <span class="pka-kv-val">${txCount.toLocaleString()}</span>
          </div>
          <div class="pka-kv-row pka-kv-last">
            <span class="pka-kv-key">Address Type</span>
            <span class="pka-kv-val">${detectType(addr)}</span>
          </div>
        </div>
      </div>
      ${txsShow.length > 0 ? `
      <div class="pka-panel pka-panel-mt">
        <div class="pka-section-head">Transaction History</div>
        <table class="pka-table">
          <thead><tr><th>TXID</th><th>Block</th></tr></thead>
          <tbody>${txRows}</tbody>
        </table>
        ${moreNote}
      </div>` : `<div class="pka-panel pka-panel-mt"><p class="pka-note">No transactions found</p></div>`}
    `;
  }

  // ── Helpers ────────────────────────────────────────────────────────────────

  function detectType(addr) {
    if (addr.startsWith('pkt1q'))  return 'P2WPKH (bech32)';
    if (addr.startsWith('tpkt1q')) return 'Testnet P2WPKH';
    if (addr.startsWith('rpkt1q')) return 'Regtest P2WPKH';
    if (addr.startsWith('pkt1p'))  return 'P2TR (taproot)';
    if (/^[0-9a-fA-F]{40,}$/.test(addr)) return 'Hex (raw)';
    if (/^[123mn]/.test(addr))     return 'P2PKH (Base58)';
    return 'Unknown';
  }

  function shortH(h) {
    return h ? (h.length > 20 ? h.slice(0, 20) + '…' : h) : '';
  }

  function truncAddr(addr) {
    return addr.length > 24 ? addr.slice(0, 12) + '…' + addr.slice(-8) : addr;
  }

  function fmtTs(ts) {
    try { return new Date(ts * 1000).toISOString().replace('T', ' ').slice(0, 16) + ' UTC'; }
    catch (_) { return ''; }
  }

  // ── DOM helpers ────────────────────────────────────────────────────────────

  function getPanel() {
    let el = document.getElementById('pk-addr-panel');
    if (!el) {
      el = document.createElement('div');
      el.id = 'pk-addr-panel';
      const root = document.querySelector('.main-wrap, main, .main-content, body');
      root.prepend(el);
    }
    el.style.display = 'block';
    el.scrollIntoView({ behavior: 'smooth' });
    return el;
  }

  function hidePanel() {
    const el = document.getElementById('pk-addr-panel');
    if (el) el.style.display = 'none';
  }

  function showLoading(label) {
    injectStyles();
    getPanel().innerHTML = `<div class="pka-loading">Loading address ${label}…</div>`;
  }

  function showError(msg) {
    injectStyles();
    getPanel().innerHTML = `<div class="pka-error">${msg}</div>`;
  }

  async function fetchJson(url) {
    try {
      const res = await fetch(url);
      if (!res.ok) return null;
      return await res.json();
    } catch (_) { return null; }
  }

  // ── Styles ─────────────────────────────────────────────────────────────────

  function injectStyles() {
    if (document.getElementById('pk-addr-css')) return;
    const s = document.createElement('style');
    s.id = 'pk-addr-css';
    s.textContent = `
      #pk-addr-panel { margin-bottom:28px; }

      .pka-back {
        display:inline-flex; align-items:center; gap:6px;
        color:var(--muted); font-size:.83rem; font-weight:600;
        margin-bottom:20px; cursor:pointer;
        padding:6px 10px; border-radius:8px;
        transition:background .12s, color .12s;
      }
      .pka-back:hover { background:var(--surface2); color:var(--text); }

      .pka-title {
        font-size:1.3rem; font-weight:700;
        margin-bottom:20px;
        display:flex; align-items:center; gap:12px; flex-wrap:wrap;
      }
      .pka-title-hash {
        font-family:'JetBrains Mono',monospace;
        font-size:.82rem; font-weight:400;
        color:var(--muted); word-break:break-all;
      }

      .pka-panel {
        background:var(--surface);
        border:1px solid var(--border);
        border-radius:14px;
        overflow:hidden;
      }
      .pka-panel-mt { margin-top:20px; }

      .pka-hero {
        text-align:center;
        padding:2rem 1rem 1.5rem;
        border-bottom:1px solid var(--border);
        background:linear-gradient(135deg, rgba(247,161,51,.06) 0%, transparent 60%);
      }
      .pka-hero-label { font-size:.7rem; color:var(--muted); text-transform:uppercase; letter-spacing:.08em; font-weight:600; }
      .pka-hero-amount { font-size:2.2rem; font-weight:700; color:var(--green); margin:.4rem 0 .3rem; font-family:'JetBrains Mono',monospace; }
      .pka-pkt { font-size:1rem; color:var(--pkt); font-weight:600; }
      .pka-hero-sub { font-size:.8rem; color:var(--muted); }

      .pka-kv {}
      .pka-kv-row {
        display:grid; grid-template-columns:180px 1fr;
        gap:16px; padding:12px 18px;
        border-bottom:1px solid var(--border);
        align-items:start;
      }
      .pka-kv-last { border-bottom:none; }
      @media(max-width:600px){ .pka-kv-row{ grid-template-columns:1fr; gap:4px; } }
      .pka-kv-key { font-size:.8rem; color:var(--muted); font-weight:600; padding-top:2px; }
      .pka-kv-val { font-size:.85rem; word-break:break-all; }
      .pka-mono   { font-family:'JetBrains Mono',monospace; }
      .pka-green  { color:var(--green); font-family:'JetBrains Mono',monospace; }

      .pka-section-head {
        font-size:.78rem; font-weight:700; color:var(--muted);
        text-transform:uppercase; letter-spacing:.08em;
        padding:12px 18px;
        border-bottom:1px solid var(--border);
      }

      .pka-table { width:100%; border-collapse:collapse; font-size:.85rem; }
      .pka-table th {
        color:var(--muted); font-weight:600; text-align:left;
        padding:10px 18px; border-bottom:1px solid var(--border);
        font-size:.78rem; text-transform:uppercase; letter-spacing:.06em;
      }
      .pka-table td { padding:10px 18px; border-bottom:1px solid var(--border); color:var(--text); }
      .pka-table tr:last-child td { border-bottom:none; }
      .pka-table tr:hover td { background:var(--surface2); }

      .pka-link  { color:var(--blue); text-decoration:none; }
      .pka-link:hover { text-decoration:underline; }

      .pka-note    { color:var(--muted); font-size:.82rem; padding:16px 18px; }
      .pka-loading { color:var(--muted); padding:24px 18px; font-style:italic; font-size:.9rem; }
      .pka-error   { color:var(--red); padding:16px 18px; }
    `;
    document.head.appendChild(s);
  }

  // expose truncAddr cho external use (e.g. app.js links)
  window.pktTruncAddr = truncAddr;

})();
