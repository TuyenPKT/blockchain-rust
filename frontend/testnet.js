// testnet.js — v15.6 PKT Testnet Web Integration
// Fetches from /api/testnet/* (real testnet chain data via RocksDB) and
// updates the #testnet-page elements injected into index.html.
// Loaded via <script src="/static/testnet.js"> at the bottom of index.html.

(function () {
  'use strict';

  // ── Helpers ──────────────────────────────────────────────────────────────────

  function shortHash(h, n) {
    n = n || 16;
    if (!h) return '\u2014';
    return h.slice(0, n) + '\u2026';
  }

  function tsToDate(ts) {
    if (!ts) return '\u2014';
    return new Date(ts * 1000).toISOString().replace('T', ' ').slice(0, 19) + ' UTC';
  }

  function renderProgressBar(pct, width) {
    width = width || 20;
    pct   = Math.min(100, Math.max(0, pct || 0));
    var filled = Math.round((pct / 100) * width);
    var empty  = width - filled;
    var bar = '';
    for (var i = 0; i < filled; i++) bar += '\u2588';   // █
    for (var i = 0; i < empty;  i++) bar += '\u2591';   // ░
    return bar + ' ' + pct + '%';
  }

  function setHtml(id, html) {
    var e = document.getElementById(id);
    if (e) e.innerHTML = html;
  }

  function setText(id, text) {
    var e = document.getElementById(id);
    if (e) e.textContent = text;
  }

  // ── Fetch sync status ─────────────────────────────────────────────────────────

  async function fetchSyncStatus() {
    try {
      var r = await fetch('api/testnet/sync-status');
      if (!r.ok) throw new Error(r.status);
      var d = await r.json();

      var pct    = d.overall_progress_pct || 0;
      var phase  = d.phase || 'Unknown';
      var done   = !!d.phase_complete;
      var color  = done ? 'var(--green)' : pct > 0 ? 'var(--pkt)' : 'var(--muted)';

      setHtml('tn-sync-phase',
        '<span style="color:' + color + ';font-weight:700">' + phase + '</span>');
      setText('tn-sync-bar',    renderProgressBar(pct));
      setText('tn-headers-count', (d.headers_downloaded || 0).toLocaleString());
      setText('tn-utxo-height',   (d.utxo_height        || 0).toLocaleString());
      setText('tn-speed',
        d.blocks_per_sec > 0 ? d.blocks_per_sec.toFixed(1) + ' blk/s' : '\u2014');
      setText('tn-eta', d.eta || '\u2014');

      setHtml('tn-stat-synced', done
        ? '<span style="color:var(--green);font-size:.82rem;font-weight:600">\u25cf Synced</span>'
        : '<span style="color:var(--pkt);font-size:.82rem;font-weight:600">\u27f3 Syncing</span>');
    } catch (_) {
      setHtml('tn-sync-phase',
        '<span style="color:var(--muted)">Not connected \u2014 run: cargo run -- sync</span>');
      setHtml('tn-stat-synced',
        '<span style="color:var(--muted);font-size:.82rem">\u25cb Offline</span>');
    }
  }

  // ── Fetch testnet stats ────────────────────────────────────────────────────────

  async function fetchTestnetStats() {
    try {
      var r = await fetch('api/testnet/stats');
      if (!r.ok) throw new Error(r.status);
      var d = await r.json();

      setText('tn-stat-height', (d.header_height || 0).toLocaleString());
      setText('tn-stat-utxos',  (d.utxo_count    || 0).toLocaleString());

      // 1 PKT = 2^30 paklets = 1,073,741,824
      var pkt = ((d.total_value || 0) / 1073741824).toFixed(2);
      setText('tn-stat-value', pkt + ' PKT');
    } catch (_) {
      setText('tn-stat-height', '\u2014');
      setText('tn-stat-utxos',  '\u2014');
      setText('tn-stat-value',  '\u2014');
    }
  }

  // ── Fetch recent headers ───────────────────────────────────────────────────────

  async function fetchTestnetHeaders() {
    var el = document.getElementById('tn-headers-list');
    if (!el) return;
    try {
      var r = await fetch('api/testnet/headers?limit=5');
      if (!r.ok) throw new Error(r.status);
      var d = await r.json();
      var headers = d.headers || [];

      if (!headers.length) {
        el.innerHTML =
          '<div style="padding:12px 18px;color:var(--muted);font-size:.85rem">' +
          'No headers synced yet \u2014 run: <span class="mono">cargo run -- sync</span></div>';
        return;
      }

      el.innerHTML = headers.map(function (h) {
        return (
          '<div class="list-item block-item" style="cursor:default">' +
            '<div class="item-icon item-icon-block" style="font-size:.7rem">#' +
              ((h.height || 0) % 1000) +
            '</div>' +
            '<div class="item-main">' +
              '<div class="item-primary">' + shortHash(h.hash || '') + '</div>' +
              '<div class="item-secondary">' +
                'h=' + (h.height || '?') + ' &nbsp;\u00b7&nbsp; bits=' + (h.bits || '?') +
              '</div>' +
            '</div>' +
            '<div class="item-right">' +
              '<div class="item-age">' + tsToDate(h.timestamp) + '</div>' +
            '</div>' +
          '</div>'
        );
      }).join('');
    } catch (_) {
      el.innerHTML =
        '<div style="padding:12px 18px;color:var(--muted)">Unable to load headers</div>';
    }
  }

  // ── Refresh all ────────────────────────────────────────────────────────────────

  function refreshTestnet() {
    fetchSyncStatus();
    fetchTestnetStats();
    fetchTestnetHeaders();
  }

  // ── Page show / auto-refresh ───────────────────────────────────────────────────

  var tnTimer = null;

  window.showTestnet = function () {
    if (typeof hideAll === 'function') hideAll();
    var page = document.getElementById('testnet-page');
    if (page) page.classList.add('active');
    refreshTestnet();
    if (tnTimer) clearInterval(tnTimer);
    tnTimer = setInterval(refreshTestnet, 15000);
  };

  // Stop auto-refresh when any other nav link is clicked
  document.addEventListener('click', function (e) {
    var a = e.target.closest ? e.target.closest('a.nav-link') : null;
    if (!a) return;
    var onclick = a.getAttribute('onclick') || '';
    if (!onclick.includes('showTestnet') && tnTimer) {
      clearInterval(tnTimer);
      tnTimer = null;
    }
  });

})();
