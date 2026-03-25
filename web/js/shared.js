// shared.js — Utilities dùng chung cho tất cả standalone pages
'use strict';

const API_BASE = '/blockchain-rust';

// ── Theme ─────────────────────────────────────────────────────────────────────
function toggleTheme() {
  const html    = document.documentElement;
  const isLight = html.getAttribute('data-theme') === 'light';
  html.setAttribute('data-theme', isLight ? '' : 'light');
  const btn = document.getElementById('themeBtn');
  if (btn) btn.textContent = isLight ? '☀️' : '🌙';
  localStorage.setItem('pkt-theme', isLight ? '' : 'light');
}
(function initTheme() {
  const t   = localStorage.getItem('pkt-theme') || '';
  document.documentElement.setAttribute('data-theme', t);
  const btn = document.getElementById('themeBtn');
  if (btn) btn.textContent = t === 'light' ? '🌙' : '☀️';
})();

// ── Format helpers ─────────────────────────────────────────────────────────────
function shortHash(h) { return h ? h.slice(0,10)+'…'+h.slice(-8) : '—'; }
function shortAddr(a) { return a ? a.slice(0,8)+'…'+a.slice(-6) : '—'; }
function pakletsToPkt(p) { return (p / 1e9).toFixed(4) + ' PKT'; }
function ago(secs) {
  if (secs < 60)   return secs + 's ago';
  if (secs < 3600) return Math.floor(secs/60) + 'm ago';
  return Math.floor(secs/3600) + 'h ago';
}
function fmtHashrate(h) {
  if (h >= 1e15) return (h/1e15).toFixed(2) + ' PH/s';
  if (h >= 1e12) return (h/1e12).toFixed(2) + ' TH/s';
  if (h >= 1e9)  return (h/1e9).toFixed(2)  + ' GH/s';
  if (h >= 1e6)  return (h/1e6).toFixed(2)  + ' MH/s';
  if (h >= 1e3)  return (h/1e3).toFixed(2)  + ' KH/s';
  return h + ' H/s';
}

// ── Fetch ─────────────────────────────────────────────────────────────────────
async function fetchJson(url) {
  try {
    const r = await fetch(url);
    if (!r.ok) return null;
    return await r.json();
  } catch (_) { return null; }
}

// ── Address link ──────────────────────────────────────────────────────────────
function addrLink(addr) {
  if (!addr || addr === '—' || addr === 'coinbase' || addr === 'unknown') return addr || '—';
  const enc = encodeURIComponent(addr);
  return `<a href="${API_BASE}/address/${enc}" style="color:var(--blue)">${addr}</a>`;
}

// ── Shared nav HTML ───────────────────────────────────────────────────────────
function navHTML() {
  return `
<nav>
  <a class="nav-logo" href="${API_BASE}/">
    <div class="nav-logo-icon">P</div>
    <span class="nav-logo-text">PKT<span>Scan</span></span>
  </a>
  <div class="nav-right">
    <a class="nav-link" href="${API_BASE}/block">Blocks</a>
    <a class="nav-link" href="${API_BASE}/rx">Transactions</a>
    <a class="nav-link" href="${API_BASE}/">Stats</a>
    <button class="theme-btn" onclick="toggleTheme()" id="themeBtn">☀️</button>
  </div>
</nav>`;
}

// ── Footer HTML ───────────────────────────────────────────────────────────────
function footerHTML() {
  return `
<footer>
  <div class="footer-inner">
    <div class="footer-logo">
      <span style="background:linear-gradient(135deg,#f7a133,#e07b10);border-radius:6px;width:22px;height:22px;display:flex;align-items:center;justify-content:center;font-size:11px;font-weight:700;color:#000;font-family:'JetBrains Mono',monospace">P</span>
      PKTScan
    </div>
    <span>PKT Blockchain Explorer — built with <a href="https://github.com/TuyenPKT/blockchain-rust">blockchain-rust</a></span>
    <span>© 2026 PKTScan</span>
  </div>
</footer>`;
}
