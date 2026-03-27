// block-list.js — v18.5 Cursor-based pagination
'use strict';

let cursor = null;   // null = first page; number = next_cursor from last response
const LIMIT = 50;

async function loadBlocks(reset) {
  if (reset) { cursor = null; }
  let url = `${API_BASE}/api/testnet/headers?limit=${LIMIT}`;
  if (cursor !== null) url += `&cursor=${cursor}`;
  const data = await fetchJson(url);
  const list = document.getElementById('blocks-list');

  if (!data) {
    list.innerHTML = '<div style="padding:24px;color:var(--red)">Failed to load blocks.</div>';
    return;
  }

  const headers = data.headers ?? [];

  if (reset) list.innerHTML = '';

  if (headers.length === 0 && reset) {
    list.innerHTML = '<div style="padding:24px;color:var(--muted)">No blocks found.</div>';
    document.getElementById('load-more').style.display = 'none';
    return;
  }

  headers.forEach(b => {
    const secsAgo = Math.floor((Date.now() - (b.timestamp ?? 0) * 1000) / 1000);
    const row = document.createElement('div');
    row.className = 'list-item block-item';
    row.style.cursor = 'pointer';
    row.innerHTML = `
      <div class="item-icon item-icon-block">#${(b.height ?? 0) % 1000}</div>
      <div class="item-main">
        <div class="item-primary">#${(b.height ?? 0).toLocaleString()}</div>
        <div class="item-secondary mono" style="font-size:.78rem">${shortHash(b.hash ?? '')}</div>
      </div>
      <div class="item-right">
        <div class="item-age">${ago(secsAgo)}</div>
      </div>
    `;
    row.onclick = () => {
      window.location.href = `${API_BASE}/block/${b.height}`;
    };
    list.appendChild(row);
  });

  if (data.next_cursor != null) cursor = data.next_cursor;
  document.getElementById('load-more').style.display =
    headers.length === LIMIT ? 'block' : 'none';
}

function loadMore() { loadBlocks(false); }

document.addEventListener('DOMContentLoaded', () => loadBlocks(true));
