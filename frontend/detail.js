// v14.6 — PKT Block & TX Detail Pages
// Hash-router: #block/N → block detail | #tx/TXID → tx detail
// Tự inject vào trang, không cần build step.

(function () {
  'use strict';

  const PAKLETS = 1_073_741_824; // 2^30

  // ── Hash-router ────────────────────────────────────────────────────────────

  function route(hash) {
    const mBlock = hash.match(/^#block\/(\d+)$/);
    const mTx    = hash.match(/^#tx\/([a-fA-F0-9]{8,})$/);

    if      (mBlock) showBlock(parseInt(mBlock[1], 10));
    else if (mTx)    showTx(mTx[1]);
    else             hidePanel();
  }

  window.addEventListener('hashchange', () => route(location.hash));
  if (location.hash) route(location.hash);

  // ── Block detail ───────────────────────────────────────────────────────────

  async function showBlock(height) {
    showLoading(`Block #${height}`);
    const data = await fetchJson(`/api/chain/${height}`);
    if (!data) { showError(`Block #${height} không tìm thấy`); return; }
    getPanel().innerHTML = renderBlock(data, height);
    injectStyles();
  }

  function renderBlock(b, height) {
    const txs    = b.transactions || b.txs || [];
    const hash   = b.hash || b.block_hash || '';
    const prev   = b.prev_hash || b.previous_hash || '';
    const merkle = b.merkle_root || '';
    const ts     = b.timestamp ? fmtTs(b.timestamp) : 'N/A';

    const txRows = txs.map(tx => {
      const id  = tx.tx_id || tx.txid || '';
      const out = (tx.outputs || []).reduce((s, o) => s + (o.value || 0), 0);
      return `<tr>
        <td><a href="#tx/${id}" class="pk-link">${shortH(id)}</a></td>
        <td class="pk-num">${(tx.outputs || []).length}</td>
        <td class="pk-num">${fmtPkt(out)}</td>
      </tr>`;
    }).join('');

    return header('Block', `#${height}`) +
      `<div class="pk-grid">
        ${fld('Height',      height)}
        ${fld('Hash',        `<span class="pk-mono">${shortH(hash)}</span>`)}
        ${fld('Prev Hash',   `<span class="pk-mono">${shortH(prev)}</span>`)}
        ${fld('Merkle Root', `<span class="pk-mono">${shortH(merkle)}</span>`)}
        ${fld('Timestamp',   ts)}
        ${fld('Nonce',       b.nonce || 0)}
        ${fld('Difficulty',  b.difficulty || 'N/A')}
        ${fld('TX Count',    txs.length)}
      </div>` +
      (txs.length > 0
        ? `<h4 class="pk-section">Transactions (${txs.length})</h4>
           <table class="pk-table">
             <thead><tr><th>TXID</th><th>Outputs</th><th>Amount</th></tr></thead>
             <tbody>${txRows}</tbody>
           </table>`
        : '');
  }

  // ── TX detail ──────────────────────────────────────────────────────────────

  async function showTx(txid) {
    showLoading(`TX ${shortH(txid)}`);
    const data = await fetchJson(`/api/tx/${txid}`);
    if (!data) { showError(`TX ${shortH(txid)} không tìm thấy`); return; }
    getPanel().innerHTML = renderTx(data, txid);
    injectStyles();
  }

  function renderTx(tx, txid) {
    const inputs  = tx.inputs  || tx.tx?.inputs  || [];
    const outputs = tx.outputs || tx.tx?.outputs || [];
    const totalOut = outputs.reduce((s, o) => s + (o.value || 0), 0);
    const blockH   = tx.block_height ?? tx.height ?? null;
    const confs    = tx.confirmations ?? 0;
    const status   = tx.status || (confs > 0 ? 'confirmed' : 'pending');
    const badge    = status === 'confirmed'
      ? '<span class="pk-badge pk-green">confirmed</span>'
      : '<span class="pk-badge pk-yellow">pending</span>';

    const inRows = inputs.map(inp => {
      const prev = inp.prev_tx_id || inp.previous_output_tx_id || '(coinbase)';
      const idx  = inp.output_index ?? inp.prev_output_index ?? '-';
      return `<tr>
        <td class="pk-mono">${shortH(prev)}</td>
        <td class="pk-num">${idx}</td>
      </tr>`;
    }).join('');

    const outRows = outputs.map((o, i) => {
      const addr = o.address || o.script_pubkey || '';
      return `<tr>
        <td class="pk-num">${i}</td>
        <td class="pk-mono">${shortH(addr)}</td>
        <td class="pk-num">${fmtPkt(o.value || 0)}</td>
      </tr>`;
    }).join('');

    const blockLink = blockH !== null
      ? `<a href="#block/${blockH}" class="pk-link">#${blockH}</a>` : 'N/A';

    return header('Transaction', shortH(txid)) +
      `<div class="pk-grid">
        ${fld('TXID',         `<span class="pk-mono">${shortH(txid)}</span>`)}
        ${fld('Block',        blockLink)}
        ${fld('Status',       badge)}
        ${fld('Confirmations', confs)}
        ${fld('Total Output', fmtPkt(totalOut))}
        ${fld('Inputs',       inputs.length)}
        ${fld('Outputs',      outputs.length)}
      </div>` +
      (inputs.length > 0
        ? `<h4 class="pk-section">Inputs (${inputs.length})</h4>
           <table class="pk-table">
             <thead><tr><th>Prev TX</th><th>Index</th></tr></thead>
             <tbody>${inRows}</tbody>
           </table>`
        : '') +
      `<h4 class="pk-section">Outputs (${outputs.length})</h4>
       <table class="pk-table">
         <thead><tr><th>#</th><th>Address / ScriptPubKey</th><th>Amount</th></tr></thead>
         <tbody>${outRows}</tbody>
       </table>`;
  }

  // ── DOM helpers ────────────────────────────────────────────────────────────

  function getPanel() {
    let el = document.getElementById('pk-detail');
    if (!el) {
      el = document.createElement('div');
      el.id = 'pk-detail';
      const root = document.querySelector('main, .main-content, body');
      root.prepend(el);
    }
    el.style.display = 'block';
    el.scrollIntoView({ behavior: 'smooth' });
    return el;
  }

  function hidePanel() {
    const el = document.getElementById('pk-detail');
    if (el) el.style.display = 'none';
  }

  function showLoading(label) {
    getPanel().innerHTML = `<div class="pk-loading">Loading ${label}…</div>`;
  }

  function showError(msg) {
    getPanel().innerHTML = `<div class="pk-error">${msg}</div>`;
  }

  async function fetchJson(url) {
    try {
      const res = await fetch(url);
      if (!res.ok) return null;
      return await res.json();
    } catch (_) { return null; }
  }

  // ── HTML helpers ───────────────────────────────────────────────────────────

  function header(type, id) {
    return `<div class="pk-header">
      <button onclick="history.back()" class="pk-back">← Back</button>
      <span class="pk-type">${type}</span>
      <span class="pk-id">${id}</span>
    </div>`;
  }

  function fld(label, value) {
    return `<div class="pk-field">
      <span class="pk-label">${label}</span>
      <span class="pk-value">${value}</span>
    </div>`;
  }

  function shortH(h) {
    if (!h) return '';
    return h.length > 20 ? h.slice(0, 20) + '…' : h;
  }

  function fmtPkt(paklets) {
    return (paklets / PAKLETS).toFixed(4) + ' PKT';
  }

  function fmtTs(ts) {
    try {
      return new Date(ts * 1000).toISOString().replace('T', ' ').slice(0, 19) + ' UTC';
    } catch (_) { return String(ts); }
  }

  // ── Styles ─────────────────────────────────────────────────────────────────

  function injectStyles() {
    if (document.getElementById('pk-detail-css')) return;
    const s = document.createElement('style');
    s.id = 'pk-detail-css';
    s.textContent = `
      #pk-detail { background:#1e293b; border-radius:10px; padding:1.5rem; margin-bottom:1.5rem; }
      .pk-header { display:flex; align-items:center; gap:1rem; margin-bottom:1.25rem; }
      .pk-back { background:#334155; color:#94a3b8; border:none; border-radius:5px;
                 padding:.35rem .75rem; cursor:pointer; font-size:.85rem; }
      .pk-back:hover { background:#475569; }
      .pk-type { font-size:.7rem; color:#64748b; text-transform:uppercase; letter-spacing:.08em; }
      .pk-id   { font-family:monospace; color:#94a3b8; font-size:.9rem; }
      .pk-grid { display:grid; grid-template-columns:repeat(auto-fit,minmax(200px,1fr));
                 gap:.5rem 1.5rem; margin-bottom:1rem; }
      .pk-field { display:flex; flex-direction:column; }
      .pk-label { font-size:.7rem; color:#64748b; text-transform:uppercase; letter-spacing:.05em; }
      .pk-value { font-size:.88rem; color:#e2e8f0; margin-top:.1rem; }
      .pk-mono  { font-family:monospace; word-break:break-all; }
      .pk-section { color:#94a3b8; font-size:.78rem; text-transform:uppercase;
                    letter-spacing:.08em; margin:1rem 0 .5rem; }
      .pk-table { width:100%; border-collapse:collapse; font-size:.85rem; }
      .pk-table th { color:#64748b; font-weight:500; text-align:left;
                     padding:.4rem .6rem; border-bottom:1px solid #334155; }
      .pk-table td { color:#cbd5e1; padding:.35rem .6rem; border-bottom:1px solid #0f172a; }
      .pk-num  { text-align:right; }
      .pk-link { color:#60a5fa; text-decoration:none; }
      .pk-link:hover { text-decoration:underline; }
      .pk-badge { font-size:.72rem; padding:.15rem .5rem; border-radius:4px; }
      .pk-green  { background:#166534; color:#4ade80; }
      .pk-yellow { background:#713f12; color:#fbbf24; }
      .pk-loading { color:#94a3b8; padding:1rem; font-style:italic; }
      .pk-error   { color:#f87171; padding:1rem; }
    `;
    document.head.appendChild(s);
  }

})();
