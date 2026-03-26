// block-detail.js — v18.4 Block Detail Enhanced
// GET /api/testnet/block/:height → header + analytics + tx list
'use strict';

const API_BASE = '/blockchain-rust';

function shortHash(h) { return h ? h.slice(0, 10) + '…' + h.slice(-8) : '—'; }
function fmtHashrate(h) {
  if (!h) return '—';
  if (h >= 1e15) return (h / 1e15).toFixed(2) + ' PH/s';
  if (h >= 1e12) return (h / 1e12).toFixed(2) + ' TH/s';
  if (h >= 1e9)  return (h / 1e9).toFixed(2)  + ' GH/s';
  if (h >= 1e6)  return (h / 1e6).toFixed(2)  + ' MH/s';
  return h.toFixed(0) + ' H/s';
}

async function init() {
  const parts  = window.location.pathname.split('/').filter(Boolean);
  const height = parts[parts.length - 1];
  const el     = document.getElementById('block-content');

  if (!height || isNaN(Number(height))) {
    el.innerHTML = `<a class="detail-back" href="${API_BASE}/block">← All Blocks</a>
      <div class="panel" style="padding:24px;color:var(--red)">Invalid block height.</div>`;
    return;
  }

  const h = Number(height);
  document.title = `Block #${h.toLocaleString()} — PKTScan`;
  el.innerHTML = `<a class="detail-back" href="${API_BASE}/block">← All Blocks</a>
    <div style="padding:40px;text-align:center;color:var(--muted)">Loading…</div>`;

  // Fetch testnet block (primary)
  let b;
  try {
    const r = await fetch(`${API_BASE}/api/testnet/block/${h}`);
    if (!r.ok) throw new Error(r.status);
    b = await r.json();
  } catch (_) {
    el.innerHTML = `<a class="detail-back" href="${API_BASE}/block">← All Blocks</a>
      <div class="panel" style="padding:24px;color:var(--red)">Block #${h.toLocaleString()} not found or node offline.</div>`;
    return;
  }

  if (b.error) {
    el.innerHTML = `<a class="detail-back" href="${API_BASE}/block">← All Blocks</a>
      <div class="panel" style="padding:24px;color:var(--red)">${b.error}</div>`;
    return;
  }

  const ts = b.timestamp
    ? new Date(b.timestamp * 1000).toISOString().replace('T', ' ').slice(0, 19) + ' UTC'
    : '—';
  const blockTime = b.block_time_secs != null ? b.block_time_secs.toFixed(1) + 's' : '—';
  const diff      = b.difficulty != null ? b.difficulty.toFixed(4) : '—';
  const hashrate  = fmtHashrate(b.hashrate);
  const confirms  = b.confirmations != null ? b.confirmations.toLocaleString() : '—';
  const txids     = b.txids || [];

  // ── Overview KV ──────────────────────────────────────────────────────────
  const kv = [
    ['Height',        `<span class="normal">${h.toLocaleString()}</span>`],
    ['Hash',          `<span class="mono" style="word-break:break-all;font-size:.8rem">${b.hash || '—'}</span>`],
    ['Previous',      b.prev_hash
      ? `<a href="${API_BASE}/block/${h - 1}" class="mono" style="color:var(--blue);font-size:.8rem;word-break:break-all">${b.prev_hash}</a>`
      : '—'],
    ['Merkle Root',   `<span class="mono" style="font-size:.78rem;word-break:break-all;color:var(--muted)">${b.merkle_root || '—'}</span>`],
    ['Timestamp',     `<span class="normal">${ts}</span>`],
    ['Block Time',    `<span class="normal ${b.block_time_secs > 120 ? 'red' : b.block_time_secs < 30 ? '' : 'green'}">${blockTime}</span>`],
    ['Confirmations', `<span class="normal">${confirms}</span>`],
    ['Difficulty',    `<span class="normal">${diff}</span>`],
    ['Hashrate',      `<span class="normal">${hashrate}</span>`],
    ['Bits',          `<span class="mono">${b.bits != null ? '0x' + (b.bits >>> 0).toString(16).padStart(8, '0') : '—'}</span>`],
    ['Nonce',         `<span class="normal">${b.nonce != null ? b.nonce.toLocaleString() : '—'}</span>`],
    ['Version',       `<span class="normal">${b.version != null ? b.version : '—'}</span>`],
    ['Transactions',  `<span class="normal">${txids.length > 0 ? txids.length : (b.tx_count ?? '—')}</span>`],
  ].map(([k, v]) => `
    <div class="kv-row">
      <div class="kv-key">${k}</div>
      <div class="kv-val">${v}</div>
    </div>`).join('');

  // ── TX list ───────────────────────────────────────────────────────────────
  let txHtml;
  if (txids.length === 0) {
    txHtml = `<div style="padding:14px 18px;color:var(--muted);font-size:.85rem">
      No transactions indexed for this block yet.<br>
      <span style="font-size:.78rem">Transactions are indexed on new blocks after sync resumes.</span>
    </div>`;
  } else {
    txHtml = txids.map(txid => `
      <div class="list-item">
        <div class="item-icon item-icon-tx" style="font-size:.7rem">TX</div>
        <div class="item-main">
          <a href="${API_BASE}/rx/${txid}" class="item-primary mono" style="font-size:.82rem;color:var(--blue)">${shortHash(txid)}</a>
          <div class="item-secondary mono" style="font-size:.72rem">${txid}</div>
        </div>
      </div>`).join('');
  }

  // ── Nav prev/next ─────────────────────────────────────────────────────────
  const nav = `
    <div style="display:flex;gap:12px;margin-top:16px">
      ${h > 0 ? `<a href="${API_BASE}/block/${h - 1}" style="color:var(--blue);font-size:.85rem">← Block #${(h - 1).toLocaleString()}</a>` : ''}
      <a href="${API_BASE}/block/${h + 1}" style="color:var(--blue);font-size:.85rem;margin-left:auto">Block #${(h + 1).toLocaleString()} →</a>
    </div>`;

  el.innerHTML = `
    <a class="detail-back" href="${API_BASE}/block">← All Blocks</a>

    <div class="detail-title">
      📦 Block <span style="color:var(--blue)">#${h.toLocaleString()}</span>
    </div>

    <div class="panel" style="margin-bottom:20px">${kv}</div>

    <div class="panel" style="margin-bottom:16px">
      <div class="panel-head">
        <div class="panel-title">
          <div class="panel-title-icon item-icon-tx">💸</div>
          Transactions
          <span style="color:var(--muted);font-size:.8rem;margin-left:6px">(${txids.length})</span>
        </div>
      </div>
      ${txHtml}
    </div>

    ${nav}`;
}

document.addEventListener('DOMContentLoaded', init);
