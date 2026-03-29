// playground.js — v19.7 API Playground
'use strict';

// ── Endpoint definitions ───────────────────────────────────────────────────────
const ENDPOINTS = [
  {
    id: 'block',
    label: 'Block by height',
    path: '/api/testnet/block/{height}',
    params: [{ name: 'height', type: 'number', placeholder: '100', label: 'Block Height' }],
    desc: 'Lấy thông tin một block theo height',
  },
  {
    id: 'blocks',
    label: 'Block list',
    path: '/api/testnet/headers',
    params: [
      { name: 'page',  type: 'number', placeholder: '0',  label: 'Page'  },
      { name: 'limit', type: 'number', placeholder: '25', label: 'Limit' },
    ],
    desc: 'Danh sách block headers (paginated)',
  },
  {
    id: 'tx',
    label: 'Transaction by txid',
    path: '/api/testnet/tx/{txid}',
    params: [{ name: 'txid', type: 'text', placeholder: 'abc123...', label: 'Transaction ID' }],
    desc: 'Lấy thông tin transaction theo txid',
  },
  {
    id: 'recent-txs',
    label: 'Recent transactions',
    path: '/api/testnet/txs',
    params: [
      { name: 'page',  type: 'number', placeholder: '0',  label: 'Page'  },
      { name: 'limit', type: 'number', placeholder: '25', label: 'Limit' },
    ],
    desc: 'Danh sách transactions gần nhất',
  },
  {
    id: 'balance',
    label: 'Address balance',
    path: '/api/testnet/balance/{address}',
    params: [{ name: 'address', type: 'text', placeholder: 'pkt1...', label: 'Address' }],
    desc: 'Số dư PKT của một address',
  },
  {
    id: 'utxos',
    label: 'Address UTXOs',
    path: '/api/testnet/utxos/{address}',
    params: [{ name: 'address', type: 'text', placeholder: 'pkt1...', label: 'Address' }],
    desc: 'Danh sách UTXOs của address',
  },
  {
    id: 'address-txs',
    label: 'Address transactions',
    path: '/api/testnet/address/{address}/txs',
    params: [
      { name: 'address', type: 'text',   placeholder: 'pkt1...', label: 'Address' },
      { name: 'page',    type: 'number', placeholder: '0',       label: 'Page'    },
      { name: 'limit',   type: 'number', placeholder: '25',      label: 'Limit'   },
    ],
    desc: 'Lịch sử giao dịch của address (paginated)',
  },
  {
    id: 'summary',
    label: 'Network summary',
    path: '/api/testnet/summary',
    params: [],
    desc: 'Tóm tắt mạng: hashrate, difficulty, mempool, block time...',
  },
  {
    id: 'sync-status',
    label: 'Sync status',
    path: '/api/testnet/sync-status',
    params: [],
    desc: 'Trạng thái đồng bộ blockchain của node',
  },
  {
    id: 'mempool',
    label: 'Mempool',
    path: '/api/testnet/mempool',
    params: [],
    desc: 'Danh sách transactions đang chờ trong mempool',
  },
  {
    id: 'fee-histogram',
    label: 'Fee histogram',
    path: '/api/testnet/mempool/fee-histogram',
    params: [],
    desc: 'Phân bố fee rate trong mempool',
  },
  {
    id: 'analytics',
    label: 'Analytics',
    path: '/api/testnet/analytics',
    params: [],
    desc: 'Time-series hashrate / difficulty / block time',
  },
  {
    id: 'rich-list',
    label: 'Rich list',
    path: '/api/testnet/rich-list',
    params: [],
    desc: 'Top addresses có nhiều PKT nhất',
  },
  {
    id: 'search',
    label: 'Search',
    path: '/api/testnet/search',
    params: [{ name: 'q', type: 'text', placeholder: 'block height, txid, address…', label: 'Query' }],
    desc: 'Tìm kiếm block, transaction hoặc address',
  },
  {
    id: 'health',
    label: 'Health',
    path: '/api/health/detailed',
    params: [],
    desc: 'Health status chi tiết của node',
  },
];

// ── State ──────────────────────────────────────────────────────────────────────
let currentEp = ENDPOINTS[0];

// ── Build URL ──────────────────────────────────────────────────────────────────
function buildApiUrl(ep, values) {
  let path = ep.path;
  const query = [];
  for (const p of ep.params) {
    const val = (values[p.name] || '').trim();
    if (path.includes(`{${p.name}}`)) {
      path = path.replace(`{${p.name}}`, val ? encodeURIComponent(val) : `{${p.name}}`);
    } else if (val) {
      query.push(`${p.name}=${encodeURIComponent(val)}`);
    }
  }
  return path + (query.length ? '?' + query.join('&') : '');
}

function getValues(ep) {
  const v = {};
  for (const p of ep.params) {
    const el = document.getElementById(`p-${p.name}`);
    v[p.name] = el ? el.value : '';
  }
  return v;
}

// ── JSON syntax highlight ──────────────────────────────────────────────────────
function highlight(json) {
  return json
    .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(
      /("(?:\\u[0-9a-fA-F]{4}|\\[^u]|[^\\"])*"(\s*:)?|true|false|null|-?\d+\.?\d*(?:[eE][+\-]?\d+)?)/g,
      (m) => {
        if (/^"/.test(m)) return `<span class="${/:$/.test(m) ? 'jk' : 'jv'}">${m}</span>`;
        if (/true|false/.test(m)) return `<span class="jb">${m}</span>`;
        if (m === 'null') return `<span class="jn">${m}</span>`;
        return `<span class="jd">${m}</span>`;
      },
    );
}

