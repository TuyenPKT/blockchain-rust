// block-detail.js — /blockchain-rust/block/:height
'use strict';

async function init() {
  // Extract height from URL: /blockchain-rust/block/<height>
  const parts  = window.location.pathname.split('/');
  const height = parts[parts.length - 1];

  if (!height || isNaN(Number(height))) {
    document.getElementById('block-content').innerHTML =
      `<a class="detail-back" href="/blockchain-rust/block">&#8592; All Blocks</a>
       <div class="panel" style="padding:24px;color:var(--red)">Invalid block height.</div>`;
    return;
  }

  document.title = `Block #${height} — PKTScan`;

  const data = await fetchJson(`${API_BASE}/api/block/${height}`);

  if (!data) {
    document.getElementById('block-content').innerHTML =
      `<a class="detail-back" href="/blockchain-rust/block">&#8592; All Blocks</a>
       <div class="panel" style="padding:24px;color:var(--red)">Block #${height} not found.</div>`;
    return;
  }

  const b  = data.block ?? data;
  const h  = b.index ?? b.height ?? height;
  const ts = b.timestamp ? new Date(b.timestamp * 1000).toISOString().replace('T', ' ').slice(0, 19) + ' UTC' : '—';
  const miner = b.miner ?? b.miner_hash ?? '—';
  const prevH = b.index ?? b.height ?? 0;

  document.getElementById('block-content').innerHTML = `
    <a class="detail-back" href="/blockchain-rust/block">&#8592; All Blocks</a>

    <div class="detail-title">
      📦 Block <span style="color:var(--blue)">#${Number(h).toLocaleString()}</span>
    </div>

    <div class="panel" style="margin-bottom:20px">
      <div class="kv-row">
        <div class="kv-key">Block Height</div>
        <div class="kv-val">${Number(h).toLocaleString()}</div>
      </div>
      <div class="kv-row">
        <div class="kv-key">Hash</div>
        <div class="kv-val">${b.hash ?? '—'}</div>
      </div>
      <div class="kv-row">
        <div class="kv-key">Previous Hash</div>
        <div class="kv-val">
          ${b.prev_hash
            ? `<a href="/blockchain-rust/block/${prevH - 1}" style="color:var(--blue)">${b.prev_hash}</a>`
            : '—'}
        </div>
      </div>
      <div class="kv-row">
        <div class="kv-key">Timestamp</div>
        <div class="kv-val normal">${ts}</div>
      </div>
      <div class="kv-row">
        <div class="kv-key">Transactions</div>
        <div class="kv-val normal">${b.tx_count ?? b.transactions?.length ?? 0}</div>
      </div>
      <div class="kv-row">
        <div class="kv-key">Nonce</div>
        <div class="kv-val">${(b.nonce ?? 0).toLocaleString()}</div>
      </div>
      <div class="kv-row">
        <div class="kv-key">Difficulty</div>
        <div class="kv-val normal">${b.difficulty ?? '—'}</div>
      </div>
      <div class="kv-row">
        <div class="kv-key">Block Reward</div>
        <div class="kv-val normal" style="color:var(--pkt)">${pakletsToPkt(b.reward ?? 50e9)}</div>
      </div>
      <div class="kv-row" style="border-bottom:none">
        <div class="kv-key">Miner</div>
        <div class="kv-val">${addrLink(miner)}</div>
      </div>
    </div>

    <div style="display:flex;gap:12px">
      ${Number(h) > 0
        ? `<a href="/blockchain-rust/block/${Number(h)-1}" style="color:var(--blue);font-size:.85rem">&#8592; Block #${Number(h)-1}</a>`
        : ''}
      <a href="/blockchain-rust/block/${Number(h)+1}" style="color:var(--blue);font-size:.85rem;margin-left:auto">Block #${Number(h)+1} &#8594;</a>
    </div>
  `;
}

document.addEventListener('DOMContentLoaded', init);
