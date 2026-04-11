/* ── CONFIG ─────────────────────────────────────────────────── */
const TESTNET_API = '/blockchain-rust/testnet';
const MAINNET_API = '/blockchain-rust/mainnet';
let API_BASE = '';

let _network = localStorage.getItem('pkt-network') || 'testnet';

function setNetwork(net) {
  _network = net;
  localStorage.setItem('pkt-network', net);
  API_BASE = net === 'mainnet' ? MAINNET_API : TESTNET_API;
  const btn = document.getElementById('networkToggle');
  if (btn) {
    btn.textContent  = net === 'mainnet' ? '🟡 Mainnet' : '🟢 Testnet';
    btn.style.color  = net === 'mainnet' ? '#f59e0b' : '#10b981';
  }
  refreshAll();
}

function toggleNetwork() {
  setNetwork(_network === 'testnet' ? 'mainnet' : 'testnet');
}

// Init
API_BASE = _network === 'mainnet' ? MAINNET_API : TESTNET_API;

/* ── UTILS ──────────────────────────────────────────────────── */
function shortHash(h) { return h ? h.slice(0,10)+'…'+h.slice(-8) : '—'; }
function shortAddr(a) { return a ? a.slice(0,8)+'…'+a.slice(-6) : '—'; }

// Make an address string clickable → navigate to Testnet page + lookup
function addrLink(addr) {
  if (!addr || addr === '—' || addr === 'coinbase' || addr === 'unknown') return addr || '—';
  var safe = addr.replace(/\\/g,'\\\\').replace(/'/g,"\\'");
  return '<span class="addr-clickable" title="Click to look up address" onclick="gotoAddress(\'' + safe + '\')">' + addr + '</span>';
}
function gotoAddress(addr) {
  window.location.href = '/blockchain-rust/address/' + encodeURIComponent(addr);
}
function ago(secs) {
  if (secs < 60) return secs + 's ago';
  if (secs < 3600) return Math.floor(secs/60) + 'm ago';
  return Math.floor(secs/3600) + 'h ago';
}
function tsAgo(ts) {
  const secs = Math.max(0, Math.floor((Date.now() - ts) / 1000));
  return ago(secs);
}
function pakletsToPkt(p) { return (p / 1e9).toFixed(4) + ' PKT'; }

let blocks = [];
let txs    = [];
let stats  = {};

/* ── API FETCH ──────────────────────────────────────────────── */
async function fetchStats() {
  try {
    const r = await fetch(`${API_BASE}/api/testnet/summary`);
    if (!r.ok) return;
    stats = await r.json();
    stats.avg_block_time_s = stats.avg_block_time_s ?? stats.block_time_avg ?? 0;
    document.getElementById('stat-height').textContent    = (stats.height ?? 0).toLocaleString("en-US");
    document.getElementById('stat-blocktime').textContent = stats.avg_block_time_s
      ? Math.round(stats.avg_block_time_s) + 's' : '—';
    document.getElementById('stat-hashrate').textContent  = fmtHashrate(stats.hashrate ?? 0);
    document.getElementById('stat-nodes').textContent     = stats.utxo_count ?? '—';
    document.getElementById('stat-txs').textContent       = stats.mempool_count ?? '—';
    document.getElementById('stat-diff').textContent      = stats.difficulty ?? '—';
    buildTicker(stats);
  } catch(e) { console.warn('fetchStats', e); }
}

async function fetchBlocks() {
  try {
    const r = await fetch(`${API_BASE}/api/testnet/headers?limit=20`);
    if (!r.ok) return;
    const data = await r.json();
    blocks = (data.headers ?? data.blocks ?? []).map(b => ({
      height:    b.height ?? b.index ?? 0,
      hash:      b.hash ?? '',
      prevHash:  b.prev_hash ?? '',
      timestamp: (b.timestamp ?? 0) * 1000,
      txCount:   b.tx_count ?? 0,
      miner:     b.miner ?? '',
      reward:    50e9,
      difficulty: b.difficulty ?? 1,
      nonce:     b.nonce ?? 0,
    }));
  } catch(e) { console.warn('fetchBlocks', e); }
}

async function fetchTxs() {
  try {
    const r = await fetch(`${API_BASE}/api/testnet/mempool?limit=20`);
    if (!r.ok) return;
    const data = await r.json();
    txs = (data.txs ?? []).map(t => ({
      txid:        t.txid ?? t.hash ?? '',
      blockHeight: 0,
      timestamp:   (t.timestamp ?? 0) * 1000,
      from:        '',
      to:          '',
      amount:      0,
      fee:         (t.fee ?? 0) / 1e9,
      isCoinbase:  false,
    }));
  } catch(e) { console.warn('fetchTxs', e); }
}

async function fetchBlockDetail(height) {
  const r = await fetch(`${API_BASE}/api/testnet/block/${height}`);
  if (!r.ok) throw new Error('not found');
  return r.json();
}

async function fetchTxDetail(txid) {
  const r = await fetch(`${API_BASE}/api/testnet/tx/${encodeURIComponent(txid)}`);
  if (!r.ok) throw new Error('not found');
  return r.json();
}

async function fetchSearch(q) {
  const r = await fetch(`${API_BASE}/api/search?q=${encodeURIComponent(q)}`);
  if (!r.ok) return null;
  return r.json();
}

function fmtHashrate(h) {
  if (h >= 1e15) return (h/1e15).toFixed(2) + ' PH/s';
  if (h >= 1e12) return (h/1e12).toFixed(2) + ' TH/s';
  if (h >= 1e9)  return (h/1e9).toFixed(2)  + ' GH/s';
  if (h >= 1e6)  return (h/1e6).toFixed(2)  + ' MH/s';
  if (h >= 1e3)  return (h/1e3).toFixed(2)  + ' KH/s';
  return h + ' H/s';
}

async function refreshAll() {
  await fetchStats();
  await fetchBlocks();
  await fetchTxs();
  if (document.getElementById('home-page').style.display !== 'none') renderHome();
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
  const diff   = s.difficulty ?? '—';
  const hr     = fmtHashrate(s.hashrate ?? 0);
  const reward = pakletsToPkt(s.block_reward ?? 50e9);
  const bt     = s.avg_block_time_s ? Math.round(s.avg_block_time_s) + 's' : '—';
  const items  = [
    `📦 Block #${height}`, `⚡ ${hr}`, `💰 ${reward} reward`,
    `🔄 Difficulty ${diff}`, `⏱ ${bt} block time`, `🔐 BLAKE3 PoW`, `🛡 Post-Quantum ready`,
    `📦 Block #${height}`, `⚡ ${hr}`, `💰 ${reward} reward`,
    `🔄 Difficulty ${diff}`, `⏱ ${bt} block time`, `🔐 BLAKE3 PoW`, `🛡 Post-Quantum ready`,
  ];
  const el = document.getElementById('tickerInner');
  el.innerHTML = items.map(t => `<span class="ticker-item">${t}<span class="ticker-sep"> ◆ </span></span>`).join('');
}

/* ── RENDER BLOCKS ──────────────────────────────────────────── */
function renderBlockItem(b, container, clickFn) {
  const secsAgo = Math.floor((Date.now() - b.timestamp) / 1000);
  const div = document.createElement('div');
  div.className = 'list-item block-item';
  div.innerHTML = `
    <div class="item-icon item-icon-block">#${b.height % 1000}</div>
    <div class="item-main">
      <div class="item-primary">#${b.height.toLocaleString("en-US")}</div>
      <div class="item-secondary">${b.txCount} txns &nbsp;·&nbsp; Miner: <span class="addr-short">${shortAddr(b.miner)}</span></div>
    </div>
    <div class="item-right">
      <div class="item-amount">${pakletsToPkt(b.reward ?? 50e9)}</div>
      <div class="item-age">${ago(secsAgo)}</div>
    </div>
  `;
  div.onclick = clickFn || (() => showBlockDetail(b));
  container.appendChild(div);
}

function renderTxItem(tx, container, clickFn) {
  const secsAgo = Math.floor((Date.now() - tx.timestamp) / 1000);
  const div = document.createElement('div');
  div.className = 'list-item tx-item';
  div.innerHTML = `
    <div>
      <div style="display:flex;align-items:center;gap:8px;margin-bottom:3px;">
        <span class="item-primary">${shortHash(tx.txid)}</span>
        <span class="badge ${tx.isCoinbase ? 'badge-coinbase' : 'badge-tx'}">${tx.isCoinbase ? 'coinbase' : 'transfer'}</span>
      </div>
      <div class="tx-from-to item-secondary">
        <span class="addr-short">${tx.isCoinbase ? 'coinbase' : shortAddr(tx.from)}</span>
        <span class="arrow">→</span>
        <span class="addr-short">${shortAddr(tx.to)}</span>
      </div>
    </div>
    <div class="item-right">
      <div class="item-amount">${tx.amount.toFixed(4)} PKT</div>
      <div class="item-age">${ago(secsAgo)}</div>
    </div>
  `;
  div.onclick = clickFn || (() => showTxDetail(tx));
  container.appendChild(div);
}

/* ── PAGES ──────────────────────────────────────────────────── */
function hideAll() {
  document.getElementById('home-page').style.display = 'none';
  document.querySelectorAll('.page').forEach(p => p.classList.remove('active'));
}
function showHome() {
  hideAll();
  if (window.pktHideAddrPanel) window.pktHideAddrPanel();
  history.replaceState(null, '', location.pathname + location.search);
  document.getElementById('home-page').style.display = 'block';
  renderHome();
}
function showBlocks() {
  hideAll();
  history.replaceState(null, '', '#blocks');
  document.getElementById('blocks-page').classList.add('active');
  const el = document.getElementById('allBlocks');
  el.innerHTML = '';
  blocks.forEach(b => renderBlockItem(b, el));
}
function showTxs() {
  hideAll();
  history.replaceState(null, '', '#txs');
  document.getElementById('txs-page').classList.add('active');
  const el = document.getElementById('allTxs');
  el.innerHTML = '';
  txs.forEach(tx => renderTxItem(tx, el));
}
function showStats() {
  hideAll();
  history.replaceState(null, '', '#stats');
  document.getElementById('stats-page').classList.add('active');
  const el = document.getElementById('statsContent');
  const s  = stats;
  const rows = [
    ['Network',           'PKT Chain'],
    ['Algorithm',         'BLAKE3 PoW'],
    ['Latest Block',      `#${(s.height ?? 0).toLocaleString("en-US")}`],
    ['Block Reward',      pakletsToPkt(s.block_reward ?? 50e9)],
    ['Difficulty',        s.difficulty ?? '—'],
    ['Hashrate',          fmtHashrate(s.hashrate ?? 0)],
    ['Avg Block Time',    s.avg_block_time_s ? Math.round(s.avg_block_time_s) + 's' : '—'],
    ['UTXO Count',        (s.utxo_count ?? 0).toLocaleString("en-US")],
    ['Mempool',           (s.mempool_count ?? 0) + ' txs'],
    ['Total Supply',      pakletsToPkt(s.total_supply ?? 0)],
    ['Block Count',       (s.block_count ?? 0).toLocaleString("en-US")],
    ['P2P Port (testnet)','8333'],
    ['P2P Port (mainnet)','64764'],
    ['Signature',         'ECDSA + Dilithium (hybrid post-quantum)'],
    ['Hash Function',     'BLAKE3 (PoW) · SHA-256 (address)'],
    ['Address Format',    'Base58Check (P2PKH / P2TR)'],
  ];
  const div = document.createElement('div');
  rows.forEach(([k, v]) => {
    div.innerHTML += `<div class="kv-row"><div class="kv-key">${k}</div><div class="kv-val normal">${v}</div></div>`;
  });
  el.innerHTML = '';
  el.appendChild(div);
}

async function showBlockDetail(b) {
  hideAll();
  document.getElementById('block-detail').classList.add('active');
  const el = document.getElementById('blockDetailContent');
  el.innerHTML = '<div style="padding:24px;color:var(--muted)">Loading…</div>';
  try {
    const d = await fetchBlockDetail(b.height ?? b);
    const block = d.block ?? d;
    const height = block.index ?? block.height ?? b.height ?? 0;
    const ts = block.timestamp ? new Date(block.timestamp * 1000).toISOString() : '—';
    el.innerHTML = `
      <div class="detail-title">
        <span>📦 Block <span style="color:var(--blue)">#${height.toLocaleString("en-US")}</span></span>
      </div>
      <div class="panel">
        <div class="kv-row"><div class="kv-key">Block Height</div><div class="kv-val">${height.toLocaleString("en-US")}</div></div>
        <div class="kv-row"><div class="kv-key">Hash</div><div class="kv-val">${block.hash ?? '—'}</div></div>
        <div class="kv-row"><div class="kv-key">Previous Hash</div><div class="kv-val">${block.prev_hash ?? '—'}</div></div>
        <div class="kv-row"><div class="kv-key">Timestamp</div><div class="kv-val normal">${ts}</div></div>
        <div class="kv-row"><div class="kv-key">Transactions</div><div class="kv-val normal">${block.tx_count ?? block.transactions?.length ?? 0}</div></div>
        <div class="kv-row"><div class="kv-key">Nonce</div><div class="kv-val">${(block.nonce ?? 0).toLocaleString("en-US")}</div></div>
        <div class="kv-row"><div class="kv-key">Difficulty</div><div class="kv-val normal">${block.difficulty ?? '—'}</div></div>
        <div class="kv-row"><div class="kv-key">Block Reward</div><div class="kv-val normal" style="color:var(--pkt)">${pakletsToPkt(block.reward ?? 50e9)}</div></div>
        <div class="kv-row"><div class="kv-key">Miner</div><div class="kv-val" style="color:var(--blue)">${addrLink(block.miner ?? block.miner_hash ?? '—')}</div></div>
      </div>`;
  } catch(e) {
    el.innerHTML = '<div style="padding:24px;color:var(--red)">Block not found</div>';
  }
}

async function showTxDetail(tx) {
  hideAll();
  document.getElementById('tx-detail').classList.add('active');
  const el = document.getElementById('txDetailContent');
  el.innerHTML = '<div style="padding:24px;color:var(--muted)">Loading…</div>';
  try {
    const d = await fetchTxDetail(tx.txid ?? tx);
    const t = d.tx ?? d;
    const ts = t.timestamp ? new Date(t.timestamp * 1000).toISOString() : '—';
    const isCoinbase = t.is_coinbase ?? false;
    const amount = ((t.amount ?? t.total_out ?? 0) / 1e9).toFixed(8);
    const fee    = ((t.fee ?? 0) / 1e9).toFixed(8);
    el.innerHTML = `
      <div class="detail-title">
        <span>💸 Transaction</span>
        <span class="hash">${t.tx_id ?? t.txid ?? ''}</span>
      </div>
      <div class="panel">
        <div class="kv-row"><div class="kv-key">TxID</div><div class="kv-val">${t.tx_id ?? t.txid ?? '—'}</div></div>
        <div class="kv-row"><div class="kv-key">Block</div><div class="kv-val" style="color:var(--blue)">#${(t.block_height ?? t.block_index ?? 0).toLocaleString("en-US")}</div></div>
        <div class="kv-row"><div class="kv-key">Timestamp</div><div class="kv-val normal">${ts}</div></div>
        <div class="kv-row"><div class="kv-key">Type</div><div class="kv-val normal"><span class="badge ${isCoinbase ? 'badge-coinbase' : 'badge-tx'}">${isCoinbase ? 'coinbase' : 'transfer'}</span></div></div>
        <div class="kv-row"><div class="kv-key">From</div><div class="kv-val" style="color:var(--blue)">${isCoinbase ? 'coinbase' : addrLink(t.from ?? '—')}</div></div>
        <div class="kv-row"><div class="kv-key">To</div><div class="kv-val" style="color:var(--blue)">${addrLink(t.to ?? t.outputs?.[0]?.address ?? '—')}</div></div>
        <div class="kv-row"><div class="kv-key">Amount</div><div class="kv-val normal" style="color:var(--pkt)">${amount} PKT</div></div>
        <div class="kv-row"><div class="kv-key">Fee</div><div class="kv-val normal">${fee} PKT</div></div>
      </div>`;
  } catch(e) {
    el.innerHTML = '<div style="padding:24px;color:var(--red)">Transaction not found</div>';
  }
}

function renderHome() {
  const lb = document.getElementById('latestBlocks');
  const lt = document.getElementById('latestTxs');
  lb.innerHTML = blocks.length ? '' : '<div style="padding:18px;color:var(--muted);font-size:.85rem">No blocks yet — run: cargo run -- mine</div>';
  lt.innerHTML = txs.length   ? '' : '<div style="padding:18px;color:var(--muted);font-size:.85rem">No transactions yet</div>';
  blocks.slice(0, 8).forEach(b  => renderBlockItem(b,  lb));
  txs.slice(0, 8).forEach(tx => renderTxItem(tx, lt));
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
    if (!results.length) {
      el.innerHTML = '<div class="search-empty">No results found</div>';
      return;
    }
    const typeIcon = { block: '📦', tx: '💸', address: '👤', label: '🏷' };
    const typeCls  = { block: 'item-icon-block', tx: 'item-icon-tx', address: '', label: '' };
    el.innerHTML = results.map((r, i) => {
      const icon  = typeIcon[r.type] || '🔍';
      const cls   = typeCls[r.type] || '';
      const sub   = r.type === 'block'   ? `Height ${r.value}` :
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
    showBlockDetail({ height: parseInt(r.value) });
  } else if (r.type === 'tx') {
    window.location.href = `${API_BASE}/rx/${encodeURIComponent(r.value)}`;
  } else if (r.type === 'address' || r.type === 'label') {
    window.location.href = `${API_BASE}/address/${encodeURIComponent(r.value)}`;
  }
}

async function heroSearch() {
  const q = document.getElementById('heroInput').value.trim();
  if (!q) return;
  // delegate to search modal — API handles type detection
  document.getElementById('searchInput').value = q;
  openSearch();
  doSearch(q);
}

/* ── INIT ────────────────────────────────────────────────────── */
function routeFromHash() {
  const hash = location.hash;
  if (hash === '#blocks')  showBlocks();
  else if (hash === '#txs')     showTxs();
  else if (hash === '#stats')   showStats();
  else if (hash === '#testnet' && window.showTestnet) window.showTestnet();
  else renderHome();
}

buildTicker({});
setNetwork(_network); // init toggle UI + load data
refreshAll().then(() => routeFromHash());
window.addEventListener('hashchange', () => routeFromHash());
setInterval(refreshAll, 15000); // refresh từ API mỗi 15s
