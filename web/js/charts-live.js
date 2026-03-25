// charts-live.js — v18.0 PKT Testnet Analytics Charts
// Render Chart.js line/bar charts từ /api/testnet/analytics
// Loaded vào testnet page, gắn vào #pkt-charts-section
'use strict';

(function () {

  const METRICS = [
    { key: 'hashrate',   label: 'Hashrate',   unit: 'H/s',     color: '#f7a133', type: 'line' },
    { key: 'block_time', label: 'Block Time', unit: 'seconds', color: '#4ecdc4', type: 'bar'  },
    { key: 'difficulty', label: 'Difficulty', unit: 'ratio',   color: '#a29bfe', type: 'line' },
  ];

  const WINDOW = 100;
  let charts = {};
  let refreshTimer = null;

  // ── Format helpers ──────────────────────────────────────────────────────────

  function fmtHashrate(h) {
    if (h >= 1e15) return (h / 1e15).toFixed(2) + ' PH/s';
    if (h >= 1e12) return (h / 1e12).toFixed(2) + ' TH/s';
    if (h >= 1e9)  return (h / 1e9).toFixed(2)  + ' GH/s';
    if (h >= 1e6)  return (h / 1e6).toFixed(2)  + ' MH/s';
    if (h >= 1e3)  return (h / 1e3).toFixed(2)  + ' KH/s';
    return h.toFixed(0) + ' H/s';
  }

  function fmtValue(val, unit) {
    if (unit === 'H/s') return fmtHashrate(val);
    if (unit === 'seconds') return val.toFixed(1) + 's';
    if (unit === 'ratio')   return val.toFixed(4);
    return val.toFixed(2);
  }

  function fmtTs(ts) {
    const d = new Date(ts * 1000);
    return d.toISOString().slice(11, 16); // HH:MM
  }

  // ── Fetch one metric ────────────────────────────────────────────────────────

  async function fetchMetric(key) {
    try {
      const r = await fetch(`${API_BASE}/api/testnet/analytics?metric=${key}&window=${WINDOW}`);
      if (!r.ok) return null;
      return await r.json();
    } catch (_) { return null; }
  }

  // ── Render one chart ────────────────────────────────────────────────────────

  function renderChart(canvasId, data, meta) {
    const ctx = document.getElementById(canvasId);
    if (!ctx) return;

    if (charts[canvasId]) {
      charts[canvasId].destroy();
    }

    const labels = data.points.map(p => '#' + p.height);
    const values = data.points.map(p => p.value);

    charts[canvasId] = new Chart(ctx, {
      type: meta.type,
      data: {
        labels,
        datasets: [{
          label: meta.label + ' (' + data.unit + ')',
          data: values,
          borderColor: meta.color,
          backgroundColor: meta.type === 'bar'
            ? meta.color + '88'
            : meta.color + '22',
          borderWidth: meta.type === 'bar' ? 0 : 1.5,
          pointRadius: 0,
          tension: 0.3,
          fill: meta.type === 'line',
        }],
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        animation: { duration: 400 },
        plugins: {
          legend: { display: false },
          tooltip: {
            callbacks: {
              label: ctx => fmtValue(ctx.parsed.y, data.unit),
            },
          },
        },
        scales: {
          x: {
            ticks: {
              maxTicksLimit: 8,
              color: 'var(--muted)',
              font: { size: 10 },
            },
            grid: { color: 'var(--border)' },
          },
          y: {
            ticks: {
              color: 'var(--muted)',
              font: { size: 10 },
              callback: v => fmtValue(v, data.unit),
            },
            grid: { color: 'var(--border)' },
          },
        },
      },
    });

    // Update stat label
    const last = values[values.length - 1];
    const statEl = document.getElementById('pkt-chart-stat-' + meta.key);
    if (statEl && last !== undefined) {
      statEl.textContent = fmtValue(last, data.unit);
    }
  }

  // ── Build HTML section ──────────────────────────────────────────────────────

  function buildSection() {
    const container = document.getElementById('pkt-charts-section');
    if (!container) return;

    container.innerHTML = `
      <div class="panel" style="margin-bottom:24px">
        <div class="panel-head">
          <div class="panel-title">
            <div class="panel-title-icon" style="background:rgba(247,161,51,.15);color:var(--pkt);border:1px solid rgba(247,161,51,.25)">📊</div>
            Network Analytics
          </div>
          <span style="font-size:.78rem;color:var(--muted)">Last ${WINDOW} blocks</span>
        </div>
        <div style="display:grid;grid-template-columns:repeat(3,1fr);gap:12px;padding:0 18px 8px">
          ${METRICS.map(m => `
            <div style="text-align:center;padding:8px 0;border-bottom:1px solid var(--border)">
              <div style="font-size:.72rem;color:var(--muted);text-transform:uppercase;letter-spacing:.07em">${m.label}</div>
              <div id="pkt-chart-stat-${m.key}" style="font-weight:700;font-size:1rem;color:${m.color}">—</div>
            </div>
          `).join('')}
        </div>
        <div style="display:grid;grid-template-columns:1fr 1fr 1fr;gap:16px;padding:16px 18px">
          ${METRICS.map(m => `
            <div>
              <div style="font-size:.75rem;color:var(--muted);margin-bottom:6px;font-weight:600">${m.label}</div>
              <div style="height:120px;position:relative">
                <canvas id="pkt-chart-${m.key}"></canvas>
              </div>
            </div>
          `).join('')}
        </div>
      </div>
    `;
  }

  // ── Load Chart.js từ CDN nếu chưa có ───────────────────────────────────────

  function loadChartJs(cb) {
    if (window.Chart) { cb(); return; }
    const s = document.createElement('script');
    s.src = 'https://cdn.jsdelivr.net/npm/chart.js@4/dist/chart.umd.min.js';
    s.onload = cb;
    s.onerror = () => console.warn('[charts-live] Chart.js CDN failed');
    document.head.appendChild(s);
  }

  // ── Main refresh ────────────────────────────────────────────────────────────

  async function refreshCharts() {
    if (!window.Chart) return;
    for (const meta of METRICS) {
      const data = await fetchMetric(meta.key);
      if (data && data.points && data.points.length > 0) {
        renderChart('pkt-chart-' + meta.key, data, meta);
      }
    }
  }

  // ── Public init ─────────────────────────────────────────────────────────────

  window.pktChartsInit = function () {
    buildSection();
    loadChartJs(async () => {
      await refreshCharts();
      // Auto-refresh mỗi 30s
      if (refreshTimer) clearInterval(refreshTimer);
      refreshTimer = setInterval(refreshCharts, 30_000);
    });
  };

  window.pktChartsStop = function () {
    if (refreshTimer) { clearInterval(refreshTimer); refreshTimer = null; }
  };

})();
