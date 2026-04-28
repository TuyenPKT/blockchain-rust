// address-page.js — Standalone address detail page
// URL: /blockchain-rust/address/:address
// Reads address from pathname, fetches API, renders.

(function () {
  'use strict';

  const API_BASE  = '/blockchain-rust';
  const PAKLETS   = 1_073_741_824; // 2^30
  const MAX_TXS   = 50;

  // ── Theme ──────────────────────────────────────────────────────────────────

  function toggleTheme() {
    const html    = document.documentElement;
    const isLight = html.getAttribute('data-theme') === 'light';
    html.setAttribute('data-theme', isLight ? '' : 'light');
    document.getElementById('themeBtn').textContent = isLight ? '☀️' : '🌙';
    localStorage.setItem('pkt-theme', isLight ? '' : 'light');
  }
  window.toggleTheme = toggleTheme;

  (function initTheme() {
    const t   = localStorage.getItem('pkt-theme') || '';
    document.documentElement.setAttribute('data-theme', t);
    const btn = document.getElementById('themeBtn');
    if (btn) btn.textContent = t === 'light' ? '🌙' : '☀️';
  })();

  // ── Helpers ────────────────────────────────────────────────────────────────

  function detectType(addr) {
    if (addr.startsWith('pkt1q'))  return 'P2WPKH (bech32)';
    if (addr.startsWith('tpkt1q')) return 'Testnet P2WPKH';
    if (addr.startsWith('rpkt1q')) return 'Regtest P2WPKH';
    if (addr.startsWith('pkt1p'))  return 'P2TR (taproot)';
    if (/^0x[0-9a-fA-F]{40}$/.test(addr)) return 'EVM (EIP-55)';
    if (/^[0-9a-fA-F]{40,}$/.test(addr)) return 'Hex (raw)';
    if (/^[123mn]/.test(addr))     return 'P2PKH (Base58)';
    return 'Unknown';
  }

  function escHtml(s) {
    return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
  }

  function shortH(h) {
    return h ? (h.length > 14 ? h.slice(0, 8) + '…' + h.slice(-6) : h) : '';
  }

  function shortAddr(a) {
    return a && a.length >= 12 ? a.slice(0, 8) + '…' + a.slice(-4) : (a || '—');
  }

  function fmtPkt(sat) {
    if (!sat) return '—';
    const pkt = sat / PAKLETS;
    return pkt.toLocaleString(undefined, { maximumFractionDigits: pkt >= 1 ? 0 : 4 }) + ' PKT';
  }

  const MIN_VALID_TS = 1735689600; // 2025-01-01

  function timeAgo(ts) {
    if (!ts || ts < MIN_VALID_TS) return '—';
    const secs = Math.max(0, Math.floor((Date.now() / 1000) - ts));
    if (secs < 10)       return 'just now';
    if (secs < 60)       return secs + ' secs ago';
    if (secs < 3600)     return Math.floor(secs / 60) + ' mins ago';
    if (secs < 86400)    return Math.floor(secs / 3600) + ' hrs ago';
    if (secs < 2592000)  return Math.floor(secs / 86400) + ' days ago';
    return new Date(ts * 1000).toLocaleDateString('en-GB', { day: 'numeric', month: 'short', year: 'numeric' });
  }

  async function fetchJson(url) {
    try {
      const res = await fetch(url);
      if (!res.ok) return null;
      return await res.json();
    } catch (_) { return null; }
  }

  // ── Render ─────────────────────────────────────────────────────────────────

  function render(addr, balance, txCount, history) {
    const balPkt  = (balance / PAKLETS).toFixed(4);
    const txsShow = history.slice(0, MAX_TXS);

    const txRows = txsShow.map(tx => {
      const txid      = tx.txid || tx.tx_id || '';
      const netSat    = tx.net_sat ?? 0;
      const isRecv    = netSat > 0;
      const isSent    = netSat < 0;
      const amtStr    = netSat === 0 ? '—'
        : (isRecv ? '+' : '') + (netSat / PAKLETS).toLocaleString(undefined, { maximumFractionDigits: 4 }) + ' PKT';
      const amtColor  = isRecv ? 'var(--green)' : isSent ? 'var(--red)' : 'var(--muted)';
      const from      = tx.from || '';
      const to        = tx.to   || '';
      const isCoinbase = !from;
      const isSelf    = from && to && from === to;
      const method    = isCoinbase ? 'Coinbase' : isSelf ? 'Transfer*' : 'Transfer';
      const ts        = tx.timestamp ?? 0;
      const height    = tx.block_height ?? tx.height ?? null;
      const age       = ts > 0 ? timeAgo(ts) : '—';
      const feeSat    = tx.fee_sat ?? 0;
      const toCell    = isSelf
        ? `<span class="addr-badge badge-muted">SELF</span>`
        : `<span class="addr-mono addr-sm" title="${escHtml(to)}">${escHtml(shortAddr(to))}</span>`;
      return `<tr>
        <td class="addr-mono"><a href="/blockchain-rust/rx/${escHtml(txid)}" class="addr-link">${escHtml(shortH(txid)) || '—'}</a></td>
        <td><span class="addr-badge badge-method">${method}</span></td>
        <td class="addr-mono addr-sm">${height !== null ? `<a href="/blockchain-rust/block/${height}" class="addr-link">${height.toLocaleString()}</a>` : '—'}</td>
        <td class="addr-muted addr-sm">${age}</td>
        <td><span class="addr-mono addr-sm addr-link-soft" title="${escHtml(from)}">${escHtml(shortAddr(from))}</span></td>
        <td><span class="addr-arrow-circle">→</span></td>
        <td>${toCell}</td>
        <td class="addr-mono addr-sm" style="font-weight:600;color:${amtColor}">${amtStr}</td>
        <td class="addr-muted addr-sm">${fmtPkt(feeSat)}</td>
      </tr>`;
    }).join('');

    const moreNote = history.length > MAX_TXS
      ? `<p class="addr-note">Showing ${MAX_TXS} of ${history.length} transactions</p>`
      : '';

    return `
      <a class="detail-back" href="/blockchain-rust/">&#8592; Back</a>

      <div class="detail-title">
        Address
        <span class="hash">${escHtml(addr)}</span>
      </div>

      <div class="panel" style="margin-bottom:20px">
        <div class="addr-hero">
          <div class="addr-hero-label">Balance</div>
          <div class="addr-hero-amount">${balPkt} <span class="addr-pkt">PKT</span></div>
          <div class="addr-hero-sub">${txCount.toLocaleString()} transaction${txCount !== 1 ? 's' : ''}</div>
        </div>
        <div class="kv-table">
          <div class="kv-row">
            <div class="kv-key">Address</div>
            <div class="kv-val">${escHtml(addr)}</div>
          </div>
          <div class="kv-row">
            <div class="kv-key">Balance</div>
            <div class="kv-val" style="color:var(--green)">${balPkt} PKT</div>
          </div>
          <div class="kv-row">
            <div class="kv-key">Transactions</div>
            <div class="kv-val normal">${txCount.toLocaleString()}</div>
          </div>
          <div class="kv-row" style="border-bottom:none">
            <div class="kv-key">Address Type</div>
            <div class="kv-val normal">${detectType(addr)}</div>
          </div>
        </div>
      </div>

      ${txsShow.length > 0 ? `
      <div class="panel">
        <div class="panel-head">
          <span class="panel-title">Transaction History</span>
        </div>
        <table class="addr-table">
          <thead>
            <tr>
              <th>Transaction Hash</th><th>Method</th><th>Block</th><th>Age</th>
              <th>From</th><th></th><th>To</th>
              <th>Amount</th><th>Txn Fee</th>
            </tr>
          </thead>
          <tbody>${txRows}</tbody>
        </table>
        ${moreNote}
      </div>` : `
      <div class="panel">
        <p class="addr-note">No transactions found</p>
      </div>`}
    `;
  }

  function renderError(msg) {
    return `
      <a class="detail-back" href="/blockchain-rust/">&#8592; Back</a>
      <div class="panel" style="padding:24px;color:var(--red)">${msg}</div>
    `;
  }

  // ── Styles ─────────────────────────────────────────────────────────────────

  function injectStyles() {
    if (document.getElementById('addr-page-css')) return;
    const s = document.createElement('style');
    s.id = 'addr-page-css';
    s.textContent = `
      .addr-hero {
        text-align: center;
        padding: 2rem 1rem 1.5rem;
        border-bottom: 1px solid var(--border);
        background: linear-gradient(135deg, rgba(247,161,51,.06) 0%, transparent 60%);
      }
      .addr-hero-label { font-size:.7rem; color:var(--muted); text-transform:uppercase; letter-spacing:.08em; font-weight:600; }
      .addr-hero-amount { font-size:2.2rem; font-weight:700; color:var(--green); margin:.4rem 0 .3rem; font-family:'JetBrains Mono',monospace; }
      .addr-pkt { font-size:1rem; color:var(--pkt); font-weight:600; }
      .addr-hero-sub { font-size:.8rem; color:var(--muted); }

      .addr-table { width:100%; border-collapse:collapse; font-size:.85rem; }
      .addr-table th {
        color:var(--muted); font-weight:600; text-align:left;
        padding:10px 18px; border-bottom:1px solid var(--border);
        font-size:.78rem; text-transform:uppercase; letter-spacing:.06em;
      }
      .addr-table td { padding:10px 18px; border-bottom:1px solid var(--border); color:var(--text); }
      .addr-table tr:last-child td { border-bottom:none; }
      .addr-table tr:hover td { background:var(--surface2); }

      .addr-link { color:var(--blue); text-decoration:none; }
      .addr-link:hover { text-decoration:underline; }
      .addr-mono { font-family:'JetBrains Mono',monospace; }
      .addr-note { color:var(--muted); font-size:.82rem; padding:16px 18px; }
      .addr-sm   { font-size:.78rem; }
      .addr-muted { color:var(--muted); font-size:.8rem; }
      .addr-arrow { color:var(--muted); font-size:.8rem; padding:10px 4px; }
      .addr-badge    { font-size:.7rem; font-weight:600; padding:2px 10px; border-radius:4px; border:1px solid var(--border); background:var(--surface2); color:var(--text); }
      .badge-method  { background:var(--surface2); color:var(--text); }
      .badge-muted   { background:transparent; color:var(--muted); border-color:var(--border); }
      .addr-link-soft{ color:var(--blue); }
      .addr-arrow-circle {
        display:inline-flex; align-items:center; justify-content:center;
        width:18px; height:18px; border-radius:50%;
        background:rgba(80,200,120,.15); color:var(--green); font-size:.7rem;
      }
    `;
    document.head.appendChild(s);
  }

  // ── Init ───────────────────────────────────────────────────────────────────

  async function init() {
    injectStyles();

    // Extract address from URL: /blockchain-rust/address/<addr>
    const parts = window.location.pathname.split('/');
    const addr  = decodeURIComponent(parts[parts.length - 1] || '');

    if (!addr) {
      document.getElementById('addr-content').innerHTML = renderError('No address specified.');
      return;
    }

    document.title = `${addr.slice(0, 12)}… — PKTScan`;

    const enc  = encodeURIComponent(addr);
    const data = await fetchJson(`${API_BASE}/api/testnet/addr/${enc}?limit=50`);

    if (!data) {
      document.getElementById('addr-content').innerHTML = renderError('Address not found or API unavailable.');
      return;
    }

    const balance   = data.balance  ?? 0;
    const txHistory = data.txs      ?? [];
    const txCount   = data.count    ?? txHistory.length;

    document.getElementById('addr-content').innerHTML = render(addr, balance, txCount, txHistory);
  }

  window.openSearch = function () {};  // stub — search modal không có trên trang này

  init();
})();
