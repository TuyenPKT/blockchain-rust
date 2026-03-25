// rx-detail.js — /blockchain-rust/rx/:txid
'use strict';

async function init() {
  // Extract txid from URL: /blockchain-rust/rx/<txid>
  const parts = window.location.pathname.split('/');
  const txid  = parts[parts.length - 1];

  if (!txid) {
    document.getElementById('tx-content').innerHTML =
      `<a class="detail-back" href="/blockchain-rust/rx">&#8592; All Transactions</a>
       <div class="panel" style="padding:24px;color:var(--red)">No transaction ID specified.</div>`;
    return;
  }

  document.title = `TX ${txid.slice(0, 12)}… — PKTScan`;

  const data = await fetchJson(`${API_BASE}/api/tx/${txid}`);

  if (!data) {
    document.getElementById('tx-content').innerHTML =
      `<a class="detail-back" href="/blockchain-rust/rx">&#8592; All Transactions</a>
       <div class="panel" style="padding:24px;color:var(--red)">Transaction not found.</div>`;
    return;
  }

  const t          = data.tx ?? data;
  const isCoinbase = t.is_coinbase ?? false;
  const ts         = t.timestamp
    ? new Date(t.timestamp * 1000).toISOString().replace('T', ' ').slice(0, 19) + ' UTC'
    : '—';
  const amount  = ((t.amount ?? t.total_out ?? 0) / 1e9).toFixed(8);
  const fee     = ((t.fee ?? 0) / 1e9).toFixed(8);
  const blockH  = t.block_height ?? t.block_index ?? null;
  const fromVal = isCoinbase ? 'coinbase' : addrLink(t.from ?? '—');
  const toVal   = addrLink(t.to ?? t.outputs?.[0]?.address ?? '—');

  document.getElementById('tx-content').innerHTML = `
    <a class="detail-back" href="/blockchain-rust/rx">&#8592; All Transactions</a>

    <div class="detail-title">
      💸 Transaction
      <span class="hash">${t.tx_id ?? t.txid ?? txid}</span>
    </div>

    <div class="panel">
      <div class="kv-row">
        <div class="kv-key">TxID</div>
        <div class="kv-val">${t.tx_id ?? t.txid ?? txid}</div>
      </div>
      <div class="kv-row">
        <div class="kv-key">Block</div>
        <div class="kv-val">
          ${blockH !== null
            ? `<a href="/blockchain-rust/block/${blockH}" style="color:var(--blue)">#${Number(blockH).toLocaleString()}</a>`
            : '—'}
        </div>
      </div>
      <div class="kv-row">
        <div class="kv-key">Timestamp</div>
        <div class="kv-val normal">${ts}</div>
      </div>
      <div class="kv-row">
        <div class="kv-key">Type</div>
        <div class="kv-val normal">
          <span class="badge ${isCoinbase ? 'badge-coinbase' : 'badge-tx'}">
            ${isCoinbase ? 'coinbase' : 'transfer'}
          </span>
        </div>
      </div>
      <div class="kv-row">
        <div class="kv-key">From</div>
        <div class="kv-val">${fromVal}</div>
      </div>
      <div class="kv-row">
        <div class="kv-key">To</div>
        <div class="kv-val">${toVal}</div>
      </div>
      <div class="kv-row">
        <div class="kv-key">Amount</div>
        <div class="kv-val normal" style="color:var(--pkt)">${amount} PKT</div>
      </div>
      <div class="kv-row" style="border-bottom:none">
        <div class="kv-key">Fee</div>
        <div class="kv-val normal">${fee} PKT</div>
      </div>
    </div>
  `;
}

document.addEventListener('DOMContentLoaded', init);
