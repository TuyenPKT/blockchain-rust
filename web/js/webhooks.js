// webhooks.js — v19.8 Webhook Manager
'use strict';

const BASE = '/blockchain-rust';

// ── State ──────────────────────────────────────────────────────────────────────
let apiKey = '';
let lastSecret = null;   // hiển thị 1 lần sau khi register

// ── API key ────────────────────────────────────────────────────────────────────
function onKeyChange() {
  apiKey = document.getElementById('api-key').value.trim();
  const status = document.getElementById('key-status');
  if (apiKey.length > 8) {
    status.textContent = '✓ Sẵn sàng — nhấn Refresh để tải danh sách';
    status.style.color = 'var(--green)';
    loadWebhooks();
  } else {
    status.textContent = '';
  }
}

function toggleKeyVisibility() {
  const el = document.getElementById('api-key');
  el.type = el.type === 'password' ? 'text' : 'password';
}

// ── Helpers ────────────────────────────────────────────────────────────────────
function authHeaders() {
  return { 'Content-Type': 'application/json', 'X-API-Key': apiKey };
}

function showToast(msg, type = 'ok') {
  const el = document.getElementById('toast');
  el.textContent = msg;
  el.className = `toast show ${type}`;
  clearTimeout(el._t);
  el._t = setTimeout(() => { el.className = 'toast'; }, 3500);
}

function fmtDate(ts) {
  return new Date(ts * 1000).toLocaleString('vi-VN', {
    day: '2-digit', month: '2-digit', year: 'numeric',
    hour: '2-digit', minute: '2-digit',
  });
}

// ── Event checkboxes ───────────────────────────────────────────────────────────
function toggleEvent(item) {
  const cb = item.querySelector('input[type=checkbox]');
  // Nếu click vào chính checkbox thì đừng toggle lại (browser đã handle)
  if (event && event.target === cb) return;
  cb.checked = !cb.checked;
}

function selectedEvents() {
  return ['new_block', 'new_tx', 'address_activity']
    .filter(id => document.getElementById(`ev-${id}`).checked);
}

// ── Register ───────────────────────────────────────────────────────────────────
async function registerWebhook() {
  if (!apiKey) { showToast('Nhập API key trước', 'err'); return; }

  const url    = document.getElementById('wh-url').value.trim();
  const events = selectedEvents();
  const addr   = document.getElementById('wh-addr').value.trim();

  if (!url)          { showToast('URL không được trống', 'err'); return; }
  if (!events.length){ showToast('Chọn ít nhất 1 event', 'err'); return; }

  const body = { url, events };
  if (addr) body.address_filter = addr;

  const btn = document.getElementById('register-btn');
  btn.disabled = true;
  btn.textContent = 'Đang đăng ký…';

  try {
    const res = await fetch(`${BASE}/api/webhooks`, {
      method: 'POST',
      headers: authHeaders(),
      body: JSON.stringify(body),
    });

    const data = await res.json();

    if (!res.ok) {
      showToast(`Lỗi: ${data.error || res.status}`, 'err');
      return;
    }

    // Hiển thị secret 1 lần
    lastSecret = data.secret;
    const secretBox = document.getElementById('secret-box');
    document.getElementById('secret-value').textContent = data.secret;
    secretBox.style.display = 'block';

    // Reset form
    document.getElementById('wh-url').value = '';
    document.getElementById('wh-addr').value = '';
    ['new_block', 'new_tx', 'address_activity'].forEach(id => {
      document.getElementById(`ev-${id}`).checked = false;
    });

    showToast(`Đã đăng ký webhook ${data.id}`, 'ok');
    loadWebhooks();
  } catch (e) {
    showToast(`Network error: ${e.message}`, 'err');
  } finally {
    btn.disabled = false;
    btn.textContent = '+ Đăng ký Webhook';
  }
}

// ── Copy / dismiss secret ──────────────────────────────────────────────────────
function copySecret() {
  if (!lastSecret) return;
  navigator.clipboard.writeText(lastSecret)
    .then(() => showToast('Đã copy secret', 'ok'));
}

function dismissSecret() {
  document.getElementById('secret-box').style.display = 'none';
  lastSecret = null;
}

// ── Load list ──────────────────────────────────────────────────────────────────
async function loadWebhooks() {
  if (!apiKey) return;

  const container = document.getElementById('wh-list-container');
  container.innerHTML = '<div class="loading">Đang tải…</div>';

  try {
    const res = await fetch(`${BASE}/api/webhooks`, { headers: authHeaders() });

    if (res.status === 403) {
      container.innerHTML = `
        <div class="empty-state">
          <div class="empty-icon">🔒</div>
          <div>API key không có quyền <strong>write</strong></div>
        </div>`;
      return;
    }

    const data = await res.json();
    renderList(data.webhooks || []);
  } catch (e) {
    container.innerHTML = `
      <div class="empty-state">
        <div class="empty-icon">⚠️</div>
        <div>Network error: ${e.message}</div>
      </div>`;
  }
}

// ── Render ─────────────────────────────────────────────────────────────────────
function renderList(webhooks) {
  const container = document.getElementById('wh-list-container');

  if (!webhooks.length) {
    container.innerHTML = `
      <div class="empty-state">
        <div class="empty-icon">🔕</div>
        <div>Chưa có webhook nào. Đăng ký webhook đầu tiên bên trái.</div>
      </div>`;
    return;
  }

  container.innerHTML = `<div class="wh-list">${webhooks.map(renderCard).join('')}</div>`;
}

function renderCard(wh) {
  const events = wh.events.map(e => `<span class="tag">${escHtml(e)}</span>`).join('');
  const addrTag = wh.address_filter
    ? `<span class="tag tag-addr">@${escHtml(wh.address_filter)}</span>` : '';

  return `
    <div class="wh-card" id="card-${wh.id}">
      <div class="wh-card-header">
        <div style="flex:1">
          <div class="wh-card-id">
            <span class="status-dot"></span>ID: ${escHtml(wh.id)}
          </div>
        </div>
        <button class="btn btn-danger" onclick="deleteWebhook('${escHtml(wh.id)}')">Xoá</button>
      </div>
      <div class="wh-card-url">${escHtml(wh.url)}</div>
      <div class="wh-card-meta">
        ${events}
        ${addrTag}
        <span class="ts">${fmtDate(wh.created_at)}</span>
      </div>
    </div>`;
}

function escHtml(s) {
  return String(s)
    .replace(/&/g, '&amp;').replace(/</g, '&lt;')
    .replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

// ── Delete ─────────────────────────────────────────────────────────────────────
async function deleteWebhook(id) {
  if (!apiKey) { showToast('Nhập API key trước', 'err'); return; }

  // Tắt nút xoá ngay
  const card = document.getElementById(`card-${id}`);
  const btn = card && card.querySelector('.btn-danger');
  if (btn) { btn.disabled = true; btn.textContent = '…'; }

  try {
    const res = await fetch(`${BASE}/api/webhooks/${encodeURIComponent(id)}`, {
      method: 'DELETE',
      headers: authHeaders(),
    });

    if (res.ok) {
      showToast(`Đã xoá webhook ${id}`, 'ok');
      loadWebhooks();
    } else {
      const data = await res.json().catch(() => ({}));
      showToast(`Lỗi: ${data.error || res.status}`, 'err');
      if (btn) { btn.disabled = false; btn.textContent = 'Xoá'; }
    }
  } catch (e) {
    showToast(`Network error: ${e.message}`, 'err');
    if (btn) { btn.disabled = false; btn.textContent = 'Xoá'; }
  }
}
