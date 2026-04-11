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
    if (/^[0-9a-fA-F]{40,}$/.test(addr)) return 'Hex (raw)';
    if (/^[123mn]/.test(addr))     return 'P2PKH (Base58)';
    return 'Unknown';
  }

  function shortH(h) {
    return h ? (h.length > 20 ? h.slice(0, 20) + '…' : h) : '';
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
      const txid   = tx.txid || tx.tx_id || '';
      const height = tx.block_height ?? tx.height ?? null;
      const blkLnk = height !== null
        ? `<a href="/blockchain-rust/block/${height}" class="addr-link">#${height.toLocaleString()}</a>`
        : '—';
      return `<tr>
        <td><a href="/blockchain-rust/rx/${txid}" class="addr-link addr-mono">${shortH(txid)}</a></td>
        <td>${blkLnk}</td>
      </tr>`;
    }).join('');

    const moreNote = history.length > MAX_TXS
      ? `<p class="addr-note">Showing ${MAX_TXS} of ${history.length} transactions</p>`
      : '';

    return `
      <a class="detail-back" href="/blockchain-rust/">&#8592; Back</a>

      <div class="detail-title">
        Address
        <span class="hash">${addr}</span>
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
            <div class="kv-val">${addr}</div>
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
            <tr><th>TXID</th><th>Block</th></tr>
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
    const data = await fetchJson(`${API_BASE}/api/address/${enc}`);

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

  document.addEventListener('DOMContentLoaded', init);
})();
