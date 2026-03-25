// block-list.js — /blockchain-rust/block  (All Blocks page)
'use strict';

let offset = 0;
const LIMIT = 50;

async function loadBlocks(reset) {
  if (reset) { offset = 0; }
  const data = await fetchJson(`${API_BASE}/api/blocks?limit=${LIMIT}&offset=${offset}`);
  const list = document.getElementById('blocks-list');

  if (!data) {
    list.innerHTML = '<div style="padding:24px;color:var(--red)">Failed to load blocks.</div>';
    return;
  }

  const blocks = (data.blocks ?? data ?? []).map(b => ({
    height:    b.index ?? b.height ?? 0,
    hash:      b.hash ?? '',
    timestamp: (b.timestamp ?? 0) * 1000,
    txCount:   b.tx_count ?? b.transactions?.length ?? 0,
    miner:     b.miner ?? b.miner_hash ?? '',
    reward:    b.reward ?? 50e9,
    difficulty: b.difficulty ?? 0,
  }));

  if (reset) list.innerHTML = '';

  if (blocks.length === 0 && reset) {
    list.innerHTML = '<div style="padding:24px;color:var(--muted)">No blocks found.</div>';
    document.getElementById('load-more').style.display = 'none';
    return;
  }

  blocks.forEach(b => {
    const secsAgo = Math.floor((Date.now() - b.timestamp) / 1000);
    const row = document.createElement('div');
    row.className = 'list-item block-item';
    row.style.cursor = 'pointer';
    row.innerHTML = `
      <div class="item-icon item-icon-block">#${b.height % 1000}</div>
      <div class="item-main">
        <div class="item-primary">#${b.height.toLocaleString()}</div>
        <div class="item-secondary">
          ${b.txCount} txns &nbsp;·&nbsp;
          Miner: <span class="addr-short">${shortAddr(b.miner)}</span>
        </div>
      </div>
      <div class="item-right">
        <div class="item-amount">${pakletsToPkt(b.reward)}</div>
        <div class="item-age">${ago(secsAgo)}</div>
      </div>
    `;
    row.onclick = () => {
      window.location.href = `${API_BASE}/block/${b.height}`;
    };
    list.appendChild(row);
  });

  offset += blocks.length;
  document.getElementById('load-more').style.display =
    blocks.length === LIMIT ? 'block' : 'none';
}

function loadMore() { loadBlocks(false); }

document.addEventListener('DOMContentLoaded', () => loadBlocks(true));
