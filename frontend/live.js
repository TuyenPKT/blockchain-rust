// v14.8 — PKT WebSocket Live Feed
// Kết nối /ws → nhận NewBlock/NewTx/Stats events real-time
// Toast notification, cập nhật stats DOM, reconnect tự động.

(function () {
  'use strict';

  const WS_PATH      = '/ws';
  const MAX_TOASTS   = 5;
  const TOAST_TTL_MS = 4000;   // toast tự biến mất sau 4s

  let ws        = null;
  let attempt   = 0;           // reconnect attempt count
  let toastSeq  = 0;
  let statusEl  = null;

  // ── WebSocket lifecycle ────────────────────────────────────────────────────

  function connect() {
    const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
    const url   = `${proto}//${location.host}${WS_PATH}`;

    setStatus('connecting');

    try {
      ws = new WebSocket(url);
    } catch (e) {
      scheduleReconnect();
      return;
    }

    ws.onopen = () => {
      attempt = 0;
      setStatus('connected');
    };

    ws.onmessage = (ev) => {
      try {
        const data = JSON.parse(ev.data);
        handleEvent(data);
      } catch (_) {}
    };

    ws.onerror = () => {};

    ws.onclose = () => {
      setStatus('disconnected');
      scheduleReconnect();
    };
  }

  function scheduleReconnect() {
    // Exponential backoff: 1s, 2s, 4s, 8s, 16s, max 30s
    const delay = Math.min(1000 * Math.pow(2, attempt), 30_000);
    attempt++;
    setStatus('reconnecting');
    setTimeout(connect, delay);
  }

  // ── Event handling ─────────────────────────────────────────────────────────

  function handleEvent(data) {
    const type = (data.type || data.event || '').toLowerCase();

    if (type === 'new_block' || type === 'newblock') {
      onNewBlock(data);
    } else if (type === 'new_tx' || type === 'newtx') {
      onNewTx(data);
    } else if (type === 'stats') {
      onStats(data);
    }
  }

  function onNewBlock(data) {
    const height = data.height ?? data.block?.height ?? '?';
    const hash   = data.hash   ?? data.block?.hash   ?? '';

    toast(`⛏ New Block #${height}`, 'success');
    updateStat('pkt-stat-height', height);
    updateStat('pkt-stat-hash',   shortH(hash));

    // Prepend to live feed list
    feedItem(`<a href="#block/${height}" class="pk-link">Block #${height}</a>
              <span class="pk-feed-hash">${shortH(hash)}</span>`, 'block');
  }

  function onNewTx(data) {
    const txid = data.txid ?? data.tx_id ?? data.hash ?? '';
    toast(`📨 New TX ${shortH(txid)}`, 'info');
    updateStat('pkt-stat-mempool', (el) => {
      const n = parseInt(el.textContent, 10);
      if (!isNaN(n)) el.textContent = String(n + 1);
    });
    feedItem(`<a href="#tx/${txid}" class="pk-link">TX ${shortH(txid)}</a>`, 'tx');
  }

  function onStats(data) {
    if (data.height      != null) updateStat('pkt-stat-height',   data.height);
    if (data.peer_count  != null) updateStat('pkt-stat-peers',    data.peer_count);
    if (data.mempool     != null) updateStat('pkt-stat-mempool',  data.mempool);
    if (data.hashrate    != null) updateStat('pkt-stat-hashrate', fmtHashrate(data.hashrate));
  }

  // ── Toast ──────────────────────────────────────────────────────────────────

  function toast(message, level) {
    const container = getToastContainer();
    const id = 'pk-toast-' + (++toastSeq);

    // Hapus toast lama jika sudah MAX_TOASTS
    const existing = container.querySelectorAll('.pk-toast');
    if (existing.length >= MAX_TOASTS) existing[0].remove();

    const el = document.createElement('div');
    el.id        = id;
    el.className = `pk-toast pk-toast-${level}`;
    el.innerHTML = `<span class="pk-toast-msg">${message}</span>
                    <button class="pk-toast-close" onclick="this.parentElement.remove()">×</button>`;
    container.appendChild(el);

    // Animate in
    requestAnimationFrame(() => el.classList.add('pk-toast-show'));

    // Auto-remove
    setTimeout(() => {
      el.classList.remove('pk-toast-show');
      setTimeout(() => el.remove(), 300);
    }, TOAST_TTL_MS);
  }

  function getToastContainer() {
    let el = document.getElementById('pk-toasts');
    if (!el) {
      el = document.createElement('div');
      el.id = 'pk-toasts';
      document.body.appendChild(el);
      injectStyles();
    }
    return el;
  }

  // ── Live feed list ─────────────────────────────────────────────────────────

  function feedItem(html, type) {
    const feed = document.getElementById('pkt-live-feed');
    if (!feed) return;

    const li = document.createElement('li');
    li.className = `pk-feed-item pk-feed-${type}`;
    li.innerHTML = html;
    feed.prepend(li);

    // Giữ tối đa 20 items
    const items = feed.querySelectorAll('li');
    if (items.length > 20) items[items.length - 1].remove();
  }

  // ── DOM helpers ────────────────────────────────────────────────────────────

  function updateStat(id, valueOrFn) {
    const el = document.getElementById(id);
    if (!el) return;
    if (typeof valueOrFn === 'function') {
      valueOrFn(el);
    } else {
      el.textContent = String(valueOrFn);
    }
  }

  function setStatus(state) {
    if (!statusEl) statusEl = document.getElementById('pk-ws-status');
    if (!statusEl) {
      statusEl = createStatusBadge();
    }

    const labels = {
      connecting:    '● Connecting…',
      connected:     '● Live',
      disconnected:  '○ Offline',
      reconnecting:  '● Reconnecting…',
    };
    const classes = {
      connecting:   'pk-ws-connecting',
      connected:    'pk-ws-live',
      disconnected: 'pk-ws-offline',
      reconnecting: 'pk-ws-connecting',
    };

    statusEl.textContent = labels[state] || state;
    statusEl.className   = `pk-ws-badge ${classes[state] || ''}`;
  }

  function createStatusBadge() {
    const el = document.createElement('span');
    el.id = 'pk-ws-status';

    // Inject kế bên tiêu đề hoặc header
    const header = document.querySelector('header, nav, h1, .header');
    if (header) header.appendChild(el);
    else        document.body.appendChild(el);
    return el;
  }

  function injectLiveFeed() {
    if (document.getElementById('pkt-live-feed')) return;
    const section = document.createElement('section');
    section.innerHTML =
      '<h4 class="pk-feed-title">Live Feed</h4>' +
      '<ul id="pkt-live-feed" class="pk-feed-list"></ul>';
    const root = document.querySelector('main, .main-content, body');
    root.appendChild(section);
  }

  // ── Format helpers ─────────────────────────────────────────────────────────

  function shortH(h) {
    return h ? (h.length > 16 ? h.slice(0, 16) + '…' : h) : '';
  }

  function fmtHashrate(khs) {
    if (khs >= 1_000_000) return (khs / 1_000_000).toFixed(2) + ' GH/s';
    if (khs >= 1_000)     return (khs / 1_000).toFixed(2)     + ' MH/s';
    return khs.toFixed(1) + ' kH/s';
  }

  // ── Styles ─────────────────────────────────────────────────────────────────

  function injectStyles() {
    if (document.getElementById('pk-live-css')) return;
    const s = document.createElement('style');
    s.id = 'pk-live-css';
    s.textContent = `
      /* Toast container */
      #pk-toasts { position:fixed; bottom:1.5rem; right:1.5rem; z-index:9999;
                   display:flex; flex-direction:column; gap:.5rem; pointer-events:none; }
      .pk-toast { display:flex; align-items:center; justify-content:space-between;
                  background:#1e293b; border-radius:8px; padding:.65rem 1rem;
                  min-width:220px; max-width:360px; box-shadow:0 4px 20px rgba(0,0,0,.5);
                  opacity:0; transform:translateX(30px);
                  transition:opacity .25s, transform .25s; pointer-events:all; }
      .pk-toast-show { opacity:1; transform:translateX(0); }
      .pk-toast-success { border-left:3px solid #4ade80; }
      .pk-toast-info    { border-left:3px solid #60a5fa; }
      .pk-toast-warning { border-left:3px solid #fbbf24; }
      .pk-toast-error   { border-left:3px solid #f87171; }
      .pk-toast-msg  { font-size:.85rem; color:#e2e8f0; }
      .pk-toast-close { background:none; border:none; color:#64748b;
                        cursor:pointer; font-size:1rem; padding:0 0 0 .75rem; }

      /* Status badge */
      .pk-ws-badge { font-size:.72rem; padding:.2rem .55rem; border-radius:4px;
                     margin-left:.75rem; white-space:nowrap; }
      .pk-ws-live       { background:#166534; color:#4ade80; }
      .pk-ws-connecting { background:#713f12; color:#fbbf24; animation:pk-blink 1s infinite; }
      .pk-ws-offline    { background:#1e1e1e; color:#64748b; }
      @keyframes pk-blink { 0%,100%{opacity:1} 50%{opacity:.4} }

      /* Live feed */
      .pk-feed-title { font-size:.78rem; color:#94a3b8; text-transform:uppercase;
                       letter-spacing:.08em; margin:1rem 0 .4rem; }
      .pk-feed-list  { list-style:none; padding:0; margin:0; }
      .pk-feed-item  { font-size:.82rem; color:#94a3b8; padding:.3rem 0;
                       border-bottom:1px solid #1e293b; display:flex; gap:.75rem; align-items:center; }
      .pk-feed-block::before { content:'⛏'; }
      .pk-feed-tx::before    { content:'📨'; }
      .pk-feed-hash { color:#475569; font-family:monospace; font-size:.75rem; }
      .pk-link { color:#60a5fa; text-decoration:none; }
      .pk-link:hover { text-decoration:underline; }
    `;
    document.head.appendChild(s);
  }

  // ── Init ───────────────────────────────────────────────────────────────────

  function init() {
    injectStyles();
    injectLiveFeed();
    connect();
  }

  // expose API cho các script khác
  window.pktLive = {
    connect,
    toast,
    isConnected: () => ws?.readyState === WebSocket.OPEN,
  };

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }

})();
