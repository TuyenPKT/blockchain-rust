// rx-list.js — v18.5 Cursor-based pagination
'use strict';

let cursor = null;   // null = first page; number = next_cursor from last response
const LIMIT = 50;

async function loadTxs(reset) {
  if (reset) { cursor = null; }
  let url = `${API_BASE}/api/testnet/txs?limit=${LIMIT}`;
  if (cursor !== null) url += `&cursor=${cursor}`;
  const data = await fetchJson(url);
  const list = document.getElementById('tx-list');

  if (!data) {
    list.innerHTML = `<div style="padding:24px;color:var(--red)">
      ⚠ Không thể tải dữ liệu transactions — server chưa chạy hoặc không phản hồi.
      <br><button onclick="loadTxs(true)" style="margin-top:10px;background:var(--surface2);border:1px solid var(--border);color:var(--text);padding:6px 16px;border-radius:6px;cursor:pointer;font-size:.82rem">↻ Thử lại</button>
    </div>`;
    return;
  }

  const txs = data.txs ?? [];

  if (reset) list.innerHTML = '';

  if (txs.length === 0 && reset) {
    list.innerHTML = '<div style="padding:24px;color:var(--muted)">No transactions found.</div>';
    document.getElementById('load-more').style.display = 'none';
    return;
  }

  txs.forEach(tx => {
    const secsAgo = Math.floor((Date.now() - (tx.timestamp ?? 0) * 1000) / 1000);
    const row = document.createElement('div');
    row.className = 'list-item tx-item';
    row.style.cursor = 'pointer';
    row.innerHTML = `
      <div class="item-main">
        <div class="item-primary mono" style="font-size:.85rem">${shortHash(tx.txid ?? '')}</div>
        <div class="item-secondary">Block #${(tx.height ?? 0).toLocaleString("en-US")}</div>
      </div>
      <div class="item-right">
        <div class="item-age">${ago(secsAgo)}</div>
      </div>
    `;
    row.onclick = () => {
      window.location.href = `${API_BASE}/rx/${tx.txid}`;
    };
    list.appendChild(row);
  });

  if (data.next_cursor != null) cursor = data.next_cursor;
  document.getElementById('load-more').style.display =
    txs.length === LIMIT ? 'block' : 'none';
}

function loadMore() { loadTxs(false); }

loadTxs(true);
