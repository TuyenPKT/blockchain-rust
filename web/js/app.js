'use strict';

const API_BASE = '/blockchain-rust';

/* ── UTILS ──────────────────────────────────────────────────── */
function shortHash(h) { return h ? h.slice(0,10)+'…'+h.slice(-8) : '—'; }
function shortAddr(a) { return a ? a.slice(0,8)+'…'+a.slice(-6) : '—'; }
const MIN_VALID_TS = 1577836800; // 2020-01-01

function timeAgo(ts) {
  if (!ts || ts < MIN_VALID_TS) return '—';
  const secs = Math.max(0, Math.floor(Date.now() / 1000 - ts));
  if (secs < 10)       return 'just now';
  if (secs < 60)       return secs + ' secs ago';
  if (secs < 3600)     return Math.floor(secs/60) + ' mins ago';
  if (secs < 86400)    return Math.floor(secs/3600) + ' hrs ago';
  if (secs < 2592000)  return Math.floor(secs/86400) + ' days ago';
  return new Date(ts * 1000).toLocaleDateString('en-GB', { day: 'numeric', month: 'short', year: 'numeric' });
}
function pakletsToPkt(p) { return (p / 1e9).toFixed(4) + ' PKT'; }
function fmtHashrate(h) {
  if (h >= 1e15) return (h/1e15).toFixed(2) + ' PH/s';
  if (h >= 1e12) return (h/1e12).toFixed(2) + ' TH/s';
  if (h >= 1e9)  return (h/1e9).toFixed(2)  + ' GH/s';
  if (h >= 1e6)  return (h/1e6).toFixed(2)  + ' MH/s';
  if (h >= 1e3)  return (h/1e3).toFixed(2)  + ' KH/s';
  return h + ' H/s';
}
function addrLink(addr) {
  if (!addr || addr === '—' || addr === 'coinbase' || addr === 'unknown') return addr || '—';
  const enc = encodeURIComponent(addr);
  return `<a href="${API_BASE}/address/${enc}" style="color:var(--blue)">${addr}</a>`;
}

/* ── THEME ──────────────────────────────────────────────────── */
function toggleTheme() {
  const html = document.documentElement;
  const isLight = html.getAttribute('data-theme') === 'light';
  html.setAttribute('data-theme', isLight ? '' : 'light');
  document.getElementById('themeBtn').textContent = isLight ? '☀️' : '🌙';
  localStorage.setItem('pkt-theme', isLight ? '' : 'light');
}
(function initTheme() {
  const t = localStorage.getItem('pkt-theme') || '';
  document.documentElement.setAttribute('data-theme', t);
  const btn = document.getElementById('themeBtn');
  if (btn) btn.textContent = t === 'light' ? '🌙' : '☀️';
})();

/* ── TICKER ─────────────────────────────────────────────────── */
function buildTicker(s) {
  s = s || {};
  const height = (s.height ?? 0).toLocaleString("en-US");
  const hr     = fmtHashrate(s.hashrate ?? 0);
  const bt     = (s.avg_block_time_s ?? s.block_time_avg) ? Math.round(s.avg_block_time_s ?? s.block_time_avg) + 's' : '—';
  const items  = [
    `📦 Block #${height}`, `⚡ ${hr}`, `💰 ${s.block_reward_pkt != null ? s.block_reward_pkt.toLocaleString(undefined,{maximumFractionDigits:0})+' PKT reward' : '20 PKT reward'}`,
    `🔄 Difficulty ${s.difficulty ?? '—'}`, `⏱ ${bt} block time`,
    `🔐 BLAKE3 PoW`, `🛡 Post-Quantum ready`,
    `📦 Block #${height}`, `⚡ ${hr}`, `💰 ${s.block_reward_pkt != null ? s.block_reward_pkt.toLocaleString(undefined,{maximumFractionDigits:0})+' PKT reward' : '20 PKT reward'}`,
    `🔄 Difficulty ${s.difficulty ?? '—'}`, `⏱ ${bt} block time`,
    `🔐 BLAKE3 PoW`, `🛡 Post-Quantum ready`,
  ];
  const el = document.getElementById('tickerInner');
  if (el) el.innerHTML = items.map(t => `<span class="ticker-item">${t}<span class="ticker-sep"> ◆ </span></span>`).join('');
}

