// rx-detail.js — v18.3 TX Detail Page
// GET /api/testnet/tx/:txid → inputs/outputs table, fee rate, confirmations
'use strict';

const API_BASE = '/blockchain-rust';

function shortHash(h) { return h ? h.slice(0, 10) + '…' + h.slice(-8) : '—'; }
function pkt(v)  { return v != null ? (v / 1_073_741_824).toFixed(8) + ' PKT' : '—'; }
function msat(v) { return v != null ? (v / 1000).toFixed(3) + ' sat/vB' : '—'; }

function addrHtml(addr) {
  if (!addr) return '<span style="color:var(--muted)">—</span>';
  const short = addr.length > 28 ? addr.slice(0, 14) + '…' + addr.slice(-10) : addr;
  const href  = `${API_BASE}/address/${encodeURIComponent(addr)}`;
  return `<a href="${href}" class="mono" style="color:var(--blue);font-size:.82rem">${short}</a>`;
}

function statusBadge(status) {
  if (status === 'mempool')   return '<span class="badge" style="background:rgba(247,161,51,.15);color:var(--pkt);border:1px solid rgba(247,161,51,.3)">⏳ Mempool</span>';
  if (status === 'confirmed') return '<span class="badge badge-coinbase">✅ Confirmed</span>';
  return '';
}

