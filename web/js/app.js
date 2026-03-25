/* ── CONFIG ─────────────────────────────────────────────────── */
const API_BASE = '/blockchain-rust';

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
  var hash = '#addr/' + encodeURIComponent(addr);
  if (window.location.hash === hash) {
    // Hash unchanged → hashchange won't fire → dispatch manually
    window.dispatchEvent(new HashChangeEvent('hashchange'));
  } else {
    window.location.hash = hash;
  }
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
    const r = await fetch(`${API_BASE}/api/stats`);
    if (!r.ok) return;
    stats = await r.json();
    document.getElementById('stat-height').textContent    = (stats.height ?? 0).toLocaleString();
    document.getElementById('stat-blocktime').textContent = stats.avg_block_time_s
      ? Math.round(stats.avg_block_time_s) + 's' : '—';
    document.getElementById('stat-hashrate').textContent  = fmtHashrate(stats.hashrate ?? 0);
    document.getElementById('stat-nodes').textContent     = stats.utxo_count ?? '—';
    document.getElementById('stat-txs').textContent       = stats.utxo_count ?? '—';
    document.getElementById('stat-diff').textContent      = stats.difficulty ?? '—';
    buildTicker(stats);
  } catch(e) { console.warn('fetchStats', e); }
}

async function fetchBlocks() {
  try {
    const r = await fetch(`${API_BASE}/api/blocks?limit=20`);
    if (!r.ok) return;
    const data = await r.json();
    blocks = (data.blocks ?? data ?? []).map(b => ({
      height:    b.index ?? b.height ?? 0,
      hash:      b.hash ?? '',
      prevHash:  b.prev_hash ?? '',
      timestamp: (b.timestamp ?? 0) * 1000,
      txCount:   b.tx_count ?? b.transactions?.length ?? 0,
      miner:     b.miner ?? b.miner_hash ?? '',
      reward:    b.reward ?? 50e9,
      difficulty: b.difficulty ?? stats.difficulty ?? 3,
      nonce:     b.nonce ?? 0,
    }));
  } catch(e) { console.warn('fetchBlocks', e); }
}

async function fetchTxs() {
  try {
    const r = await fetch(`${API_BASE}/api/txs?limit=20`);
    if (!r.ok) return;
    const data = await r.json();
    txs = (data.txs ?? data ?? []).map(t => ({
      txid:        t.tx_id ?? t.txid ?? '',
      blockHeight: t.block_height ?? t.block_index ?? 0,
      timestamp:   (t.block_timestamp ?? t.timestamp ?? 0) * 1000,
      from:        t.from ?? (t.is_coinbase ? 'coinbase' : ''),
      to:          t.to ?? t.outputs?.[0]?.address ?? '',
      amount:      (t.output_total ?? t.amount ?? t.total_out ?? 0) / 1e9,
      fee:         (t.fee ?? 0) / 1e9,
      isCoinbase:  t.is_coinbase ?? false,
    }));
  } catch(e) { console.warn('fetchTxs', e); }
}

async function fetchBlockDetail(height) {
  const r = await fetch(`${API_BASE}/api/block/${height}`);
  if (!r.ok) throw new Error('not found');
  return r.json();
}