/* ── FETCH ──────────────────────────────────────────────────── */
async function fetchStats() {
  try {
    const r = await fetch(`${API_BASE}/api/testnet/summary`);
    if (!r.ok) return;
    const s = await r.json();
    const avg = s.avg_block_time_s ?? s.block_time_avg ?? 0;
    document.getElementById('stat-height').textContent    = (s.height ?? 0).toLocaleString("en-US");
    document.getElementById('stat-blocktime').textContent = avg ? Math.round(avg) + 's' : '—';
    document.getElementById('stat-hashrate').textContent  = fmtHashrate(s.hashrate ?? 0);
    document.getElementById('stat-nodes').textContent     = (s.utxo_count ?? 0).toLocaleString("en-US");
    document.getElementById('stat-txs').textContent       = (s.mempool_count ?? 0) + ' txs';
    document.getElementById('stat-diff').textContent      = s.difficulty ?? '—';
    if (s.block_reward_pkt != null) {
      const el = document.getElementById('stat-reward');
      if (el) el.textContent = s.block_reward_pkt.toLocaleString(undefined, {maximumFractionDigits: 0}) + ' PKT';
    }
    buildTicker(s);
    renderStats(s);
  } catch(e) { console.warn('fetchStats', e); }
}

async function fetchBlocks() {
  try {
    const r = await fetch(`${API_BASE}/api/testnet/headers?limit=8`);
    if (!r.ok) return;
    const data = await r.json();
    const blocks = data.headers ?? data.blocks ?? [];
    const el = document.getElementById('latestBlocks');
    el.innerHTML = blocks.length ? '' : '<div style="padding:18px;color:var(--muted);font-size:.85rem">No blocks yet</div>';
    blocks.forEach(b => {
      const div = document.createElement('div');
      div.className = 'list-item block-item';
      div.style.cursor = 'pointer';
      div.innerHTML = `
        <div class="item-icon item-icon-block">#${(b.height ?? 0) % 1000}</div>
        <div class="item-main">
          <div class="item-primary">#${(b.height ?? 0).toLocaleString("en-US")}</div>
          <div class="item-secondary mono" style="font-size:.78rem">${shortHash(b.hash ?? '')}</div>
        </div>
        <div class="item-right">
          <div class="item-age">${timeAgo(b.timestamp ?? 0)}</div>
        </div>`;
      div.onclick = () => { window.location.href = `${API_BASE}/block/${b.height}`; };
      el.appendChild(div);
    });
  } catch(e) { console.warn('fetchBlocks', e); }
}

async function fetchTxs() {
  try {
    const r = await fetch(`${API_BASE}/api/testnet/txs?limit=8`);
    if (!r.ok) return;
    const data = await r.json();
    const txs = data.txs ?? [];
    const el = document.getElementById('latestTxs');
    el.innerHTML = txs.length ? '' : '<div style="padding:18px;color:var(--muted);font-size:.85rem">No transactions yet</div>';
    txs.forEach(tx => {
      const div = document.createElement('div');
      div.className = 'list-item tx-item';
      div.style.cursor = 'pointer';
      div.innerHTML = `
        <div class="item-main">
          <div class="item-primary mono" style="font-size:.85rem">${shortHash(tx.txid ?? '')}</div>
          <div class="item-secondary">Block #${(tx.height ?? 0).toLocaleString("en-US")}</div>
        </div>
        <div class="item-right">
          <div class="item-age">${timeAgo(tx.timestamp ?? 0)}</div>
        </div>`;
      div.onclick = () => { window.location.href = `${API_BASE}/rx/${tx.txid}`; };
      el.appendChild(div);
    });
  } catch(e) { console.warn('fetchTxs', e); }
}

function renderStats(s) {
  const el = document.getElementById('statsContent');
  if (!el) return;
  const rows = [
    ['Network',        'PKT Chain'],
    ['Algorithm',      'BLAKE3 PoW'],
    ['Latest Block',   `#${(s.height ?? 0).toLocaleString("en-US")}`],
    ['Difficulty',     s.difficulty ?? '—'],
    ['Hashrate',       fmtHashrate(s.hashrate ?? 0)],
    ['Avg Block Time', (s.avg_block_time_s ?? s.block_time_avg) ? Math.round(s.avg_block_time_s ?? s.block_time_avg) + 's' : '—'],
    ['UTXO Count',     (s.utxo_count ?? 0).toLocaleString("en-US")],
    ['Mempool',        (s.mempool_count ?? 0) + ' txs'],
    ['Signature',      'ECDSA + Dilithium (hybrid post-quantum)'],
    ['Hash Function',  'BLAKE3 (PoW) · SHA-256 (address)'],
    ['Address Format', 'Base58Check (P2PKH / P2TR)'],
  ];
  el.innerHTML = rows.map(([k, v]) =>
    `<div class="kv-row"><div class="kv-key">${k}</div><div class="kv-val normal">${v}</div></div>`
  ).join('');
}

