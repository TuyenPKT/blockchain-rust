// health.js — v18.8 Node Health Status
'use strict';

const API_BASE = '/blockchain-rust';
const REFRESH_MS = 10_000;

function fmtBytes(b) {
  if (b >= 1e9) return (b / 1e9).toFixed(2) + ' GB';
  if (b >= 1e6) return (b / 1e6).toFixed(1) + ' MB';
  if (b >= 1e3) return (b / 1e3).toFixed(0) + ' KB';
  return b + ' B';
}

function fmtAge(secs) {
  if (secs === 0) return '—';
  if (secs < 60)  return secs + 's';
  if (secs < 3600) return Math.floor(secs / 60) + 'm ' + (secs % 60) + 's';
  return Math.floor(secs / 3600) + 'h ' + Math.floor((secs % 3600) / 60) + 'm';
}

async function fetchHealth() {
  const el = document.getElementById('health-content');
  let h;
  try {
    const r = await fetch(`${API_BASE}/api/health/detailed`);
    h = await r.json();
  } catch (_) {
    el.innerHTML = '<div class="panel" style="padding:24px;color:var(--red)">Unable to reach health endpoint.</div>';
    return;
  }

  const statusColor  = h.ok ? 'var(--green)' : h.alert ? 'var(--red)' : 'var(--yellow)';
  const statusLabel  = h.ok ? '✅ Healthy' : h.alert ? '🚨 Alert' : '⚠️ Degraded';

  const checkedAt = h.checked_at
    ? new Date(h.checked_at * 1000).toISOString().replace('T', ' ').slice(0, 19) + ' UTC'
    : '—';

  const kv = [
    ['Status',         `<span style="color:${statusColor};font-weight:600">${statusLabel}</span>`],
    ['Sync Height',    `<span class="normal">${(h.sync_height ?? 0).toLocaleString()}</span>`],
    ['UTXO Height',    `<span class="normal">${(h.utxo_height ?? 0).toLocaleString()}</span>`],
    ['Sync Lag',       `<span class="normal ${(h.sync_lag ?? 0) > 10 ? 'red' : ''}">${(h.sync_lag ?? 0).toLocaleString()} blocks</span>`],
    ['Last Block Age', `<span class="normal ${(h.last_block_age_secs ?? 0) > 600 ? 'red' : ''}">${fmtAge(h.last_block_age_secs ?? 0)}</span>`],
    ['Mempool',        `<span class="normal">${(h.mempool_count ?? 0).toLocaleString()} pending TXs</span>`],
    ['SyncDB Size',    `<span class="normal">${fmtBytes(h.syncdb_size_bytes ?? 0)}</span>`],
    ['UtxoDB Size',    `<span class="normal">${fmtBytes(h.utxodb_size_bytes ?? 0)}</span>`],
    ['AddrDB Size',    `<span class="normal">${fmtBytes(h.addrdb_size_bytes ?? 0)}</span>`],
    ['MempoolDB Size', `<span class="normal">${fmtBytes(h.mempooldb_size_bytes ?? 0)}</span>`],
    ['Total DB Size',  `<span class="normal">${fmtBytes(h.total_db_size_bytes ?? 0)}</span>`],
    ['Checked At',     `<span class="normal">${checkedAt}</span>`],
  ].map(([k, v]) => `
    <div class="kv-row">
      <div class="kv-key">${k}</div>
      <div class="kv-val">${v}</div>
    </div>`).join('');

  const alertBanner = h.alert_message
    ? `<div style="background:${h.alert ? 'var(--red)' : 'var(--yellow)'};color:#fff;padding:12px 18px;border-radius:8px;margin-bottom:16px;font-size:.88rem;font-weight:600">
        ${h.alert_message}
       </div>`
    : '';

  el.innerHTML = `
    ${alertBanner}
    <div class="panel" style="margin-bottom:16px">${kv}</div>
    <div style="text-align:right;color:var(--muted);font-size:.78rem">
      Auto-refresh every 10s &nbsp;·&nbsp; <a href="${API_BASE}/api/health/detailed" style="color:var(--blue)">raw JSON</a>
    </div>`;
}

document.addEventListener('DOMContentLoaded', () => {
  fetchHealth();
  setInterval(fetchHealth, REFRESH_MS);
});
