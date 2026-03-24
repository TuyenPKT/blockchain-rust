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

  // ── Address → script_pubkey conversion ───────────────────────────────────────

  var B58_ALPHA = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';

  function b58Decode(s) {
    var n = BigInt(0);
    for (var i = 0; i < s.length; i++) {
      var idx = B58_ALPHA.indexOf(s[i]);
      if (idx < 0) return null;
      n = n * BigInt(58) + BigInt(idx);
    }
    var hex = n.toString(16);
    if (hex.length % 2) hex = '0' + hex;
    var bytes = [];
    for (var j = 0; j < hex.length; j += 2) {
      bytes.push(parseInt(hex.slice(j, j + 2), 16));
    }
    // Prepend leading zero-bytes for each leading '1' in input
    var leading = 0;
    while (leading < s.length && s[leading] === '1') leading++;
    var result = new Array(leading).fill(0).concat(bytes);
    return result;
  }

  /// Convert PKT/Bitcoin address or script_pubkey hex → script_pubkey hex.
  /// Returns null if format not recognised.
  function addrToScriptHex(addr) {
    addr = addr.trim();
    if (!addr) return null;

    // Already a script_pubkey hex → use as-is
    if (/^[0-9a-fA-F]+$/.test(addr) && addr.length >= 10) return addr.toLowerCase();

    // Base58Check P2PKH (starts with 1, m, n, p…)
    if (/^[1mnpP]/.test(addr)) {
      var decoded = b58Decode(addr);
      if (!decoded || decoded.length !== 25) return null;
      // [version:1] [hash160:20] [checksum:4]
      var hash160 = decoded.slice(1, 21)
        .map(function (b) { return b.toString(16).padStart(2, '0'); }).join('');
      return '76a914' + hash160 + '88ac';
    }

    return null;
  }

  /// Expose globally so other panels can trigger address lookup by clicking.
  window.tnLookupByAddr = function (addr) {
    var input = document.getElementById('tn-addr-input');
    if (input) input.value = addr;
    // Scroll to the address panel
    var panel = input && input.closest ? input.closest('.panel') : null;
    if (panel) panel.scrollIntoView({ behavior: 'smooth', block: 'start' });
    window.tnLookupAddress();
  };

  // ── Address Lookup ────────────────────────────────────────────────────────────

  var tnAddrScript = '';
  var tnAddrCursor = null;

  window.tnLookupAddress = async function () {
    var input = document.getElementById('tn-addr-input');
    if (!input) return;
    var raw    = input.value.trim();
    if (!raw) return;

    // Auto-convert human-readable address → script_pubkey hex
    var script = addrToScriptHex(raw);
    if (!script) {
      setHtml('tn-addr-balance',
        '<div style="color:#ff6b6b;font-size:.85rem;padding:6px 0">' +
        'Địa chỉ không hợp lệ &mdash; cần script_pubkey hex hoặc địa chỉ Base58 (bắt đầu với 1, m, n)</div>');
      setHtml('tn-addr-txs', '');
      return;
    }
    tnAddrScript = script;
    tnAddrCursor = null;

    // Show converted script if input was a human address
    var addrLabel = (raw !== script)
      ? '<div class="mono" style="font-size:.76rem;color:var(--muted);margin-bottom:10px">' +
        'script: ' + script + '</div>'
      : '';

    setHtml('tn-addr-balance', addrLabel);
    setHtml('tn-addr-txs', '<div style="color:var(--muted);font-size:.85rem">Loading…</div>');
    var moreEl = document.getElementById('tn-addr-more');
    if (moreEl) moreEl.style.display = 'none';

    // Fetch balance
    try {
      var rb = await fetch('api/testnet/balance/' + encodeURIComponent(script));
      if (rb.ok) {
        var db = await rb.json();
        var bal     = db.balance || 0;
        var pktAmt  = (bal / 1073741824).toFixed(8);
        setText('tn-addr-badge', bal > 0 ? pktAmt + ' PKT' : '');
        setHtml('tn-addr-balance', addrLabel +
          '<div style="display:flex;align-items:baseline;gap:10px;padding:8px 0 14px;border-bottom:1px solid var(--border);margin-bottom:12px">' +
            '<span style="font-size:1.3rem;font-weight:700;color:var(--pkt)">' + pktAmt + ' PKT</span>' +
            '<span style="color:var(--muted);font-size:.8rem">' + bal.toLocaleString() + ' paklets</span>' +
          '</div>');
      }
    } catch (_) {}

    await tnFetchAddrTxs(true);
  };

  async function tnFetchAddrTxs(reset) {
    var script = tnAddrScript;
    if (!script) return;
    var url = 'api/testnet/address/' + encodeURIComponent(script) + '/txs?limit=20';
    if (tnAddrCursor !== null) url += '&cursor=' + tnAddrCursor;

    try {
      var r = await fetch(url);
      if (!r.ok) throw new Error(r.status);
      var d = await r.json();
      var txs = d.txs || [];

      if (!txs.length && reset) {
        setHtml('tn-addr-txs', '<div style="color:var(--muted);font-size:.85rem">No transactions found</div>');
        var me = document.getElementById('tn-addr-more');
        if (me) me.style.display = 'none';
        return;
      }

      var rows = txs.map(function (tx) {
        return '<div class="list-item" style="cursor:default">' +
          '<div class="item-icon item-icon-block" style="font-size:.72rem">#' + (tx.height % 1000) + '</div>' +
          '<div class="item-main">' +
            '<div class="item-primary mono" style="font-size:.8rem">' + (tx.txid || '') + '</div>' +
            '<div class="item-secondary">block ' + tx.height + '</div>' +
          '</div>' +
        '</div>';
      }).join('');

      var txsEl = document.getElementById('tn-addr-txs');
      if (txsEl) {
        if (reset) { txsEl.innerHTML = rows; }
        else        { txsEl.innerHTML += rows; }
      }

      var moreEl = document.getElementById('tn-addr-more');
      if (txs.length >= 20) {
        tnAddrCursor = txs[txs.length - 1].height;
        if (moreEl) moreEl.style.display = 'block';
      } else {
        if (moreEl) moreEl.style.display = 'none';
      }
    } catch (_) {
      if (reset) {
        setHtml('tn-addr-txs', '<div style="color:var(--muted);font-size:.85rem">Failed to load transactions</div>');
      }
    }
  }

  window.tnLoadMoreTxs = function () { tnFetchAddrTxs(false); };

  // Allow Enter key on address input
  document.addEventListener('DOMContentLoaded', function () {
    var inp = document.getElementById('tn-addr-input');
    if (inp) {
      inp.addEventListener('keydown', function (e) {
        if (e.key === 'Enter') window.tnLookupAddress();
      });
    }
  });

  // ── Rich List ─────────────────────────────────────────────────────────────────

  async function fetchRichList() {
    var el = document.getElementById('tn-rich-list');
    if (!el) return;
    try {
      var r = await fetch('api/testnet/rich-list?limit=20');
      if (!r.ok) throw new Error(r.status);
      var d = await r.json();
      var holders = d.holders || [];

      var countEl = document.getElementById('tn-rich-count');
      if (countEl) countEl.textContent = (d.count || 0) + ' addresses';

      if (!holders.length) {
        el.innerHTML =
          '<div style="padding:12px 18px;color:var(--muted);font-size:.85rem">' +
          'No address data yet \u2014 run: <span class="mono">cargo run -- sync</span></div>';
        return;
      }

      el.innerHTML = holders.map(function (h, i) {
        var pkt     = (h.balance_pkt || 0).toFixed(2);
        var full    = h.script || '';
        var display = shortHash(full, 20);
        var escaped = full.replace(/'/g, "\\'");
        return '<div class="list-item" style="cursor:pointer" onclick="tnLookupByAddr(\'' + escaped + '\')" title="Click to look up">' +
          '<div class="item-icon" style="background:rgba(255,215,0,.1);color:#ffd700;' +
          'border:1px solid rgba(255,215,0,.25);font-size:.78rem;font-weight:700">' + (i + 1) + '</div>' +
          '<div class="item-main">' +
            '<div class="item-primary mono" style="font-size:.8rem">' + display + '</div>' +
            '<div class="item-secondary">' + (h.balance || 0).toLocaleString() + ' paklets</div>' +
          '</div>' +
          '<div class="item-right">' +
            '<div class="item-age" style="color:var(--pkt);font-weight:600;font-size:.9rem">' + pkt + ' PKT</div>' +
          '</div>' +
        '</div>';
      }).join('');
    } catch (_) {
      el.innerHTML = '<div style="padding:12px 18px;color:var(--muted)">Unable to load rich list</div>';
    }
  }

  // ── Mempool ───────────────────────────────────────────────────────────────────

  function timeAgoSecs(ts) {
    var diff = Math.floor(Date.now() / 1000) - ts;
    if (diff < 0)    return 'just now';
    if (diff < 60)   return diff + 's ago';
    if (diff < 3600) return Math.floor(diff / 60) + 'm ago';
    return Math.floor(diff / 3600) + 'h ago';
  }

  async function fetchMempool() {
    var el = document.getElementById('tn-mempool-list');
    if (!el) return;
    try {
      var r = await fetch('api/testnet/mempool?limit=20');
      if (!r.ok) throw new Error(r.status);
      var d = await r.json();
      var txs   = d.txs || [];
      var count = d.count || 0;

      var countEl = document.getElementById('tn-mempool-count');
      if (countEl) countEl.textContent = count + ' pending';

      if (!txs.length) {
        el.innerHTML = '<div style="padding:12px 18px;color:var(--muted);font-size:.85rem">Mempool empty</div>';
        return;
      }

      el.innerHTML = txs.map(function (tx) {
        var feeRate = (tx.fee_rate_msat_vb / 1000).toFixed(3);
        var age     = tx.ts_secs ? timeAgoSecs(tx.ts_secs) : '\u2014';
        return '<div class="list-item" style="cursor:default">' +
          '<div class="item-icon" style="background:rgba(100,255,200,.08);color:#64ffc8;' +
          'border:1px solid rgba(100,255,200,.2);font-size:.72rem;font-weight:600">TX</div>' +
          '<div class="item-main">' +
            '<div class="item-primary mono" style="font-size:.8rem">' + shortHash(tx.txid, 24) + '</div>' +
            '<div class="item-secondary">' + tx.size + '\u202fB &nbsp;\u00b7&nbsp; ' + feeRate + '\u202fsat/vB</div>' +
          '</div>' +
          '<div class="item-right">' +
            '<div class="item-age">' + age + '</div>' +
          '</div>' +
        '</div>';
      }).join('');
    } catch (_) {
      el.innerHTML = '<div style="padding:12px 18px;color:var(--muted)">Mempool unavailable</div>';
    }
  }

  // ── Refresh all ────────────────────────────────────────────────────────────────

  function refreshTestnet() {
    fetchSyncStatus();
    fetchTestnetStats();
    fetchTestnetHeaders();
    fetchRichList();
    fetchMempool();
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