async function fetchTxDetail(txid) {
  const r = await fetch(`${API_BASE}/api/tx/${txid}`);
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
  const height = (s.height ?? 0).toLocaleString();
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
      <div class="item-primary">#${b.height.toLocaleString()}</div>
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
  document.getElementById('blocks-page').classList.add('active');
  const el = document.getElementById('allBlocks');
  el.innerHTML = '';
  blocks.forEach(b => renderBlockItem(b, el));
}
function showTxs() {
  hideAll();
  document.getElementById('txs-page').classList.add('active');
  const el = document.getElementById('allTxs');
  el.innerHTML = '';
  txs.forEach(tx => renderTxItem(tx, el));
}
function showStats() {
  hideAll();
  document.getElementById('stats-page').classList.add('active');
  const el = document.getElementById('statsContent');
  const s  = stats;
  const rows = [
    ['Network',           'PKT Chain'],
    ['Algorithm',         'BLAKE3 PoW'],
    ['Latest Block',      `#${(s.height ?? 0).toLocaleString()}`],
    ['Block Reward',      pakletsToPkt(s.block_reward ?? 50e9)],
    ['Difficulty',        s.difficulty ?? '—'],
    ['Hashrate',          fmtHashrate(s.hashrate ?? 0)],
    ['Avg Block Time',    s.avg_block_time_s ? Math.round(s.avg_block_time_s) + 's' : '—'],
    ['UTXO Count',        (s.utxo_count ?? 0).toLocaleString()],
    ['Mempool',           (s.mempool_count ?? 0) + ' txs'],
    ['Total Supply',      pakletsToPkt(s.total_supply ?? 0)],
    ['Block Count',       (s.block_count ?? 0).toLocaleString()],
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
        <span>📦 Block <span style="color:var(--blue)">#${height.toLocaleString()}</span></span>
      </div>
      <div class="panel">
        <div class="kv-row"><div class="kv-key">Block Height</div><div class="kv-val">${height.toLocaleString()}</div></div>
        <div class="kv-row"><div class="kv-key">Hash</div><div class="kv-val">${block.hash ?? '—'}</div></div>
        <div class="kv-row"><div class="kv-key">Previous Hash</div><div class="kv-val">${block.prev_hash ?? '—'}</div></div>
        <div class="kv-row"><div class="kv-key">Timestamp</div><div class="kv-val normal">${ts}</div></div>
        <div class="kv-row"><div class="kv-key">Transactions</div><div class="kv-val normal">${block.tx_count ?? block.transactions?.length ?? 0}</div></div>
        <div class="kv-row"><div class="kv-key">Nonce</div><div class="kv-val">${(block.nonce ?? 0).toLocaleString()}</div></div>
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
        <div class="kv-row"><div class="kv-key">Block</div><div class="kv-val" style="color:var(--blue)">#${(t.block_height ?? t.block_index ?? 0).toLocaleString()}</div></div>
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

function doSearch(query) {
  const q = query.trim().toLowerCase();
  const el = document.getElementById('searchResults');
  if (!q) {
    el.innerHTML = '<div class="search-empty">Type to search blocks, transactions, or addresses</div>';
    return;
  }
  let results = [];
  // match blocks by height
  blocks.forEach(b => {
    if (String(b.height).includes(q) || b.hash.startsWith(q)) {
      results.push({ type: 'block', label: `Block #${b.height.toLocaleString()}`, value: b.hash, data: b });
    }
  });
  // match txs
  txs.forEach(tx => {
    if (tx.txid.startsWith(q)) {
      results.push({ type: 'tx', label: 'Transaction', value: tx.txid, data: tx });
    }
  });
  // address match từ cached txs
  const addrs = new Set([...txs.map(t => t.from), ...txs.map(t => t.to)].filter(Boolean));
  addrs.forEach(addr => {
    if (addr.toLowerCase().includes(q)) {
      results.push({ type: 'address', label: 'Address', value: addr, data: null });
    }
  });
  results = results.slice(0, 6);
  if (!results.length) {
    el.innerHTML = '<div class="search-empty">No results found</div>';
    return;
  }
  el.innerHTML = results.map((r, i) => {
    const icon = r.type === 'block' ? '📦' : r.type === 'tx' ? '💸' : '👤';
    const cls  = r.type === 'block' ? 'icon-block' : r.type === 'tx' ? 'icon-tx' : 'icon-addr';
    return `<div class="search-result-item" onclick="selectResult(${i})">
      <div class="search-result-icon ${cls}">${icon}</div>
      <div class="search-result-main">
        <div class="search-result-type">${r.type}</div>
        <div class="search-result-value">${r.value}</div>
      </div>
    </div>`;
  }).join('');
  el._results = results;
}

function selectResult(i) {
  const results = document.getElementById('searchResults')._results;
  if (!results) return;
  const r = results[i];
  closeSearch();
  if (r.type === 'block') showBlockDetail(r.data);
  else if (r.type === 'tx') showTxDetail(r.data);
}

async function heroSearch() {
  const q = document.getElementById('heroInput').value.trim();
  if (!q) return;
  // try block height
  const height = parseInt(q);
  if (!isNaN(height) && String(height) === q) {
    showBlockDetail({ height });
    return;
  }
  // try tx hash prefix in local cache
  const tx = txs.find(x => x.txid.startsWith(q.toLowerCase()));
  if (tx) { showTxDetail(tx); return; }
  // fallback: open search modal
  document.getElementById('searchInput').value = q;
  openSearch();
  doSearch(q);
}

/* ── INIT ────────────────────────────────────────────────────── */
buildTicker({});
refreshAll().then(() => renderHome());
setInterval(refreshAll, 15000); // refresh từ API mỗi 15s