async function init() {
  const parts = window.location.pathname.split('/').filter(Boolean);
  const txid  = parts[parts.length - 1];
  const el    = document.getElementById('tx-content');

  if (!txid || txid === 'detail.html') {
    el.innerHTML = `<a class="detail-back" href="${API_BASE}/rx">← All Transactions</a>
      <div class="panel" style="padding:24px;color:var(--red)">No transaction ID specified.</div>`;
    return;
  }

  document.title = `TX ${txid.slice(0, 12)}… — PKTScan`;
  el.innerHTML = `<a class="detail-back" href="${API_BASE}/rx">← All Transactions</a>
    <div style="padding:40px;text-align:center;color:var(--muted)">Loading…</div>`;

  let t;
  try {
    const r = await fetch(`${API_BASE}/api/testnet/tx/${txid}`);
    if (!r.ok) throw new Error(r.status);
    t = await r.json();
  } catch (_) {
    el.innerHTML = `<a class="detail-back" href="${API_BASE}/rx">← All Transactions</a>
      <div class="panel" style="padding:24px;color:var(--red)">Transaction not found or node offline.</div>`;
    return;
  }

  if (t.error) {
    el.innerHTML = `<a class="detail-back" href="${API_BASE}/rx">← All Transactions</a>
      <div class="panel" style="padding:24px;color:var(--red)">${t.error}</div>`;
    return;
  }

  const ts = t.timestamp
    ? new Date(t.timestamp * 1000).toISOString().replace('T', ' ').slice(0, 19) + ' UTC'
    : '—';

  // ── Overview KV rows ──────────────────────────────────────────────────────
  const kvRows = [
    ['TxID',         `<span class="mono" style="word-break:break-all;font-size:.82rem">${t.txid}</span>`],
    ['Status',       statusBadge(t.status)],
    ['Type',         t.is_coinbase ? '<span class="badge badge-coinbase">coinbase</span>' : '<span class="badge badge-tx">transfer</span>'],
    ['Time',         `<span class="normal">${ts}</span>`],
    ['Size',         t.size != null ? `<span class="normal">${t.size} bytes</span>` : '—'],
    ['Fee Rate',     `<span class="normal">${msat(t.fee_rate_msat_vb)}</span>`],
    ['Total Output', `<span style="color:var(--pkt);font-weight:600">${pkt(t.total_out)}</span>`],
    ['Block',        t.block_height != null
      ? `<a href="${API_BASE}/block/${t.block_height}" style="color:var(--blue)">#${Number(t.block_height).toLocaleString()}</a>`
      : (t.note ? `<span class="normal" style="color:var(--muted);font-size:.82rem">${t.note}</span>` : '—')],
    ['Confirmations', t.confirmations != null ? `<span class="normal">${t.confirmations}</span>` : '—'],
  ].map(([k, v]) => `
    <div class="kv-row">
      <div class="kv-key">${k}</div>
      <div class="kv-val">${v}</div>
    </div>`).join('');

  // ── Inputs table ──────────────────────────────────────────────────────────
  const inputs = t.inputs || [];
  const inputsHtml = inputs.length === 0
    ? '<div style="padding:12px 18px;color:var(--muted);font-size:.85rem">No input data available</div>'
    : inputs.map((inp, i) => {
        if (inp.type === 'coinbase') {
          return `<div class="list-item">
            <div class="item-icon item-icon-block" style="font-size:.75rem">CB</div>
            <div class="item-main">
              <div class="item-primary mono" style="font-size:.82rem">coinbase</div>
              <div class="item-secondary">Block reward</div>
            </div>
          </div>`;
        }
        const prevLink = `<a href="${API_BASE}/rx/${inp.prev_txid}" style="color:var(--blue);font-size:.78rem">${shortHash(inp.prev_txid)}:${inp.prev_vout}</a>`;
        return `<div class="list-item">
          <div class="item-icon" style="background:rgba(100,180,255,.1);color:#64b4ff;border:1px solid rgba(100,180,255,.25);font-size:.75rem;font-weight:700">${i}</div>
          <div class="item-main">
            <div class="item-primary">${addrHtml(inp.address)}</div>
            <div class="item-secondary">← ${prevLink}</div>
          </div>
          <div class="item-right">
            <div class="item-age" style="color:var(--pkt);font-size:.85rem">${inp.value != null ? pkt(inp.value) : '—'}</div>
          </div>
        </div>`;
      }).join('');

  // ── Outputs table ─────────────────────────────────────────────────────────
  const outputs = t.outputs || [];
  const outputsHtml = outputs.length === 0
    ? '<div style="padding:12px 18px;color:var(--muted);font-size:.85rem">No output data available</div>'
    : outputs.map((o) => {
        const spentBadge = o.spent === false
          ? '<span style="font-size:.7rem;color:#4ecdc4;margin-left:6px">UNSPENT</span>' : '';
        return `<div class="list-item">
          <div class="item-icon" style="background:rgba(78,205,196,.1);color:#4ecdc4;border:1px solid rgba(78,205,196,.25);font-size:.75rem;font-weight:700">${o.vout}</div>
          <div class="item-main">
            <div class="item-primary">${addrHtml(o.address)}${spentBadge}</div>
          </div>
          <div class="item-right">
            <div class="item-age" style="color:var(--pkt);font-size:.85rem;font-weight:600">${pkt(o.value)}</div>
          </div>
        </div>`;
      }).join('');

  el.innerHTML = `
    <a class="detail-back" href="${API_BASE}/rx">← All Transactions</a>

    <div class="detail-title">
      💸 Transaction
      <span class="hash" style="font-size:.75rem">${txid}</span>
    </div>

    <div class="panel" style="margin-bottom:20px">${kvRows}</div>

    <div style="display:grid;grid-template-columns:1fr 1fr;gap:16px">
      <div class="panel">
        <div class="panel-head">
          <div class="panel-title">
            <div class="panel-title-icon" style="background:rgba(100,180,255,.1);color:#64b4ff;border:1px solid rgba(100,180,255,.25)">↩</div>
            Inputs <span style="color:var(--muted);font-size:.8rem;margin-left:6px">(${inputs.length})</span>
          </div>
        </div>
        ${inputsHtml}
      </div>
      <div class="panel">
        <div class="panel-head">
          <div class="panel-title">
            <div class="panel-title-icon" style="background:rgba(78,205,196,.1);color:#4ecdc4;border:1px solid rgba(78,205,196,.25)">↪</div>
            Outputs <span style="color:var(--muted);font-size:.8rem;margin-left:6px">(${outputs.length})</span>
          </div>
        </div>
        ${outputsHtml}
      </div>
    </div>`;
}

document.addEventListener('DOMContentLoaded', init);
