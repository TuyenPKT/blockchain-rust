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
  const t = localStorage.getItem('pkt-theme') || '';
  document.documentElement.setAttribute('data-theme', t);
  // btn not ready yet — set after inject
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

// ── Nav HTML ──────────────────────────────────────────────────────────────────
function _navHTML(activePage) {
  const links = [
    ['block',     'Blocks'],
    ['rx',        'Transactions'],
    ['playground','API'],
    ['webhooks',  'Webhooks'],
    ['dev',       'Developers'],
  ];
  const linkHtml = links.map(([page, label]) => {
    const active = activePage === page ? ' style="color:var(--text)"' : '';
    return `<a class="nav-link"${active} href="${API_BASE}/${page}">${label}</a>`;
  }).join('\n    ');

  return `<nav>
  <a class="nav-logo" href="${API_BASE}/">
    <div class="nav-logo-icon">P</div>
    <span class="nav-logo-text">PKT<span>Scan</span></span>
  </a>
  <div class="nav-right">
    ${linkHtml}
    <button class="theme-btn" onclick="toggleTheme()" aria-label="Toggle theme" id="themeBtn">☀️</button>
  </div>
</nav>`;
}

// ── Footer HTML ───────────────────────────────────────────────────────────────
function _footerHTML() {
  return `<footer>
  <div class="footer-inner">
    <div class="footer-logo">
      <div class="nav-logo-icon" style="width:24px;height:24px;font-size:11px;">P</div>
      PKTScan
    </div>
    <span>PKT Blockchain Explorer — built with <a href="https://github.com/TuyenPKT/blockchain-rust">blockchain-rust</a></span>
    <span>© 2026 PKTScan</span>
  </div>
</footer>`;
}

// ── Auto-inject nav + footer ──────────────────────────────────────────────────
(function injectLayout() {
  // Detect active page from pathname
  const path  = window.location.pathname;
  const parts = path.split('/').filter(Boolean);
  // /blockchain-rust/block/... → parts = ['blockchain-rust','block',...]
  const activePage = parts[1] || '';

  document.addEventListener('DOMContentLoaded', function () {
    // Inject nav if placeholder exists or body has no nav yet
    const navEl = document.getElementById('shared-nav');
    if (navEl) {
      navEl.outerHTML = _navHTML(activePage);
    } else if (!document.querySelector('nav')) {
      document.body.insertAdjacentHTML('afterbegin', _navHTML(activePage));
    }

    // Inject footer if placeholder exists or body has no footer yet
    const footerEl = document.getElementById('shared-footer');
    if (footerEl) {
      footerEl.outerHTML = _footerHTML();
    } else if (!document.querySelector('footer')) {
      document.body.insertAdjacentHTML('beforeend', _footerHTML());
    }

    // Apply theme to button after inject
    const btn = document.getElementById('themeBtn');
    if (btn) btn.textContent = (localStorage.getItem('pkt-theme') === 'light') ? '🌙' : '☀️';
  });
})();
