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
        ? `<a href="#block/${height}" class="pk-link">#${height.toLocaleString()}</a>` : '—';

      return `<tr>
        <td><a href="#tx/${txid}" class="pk-link pk-mono">${shortH(txid)}</a></td>
        <td>${blockLnk}</td>
      </tr>`;
    }).join('');

    const moreNote = history.length > MAX_TXS
      ? `<p class="pk-note">Hiển thị ${MAX_TXS}/${history.length} giao dịch gần nhất</p>`
      : '';

    return header('Address', truncAddr(addr)) +
      `<div class="pk-addr-hero">
        <div class="pk-addr-full pk-mono">${addr}</div>
        <div class="pk-addr-balance">${balPkt} <span class="pk-pkt-label">PKT</span></div>
        <div class="pk-addr-meta">${txCount} transaction${txCount !== 1 ? 's' : ''}</div>
      </div>
      <div class="pk-grid">
        ${fld('Balance',      `${balPkt} PKT`)}
        ${fld('TX Count',     txCount)}
        ${fld('Address Type', detectType(addr))}
      </div>` +
      (txsShow.length > 0
        ? `<h4 class="pk-section">Transaction History</h4>
           <table class="pk-table">
             <thead><tr><th>TXID</th><th>Block</th></tr></thead>
             <tbody>${txRows}</tbody>
           </table>
           ${moreNote}`
        : '<p class="pk-note">Chưa có giao dịch nào</p>');
  }

  // ── Helpers ────────────────────────────────────────────────────────────────

  function detectType(addr) {
    if (addr.startsWith('pkt1q'))  return 'P2WPKH (bech32)';
    if (addr.startsWith('tpkt1q')) return 'Testnet P2WPKH';
    if (addr.startsWith('rpkt1q')) return 'Regtest P2WPKH';
    if (addr.startsWith('pkt1p'))  return 'P2TR (taproot)';
    if (/^[0-9a-fA-F]{40,}$/.test(addr)) return 'Hex (raw)';
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
      const root = document.querySelector('main, .main-content, body');
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
    getPanel().innerHTML = `<div class="pk-loading">Loading address ${label}…</div>`;
  }

  function showError(msg) {
    getPanel().innerHTML = `<div class="pk-error">${msg}</div>`;
  }

  async function fetchJson(url) {
    try {
      const res = await fetch(url);
      if (!res.ok) return null;
      return await res.json();
    } catch (_) { return null; }
  }

  // ── HTML helpers ───────────────────────────────────────────────────────────

  function header(type, id) {
    return `<div class="pk-header">
      <button onclick="history.back()" class="pk-back">← Back</button>
      <span class="pk-type">${type}</span>
      <span class="pk-id">${id}</span>
    </div>`;
  }

  function fld(label, value) {
    return `<div class="pk-field">
      <span class="pk-label">${label}</span>
      <span class="pk-value">${value}</span>
    </div>`;
  }

  // ── Styles ─────────────────────────────────────────────────────────────────

  function injectStyles() {
    if (document.getElementById('pk-addr-css')) return;
    const s = document.createElement('style');
    s.id = 'pk-addr-css';
    s.textContent = `
      #pk-addr-panel { background:#1e293b; border-radius:10px; padding:1.5rem; margin-bottom:1.5rem; }
      .pk-header { display:flex; align-items:center; gap:1rem; margin-bottom:1.25rem; }
      .pk-back  { background:#334155; color:#94a3b8; border:none; border-radius:5px;
                  padding:.35rem .75rem; cursor:pointer; font-size:.85rem; }
      .pk-back:hover { background:#475569; }
      .pk-type  { font-size:.7rem; color:#64748b; text-transform:uppercase; letter-spacing:.08em; }
      .pk-id    { font-family:monospace; color:#94a3b8; font-size:.88rem; }

      .pk-addr-hero   { text-align:center; padding:1.5rem 0 1rem; }
      .pk-addr-full   { font-size:.8rem; color:#64748b; word-break:break-all; margin-bottom:.75rem; }
      .pk-addr-balance { font-size:2rem; font-weight:700; color:#4ade80; }
      .pk-pkt-label   { font-size:1rem; color:#94a3b8; font-weight:400; }
      .pk-addr-meta   { font-size:.8rem; color:#64748b; margin-top:.25rem; }

      .pk-grid  { display:grid; grid-template-columns:repeat(auto-fit,minmax(180px,1fr));
                  gap:.5rem 1.5rem; margin:1rem 0; }
      .pk-field { display:flex; flex-direction:column; }
      .pk-label { font-size:.7rem; color:#64748b; text-transform:uppercase; letter-spacing:.05em; }
      .pk-value { font-size:.88rem; color:#e2e8f0; margin-top:.1rem; }

      .pk-section { color:#94a3b8; font-size:.78rem; text-transform:uppercase;
                    letter-spacing:.08em; margin:1rem 0 .5rem; }
      .pk-table { width:100%; border-collapse:collapse; font-size:.85rem; }
      .pk-table th { color:#64748b; font-weight:500; text-align:left;
                     padding:.4rem .6rem; border-bottom:1px solid #334155; }
      .pk-table td { color:#cbd5e1; padding:.35rem .6rem; border-bottom:1px solid #0f172a; }
      .pk-num     { text-align:right; }
      .pk-mono    { font-family:monospace; }

      .pk-incoming { color:#4ade80; }
      .pk-outgoing { color:#f87171; }

      .pk-link  { color:#60a5fa; text-decoration:none; }
      .pk-link:hover { text-decoration:underline; }

      .pk-badge  { font-size:.7rem; padding:.1rem .45rem; border-radius:4px; font-weight:600; }
      .pk-green  { background:#166534; color:#4ade80; }
      .pk-red    { background:#7f1d1d; color:#f87171; }

      .pk-note    { color:#64748b; font-size:.8rem; margin-top:.75rem; }
      .pk-loading { color:#94a3b8; padding:1rem; font-style:italic; }
      .pk-error   { color:#f87171; padding:1rem; }
    `;
    document.head.appendChild(s);
  }

  // expose truncAddr cho external use (e.g. app.js links)
  window.pktTruncAddr = truncAddr;

})();
