// rx-list.js — /blockchain-rust/rx  (All Transactions page)
'use strict';

let offset = 0;
const LIMIT = 50;

async function loadTxs(reset) {
  if (reset) { offset = 0; }
  const data = await fetchJson(`${API_BASE}/api/txs?limit=${LIMIT}&offset=${offset}`);
  const list = document.getElementById('tx-list');

  if (!data) {
    list.innerHTML = '<div style="padding:24px;color:var(--red)">Failed to load transactions.</div>';
    return;
  }

  const txs = (data.txs ?? data ?? []).map(t => ({
    txid:       t.tx_id ?? t.txid ?? '',
    blockHeight: t.block_height ?? t.block_index ?? 0,
    timestamp:  (t.block_timestamp ?? t.timestamp ?? 0) * 1000,
    from:       t.from ?? (t.is_coinbase ? 'coinbase' : ''),
    to:         t.to ?? t.outputs?.[0]?.address ?? '',
    amount:     (t.output_total ?? t.amount ?? t.total_out ?? 0) / 1e9,
    isCoinbase: t.is_coinbase ?? false,
  }));

  if (reset) list.innerHTML = '';

  if (txs.length === 0 && reset) {
    list.innerHTML = '<div style="padding:24px;color:var(--muted)">No transactions found.</div>';
    document.getElementById('load-more').style.display = 'none';
    return;
  }

  txs.forEach(tx => {
    const secsAgo = Math.floor((Date.now() - tx.timestamp) / 1000);
    const row = document.createElement('div');
    row.className = 'list-item tx-item';
    row.style.cursor = 'pointer';
    row.innerHTML = `
      <div>
        <div style="display:flex;align-items:center;gap:8px;margin-bottom:3px">
          <span class="item-primary">${shortHash(tx.txid)}</span>
          <span class="badge ${tx.isCoinbase ? 'badge-coinbase' : 'badge-tx'}">
            ${tx.isCoinbase ? 'coinbase' : 'transfer'}
          </span>
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
    row.onclick = () => {
      window.location.href = `${API_BASE}/rx/${tx.txid}`;
    };
    list.appendChild(row);
  });

  offset += txs.length;
  document.getElementById('load-more').style.display =
    txs.length === LIMIT ? 'block' : 'none';
}

function loadMore() { loadTxs(false); }

document.addEventListener('DOMContentLoaded', () => loadTxs(true));