/* ── SEARCH ─────────────────────────────────────────────────── */
function openSearch() {
  document.getElementById('searchBackdrop').classList.add('open');
  setTimeout(() => document.getElementById('searchInput').focus(), 50);
}
function closeSearch(e) {
  if (!e || e.target === document.getElementById('searchBackdrop')) {
    document.getElementById('searchBackdrop').classList.remove('open');
    document.getElementById('searchInput').value = '';
    document.getElementById('searchResults').innerHTML =
      '<div class="search-empty">Type to search blocks, transactions, or addresses</div>';
  }
}
document.addEventListener('keydown', e => {
  if (e.key === 'Escape') closeSearch();
  if ((e.metaKey || e.ctrlKey) && e.key === 'k') { e.preventDefault(); openSearch(); }
});

let _searchTimer = null;
function doSearch(query) {
  const el = document.getElementById('searchResults');
  const q  = query.trim();
  if (!q) {
    el.innerHTML = '<div class="search-empty">Type to search blocks, transactions, or addresses</div>';
    clearTimeout(_searchTimer);
    return;
  }
  el.innerHTML = '<div class="search-empty">Searching…</div>';
  clearTimeout(_searchTimer);
  _searchTimer = setTimeout(() => _doSearchApi(q), 280);
}

async function _doSearchApi(q) {
  const el = document.getElementById('searchResults');
  try {
    const d = await fetch(`${API_BASE}/api/testnet/search?q=${encodeURIComponent(q)}`).then(r => r.json());
    const results = d.results || [];
    el._results = results;
    if (!results.length) { el.innerHTML = '<div class="search-empty">No results found</div>'; return; }
    const typeIcon = { block: '📦', tx: '💸', address: '👤', label: '🏷' };
    const typeCls  = { block: 'item-icon-block', tx: 'item-icon-tx', address: '', label: '' };
    el.innerHTML = results.map((r, i) => {
      const icon = typeIcon[r.type] || '🔍';
      const cls  = typeCls[r.type] || '';
      const sub  = r.type === 'block'   ? `Height ${r.value}` :
                   r.type === 'address' ? `${((r.meta?.balance_pkt)||0).toFixed(2)} PKT` :
                   r.type === 'label'   ? (r.meta?.category || '') :
                   r.meta?.in_mempool   ? 'mempool' : 'tx';
      return `<div class="search-result-item" onclick="selectResult(${i})">
        <div class="search-result-icon ${cls}" style="font-size:14px">${icon}</div>
        <div class="search-result-main">
          <div class="search-result-type">${r.label}</div>
          <div class="search-result-value">${r.value.length > 40 ? r.value.slice(0,18)+'…'+r.value.slice(-10) : r.value}${sub ? ' · '+sub : ''}</div>
        </div>
      </div>`;
    }).join('');
  } catch (_) {
    el.innerHTML = '<div class="search-empty">Search unavailable</div>';
  }
}

function selectResult(i) {
  const el      = document.getElementById('searchResults');
  const results = el._results;
  if (!results) return;
  const r = results[i];
  closeSearch();
  if (r.type === 'block') {
    window.location.href = `${API_BASE}/block/${parseInt(r.value)}`;
  } else if (r.type === 'tx') {
    window.location.href = `${API_BASE}/rx/${encodeURIComponent(r.value)}`;
  } else if (r.type === 'address' || r.type === 'label') {
    window.location.href = `${API_BASE}/address/${encodeURIComponent(r.value)}`;
  }
}

function heroSearch() {
  const q = document.getElementById('heroInput').value.trim();
  if (!q) return;
  document.getElementById('searchInput').value = q;
  openSearch();
  doSearch(q);
}

/* ── INIT ────────────────────────────────────────────────────── */
buildTicker({});
fetchStats();
fetchBlocks();
fetchTxs();
setInterval(() => { fetchStats(); fetchBlocks(); fetchTxs(); }, 15000);
