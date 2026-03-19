// PKTScan — frontend app (v14.2)
// HTMX + vanilla JS, fetch từ /api/* endpoints

const API = '';   // same-origin

// ── Helpers ───────────────────────────────────────────────────────────────

const PAKLETS_PER_PKT = 1_073_741_824n;   // 2^30

function pakletsToPkt(paklets) {
  return (BigInt(paklets) * 100_000_000n / PAKLETS_PER_PKT / 1n).toString().replace(/(\d)(?=(\d{3})+$)/g, '$1,');
}

function shortHash(h, n = 12) {
  if (!h) return '—';
  return h.slice(0, n) + '…';
}

function timeAgo(ts) {
  const diff = Math.floor(Date.now() / 1000) - ts;
  if (diff < 60)  return diff + 's ago';
  if (diff < 3600) return Math.floor(diff / 60) + 'm ago';
  return Math.floor(diff / 3600) + 'h ago';
}

function el(id) { return document.getElementById(id); }

function setText(id, text) {
  const e = el(id);
  if (e) e.textContent = text;
}

function setHtml(id, html) {
  const e = el(id);
  if (e) e.innerHTML = html;
}

// ── Status badge ──────────────────────────────────────────────────────────

function statusBadge(ok) {
  const cls = ok ? 'badge-green' : 'badge-red';
  const txt = ok ? '● online' : '○ offline';
  return `<span class="${cls}">${txt}</span>`;
}

// ── Fetch node status ─────────────────────────────────────────────────────

async function fetchStatus() {
  try {
    const r = await fetch(`${API}/api/status`);
    if (!r.ok) throw new Error(r.status);
    const d = await r.json();
    setText('stat-height',   d.height  ?? '—');
    setText('stat-peers',    d.peers   ?? '—');
    setText('stat-mempool',  d.mempool ?? '—');
    setText('stat-network',  d.network ?? '—');
    setHtml('stat-status',   statusBadge(true));
  } catch {
    setHtml('stat-status', statusBadge(false));
  }
}

// ── Fetch recent blocks ───────────────────────────────────────────────────

async function fetchBlocks() {
  try {
    const r = await fetch(`${API}/api/chain?limit=10`);
    if (!r.ok) throw new Error(r.status);
    const blocks = await r.json();
    const rows = blocks.map(b => `
      <tr>
        <td><a class="hash-link" href="#block/${b.height}">${b.height}</a></td>
        <td class="mono muted">${shortHash(b.hash)}</td>
        <td>${b.tx_count ?? 0}</td>
        <td class="muted">${b.timestamp ? timeAgo(b.timestamp) : '—'}</td>
      </tr>`).join('');
    setHtml('block-table-body', rows || '<tr><td colspan="4" class="muted center">No blocks</td></tr>');
  } catch {
    setHtml('block-table-body', '<tr><td colspan="4" class="error">Failed to load blocks</td></tr>');
  }
}

// ── Balance lookup ────────────────────────────────────────────────────────

async function lookupBalance() {
  const addr = (el('balance-addr') || {}).value || '';
  if (!addr.trim()) return;
  const out = el('balance-result');
  if (out) out.textContent = 'Loading…';
  try {
    const r = await fetch(`${API}/api/balance/${encodeURIComponent(addr.trim())}`);
    if (!r.ok) throw new Error(r.status);
    const d = await r.json();
    if (out) out.textContent = `${d.balance ?? d.confirmed ?? 0} paklets`;
  } catch (e) {
    if (out) out.textContent = `Error: ${e.message}`;
  }
}

// ── Search ────────────────────────────────────────────────────────────────

async function search(query) {
  if (!query) return;
  // Try block height first
  if (/^\d+$/.test(query)) {
    window.location.hash = `#block/${query}`;
    return;
  }
  // Otherwise treat as address
  window.location.hash = `#address/${query}`;
}

function handleSearch(e) {
  if (e.key === 'Enter') search((el('search-input') || {}).value || '');
}

// ── Theme toggle ──────────────────────────────────────────────────────────

function toggleTheme() {
  const html = document.documentElement;
  const current = html.getAttribute('data-theme');
  const next = current === 'light' ? '' : 'light';
  html.setAttribute('data-theme', next);
  localStorage.setItem('pkt-theme', next);
}

function loadTheme() {
  const saved = localStorage.getItem('pkt-theme');
  if (saved) document.documentElement.setAttribute('data-theme', saved);
}

// ── Init ──────────────────────────────────────────────────────────────────

function init() {
  loadTheme();
  fetchStatus();
  fetchBlocks();

  // Auto-refresh every 30s
  setInterval(() => { fetchStatus(); fetchBlocks(); }, 30_000);

  // Search input
  const si = el('search-input');
  if (si) si.addEventListener('keydown', handleSearch);

  // Balance lookup button
  const btn = el('balance-btn');
  if (btn) btn.addEventListener('click', lookupBalance);

  // Theme toggle
  const th = el('theme-toggle');
  if (th) th.addEventListener('click', toggleTheme);
}

document.addEventListener('DOMContentLoaded', init);