// ── Render ─────────────────────────────────────────────────────────────────────
function renderParams(ep) {
  const panel = document.getElementById('params-panel');
  const container = document.getElementById('params');
  if (!ep.params.length) { panel.style.display = 'none'; return; }
  panel.style.display = '';
  container.innerHTML = ep.params.map((p) => `
    <label class="param-label" for="p-${p.name}">${p.label}</label>
    <input class="param-input" id="p-${p.name}" type="${p.type}"
      placeholder="${p.placeholder}" oninput="onParamChange()" />
  `).join('');
}

function updateUrlDisplay() {
  const url = buildApiUrl(currentEp, getValues(currentEp));
  document.getElementById('url-display').textContent = url;
  // Update hash for bookmarking / sharing
  const vals = getValues(currentEp);
  const parts = [`ep=${currentEp.id}`,
    ...Object.entries(vals).filter(([, v]) => v).map(([k, v]) => `${k}=${encodeURIComponent(v)}`)];
  history.replaceState(null, '', '#' + parts.join('&'));
}

function clearResponse() {
  document.getElementById('response').innerHTML =
    '<span style="color:var(--muted);font-style:italic">Click ▶ Run để gọi API…</span>';
  document.getElementById('status-badge').textContent = '';
  document.getElementById('time-badge').textContent = '';
}

// ── Handlers ───────────────────────────────────────────────────────────────────
function onEndpointChange() {
  const id = document.getElementById('ep-select').value;
  currentEp = ENDPOINTS.find((e) => e.id === id) || ENDPOINTS[0];
  document.getElementById('ep-desc').textContent = currentEp.desc;
  renderParams(currentEp);
  updateUrlDisplay();
  clearResponse();
}

function onParamChange() {
  updateUrlDisplay();
}

// ── Run ────────────────────────────────────────────────────────────────────────
async function runRequest() {
  const url = buildApiUrl(currentEp, getValues(currentEp));
  const respEl   = document.getElementById('response');
  const statusEl = document.getElementById('status-badge');
  const timeEl   = document.getElementById('time-badge');
  const runBtn   = document.getElementById('run-btn');

  respEl.innerHTML = '<span style="color:var(--muted);font-style:italic">Loading…</span>';
  statusEl.textContent = '';
  timeEl.textContent = '';
  runBtn.disabled = true;

  const t0 = Date.now();
  try {
    const res = await fetch(url);
    const ms = Date.now() - t0;
    statusEl.textContent = `HTTP ${res.status}`;
    statusEl.className = `badge ${res.ok ? 'badge-ok' : 'badge-err'}`;
    timeEl.textContent = `${ms}ms`;

    const text = await res.text();
    try {
      const pretty = JSON.stringify(JSON.parse(text), null, 2);
      respEl.innerHTML = highlight(pretty);
    } catch {
      respEl.textContent = text;
    }
  } catch (e) {
    const ms = Date.now() - t0;
    statusEl.textContent = 'ERROR';
    statusEl.className = 'badge badge-err';
    timeEl.textContent = `${ms}ms`;
    respEl.textContent = `Network error: ${e.message}`;
  } finally {
    runBtn.disabled = false;
  }
}

// ── Copy ───────────────────────────────────────────────────────────────────────
function copyUrl() {
  const url = buildApiUrl(currentEp, getValues(currentEp));
  const full = window.location.origin + url;
  navigator.clipboard.writeText(full).then(() => flash('copy-url-btn', 'Copied!'));
}

function copyResponse() {
  navigator.clipboard.writeText(document.getElementById('response').textContent)
    .then(() => flash('copy-resp-btn', 'Copied!'));
}

function flash(id, label) {
  const btn = document.getElementById(id);
  const orig = btn.textContent;
  btn.textContent = label;
  setTimeout(() => { btn.textContent = orig; }, 1500);
}

// ── Restore from URL hash ──────────────────────────────────────────────────────
function restoreFromHash() {
  const hash = location.hash.slice(1);
  if (!hash) return;
  const map = Object.fromEntries(
    hash.split('&').map((s) => {
      const i = s.indexOf('=');
      return i < 0 ? [s, ''] : [s.slice(0, i), decodeURIComponent(s.slice(i + 1))];
    }),
  );
  if (map.ep) {
    const ep = ENDPOINTS.find((e) => e.id === map.ep);
    if (ep) {
      currentEp = ep;
      document.getElementById('ep-select').value = ep.id;
      document.getElementById('ep-desc').textContent = ep.desc;
    }
  }
  renderParams(currentEp);
  for (const p of currentEp.params) {
    const el = document.getElementById(`p-${p.name}`);
    if (el && map[p.name]) el.value = map[p.name];
  }
  updateUrlDisplay();
}

// ── Keyboard: Ctrl/Cmd+Enter = Run ────────────────────────────────────────────
document.addEventListener('keydown', (e) => {
  if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
    e.preventDefault();
    runRequest();
  }
});

// ── Boot ───────────────────────────────────────────────────────────────────────
(function init() {
  const sel = document.getElementById('ep-select');
  sel.innerHTML = ENDPOINTS.map((e) => `<option value="${e.id}">${e.label}</option>`).join('');
  sel.addEventListener('change', onEndpointChange);
  onEndpointChange();
  restoreFromHash();
})();
