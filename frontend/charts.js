// v14.5 — PKT Web Charts
// Fetch /api/analytics/:metric → render sparkline (ASCII) + Chart.js line charts
// Tự inject vào trang, không cần build step. Chart.js load từ CDN nếu có.

(function () {
  'use strict';

  const METRICS = [
    { id: 'hashrate',      label: 'Hashrate',   unit: 'kH/s',    color: '#4ade80' },
    { id: 'block_time',    label: 'Block Time', unit: 's',       color: '#60a5fa' },
    { id: 'tx_throughput', label: 'TX Volume',  unit: 'tx/blk',  color: '#f59e0b' },
    { id: 'fee_market',    label: 'Fee Market', unit: 'sat/vB',  color: '#a78bfa' },
  ];

  const WINDOW   = 50;
  const INTERVAL = 30_000; // ms
  const SPARKS   = '▁▂▃▄▅▆▇█';

  let chartInstances = {};

  // ── Data fetching ──────────────────────────────────────────────────────────

  async function fetchMetric(metricId) {
    try {
      const res = await fetch(`/api/analytics/${metricId}?window=${WINDOW}`);
      if (!res.ok) return [];
      const json = await res.json();
      // AnalyticsSeries { metric, window, points: [{height, timestamp, value}] }
      if (Array.isArray(json))         return json;
      if (Array.isArray(json.points))  return json.points;
      if (Array.isArray(json.data))    return json.data;
      return [];
    } catch (_) {
      return [];
    }
  }

  // ── Sparkline (ASCII) ──────────────────────────────────────────────────────

  function toSparkline(points) {
    if (!points || points.length === 0) return '(no data)';
    const values = points.map(p => Number(p.value) || Number(p.value2) || 0);
    const min    = Math.min(...values);
    const max    = Math.max(...values);
    const range  = max - min;
    return values.map(v => {
      if (range < 1e-10) return '▄';
      const idx = Math.round((v - min) / range * 7);
      return SPARKS[Math.min(7, Math.max(0, idx))];
    }).join('');
  }

  function stats(points) {
    const values = points.map(p => Number(p.value) || 0);
    if (values.length === 0) return { min: 0, max: 0, avg: 0, latest: 0 };
    const min    = Math.min(...values);
    const max    = Math.max(...values);
    const avg    = values.reduce((a, b) => a + b, 0) / values.length;
    const latest = values[values.length - 1];
    return { min, max, avg, latest };
  }

  // ── Sparkline card (HTML) ──────────────────────────────────────────────────

  function renderSparkCard(metric, points) {
    const id = 'pkt-spark-' + metric.id;
    let el = document.getElementById(id);
    if (!el) {
      el = document.createElement('div');
      el.id = id;
      el.className = 'pkt-spark-card';
      document.getElementById('pkt-sparklines').appendChild(el);
    }

    const spark = toSparkline(points);
    const s     = stats(points);
    el.innerHTML =
      `<div class="pkt-spark-label">${metric.label}</div>` +
      `<div class="pkt-spark-line">${spark}</div>` +
      `<div class="pkt-spark-stats">` +
        `latest <b>${s.latest.toFixed(1)}</b>${metric.unit} · ` +
        `avg <b>${s.avg.toFixed(1)}</b> · ` +
        `min <b>${s.min.toFixed(1)}</b> · ` +
        `max <b>${s.max.toFixed(1)}</b>` +
      `</div>`;
  }

  // ── Chart.js line chart ────────────────────────────────────────────────────

  function getOrCreateCanvas(metric) {
    const wrapperId = 'pkt-chart-wrap-' + metric.id;
    let wrapper = document.getElementById(wrapperId);
    if (!wrapper) {
      wrapper = document.createElement('div');
      wrapper.id = wrapperId;
      wrapper.className = 'pkt-chart-wrap';
      wrapper.innerHTML = `<div class="pkt-chart-title">${metric.label} (${metric.unit})</div>`;
      const canvas = document.createElement('canvas');
      canvas.id = 'pkt-canvas-' + metric.id;
      wrapper.appendChild(canvas);
      document.getElementById('pkt-charts').appendChild(wrapper);
    }
    return document.getElementById('pkt-canvas-' + metric.id);
  }

  function renderLineChart(metric, points) {
    if (typeof Chart === 'undefined') return; // CDN chưa load

    const labels = points.map(p => `#${p.height}`);
    const values = points.map(p => Number(p.value) || 0);
    const canvas = getOrCreateCanvas(metric);

    if (chartInstances[metric.id]) {
      const c = chartInstances[metric.id];
      c.data.labels             = labels;
      c.data.datasets[0].data  = values;
      c.update('none');
      return;
    }

    chartInstances[metric.id] = new Chart(canvas, {
      type: 'line',
      data: {
        labels,
        datasets: [{
          label: metric.label,
          data: values,
          borderColor: metric.color,
          backgroundColor: metric.color + '22',
          borderWidth: 1.5,
          pointRadius: 0,
          fill: true,
          tension: 0.3,
        }],
      },
      options: {
        responsive: true,
        animation: false,
        plugins: { legend: { display: false } },
        scales: {
          x: { display: false },
          y: {
            ticks: { color: '#94a3b8', font: { size: 11 } },
            grid:  { color: '#1e293b' },
          },
        },
      },
    });
  }

  // ── Main refresh loop ──────────────────────────────────────────────────────

  async function refresh() {
    for (const metric of METRICS) {
      const points = await fetchMetric(metric.id);
      renderSparkCard(metric, points);
      renderLineChart(metric, points);
    }
  }

  // ── Styles ─────────────────────────────────────────────────────────────────

  function injectStyles() {
    if (document.getElementById('pkt-chart-css')) return;
    const s = document.createElement('style');
    s.id = 'pkt-chart-css';
    s.textContent = `
      #pkt-sparklines {
        display: flex; flex-wrap: wrap; gap: .5rem; margin: 1rem 0;
      }
      .pkt-spark-card {
        background: #1e293b; border-radius: 6px;
        padding: .65rem 1rem; min-width: 200px;
      }
      .pkt-spark-label {
        font-size: .7rem; color: #94a3b8; margin-bottom: .2rem;
        text-transform: uppercase; letter-spacing: .05em;
      }
      .pkt-spark-line {
        font-family: monospace; font-size: 1rem;
        color: #4ade80; letter-spacing: 2px;
      }
      .pkt-spark-stats {
        font-size: .68rem; color: #64748b; margin-top: .25rem;
      }
      #pkt-charts {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
        gap: 1rem; margin-top: 1rem;
      }
      .pkt-chart-wrap {
        background: #1e293b; border-radius: 8px; padding: 1rem;
      }
      .pkt-chart-title {
        font-size: .75rem; color: #94a3b8; margin-bottom: .5rem;
        text-transform: uppercase; letter-spacing: .05em;
      }
    `;
    document.head.appendChild(s);
  }

  // ── Chart.js CDN loader ────────────────────────────────────────────────────

  function loadChartJs(cb) {
    if (typeof Chart !== 'undefined') { cb(); return; }
    const script = document.createElement('script');
    script.src = 'https://cdn.jsdelivr.net/npm/chart.js@4.4.0/dist/chart.umd.min.js';
    script.onload  = cb;
    script.onerror = cb; // graceful — sparklines vẫn hoạt động dù CDN fail
    document.head.appendChild(script);
  }

  // ── Bootstrap ──────────────────────────────────────────────────────────────

  function init() {
    injectStyles();

    // Tạo containers nếu chưa có
    const root = document.querySelector('main, .main-content, body');
    if (!document.getElementById('pkt-sparklines')) {
      const section = document.createElement('section');
      section.innerHTML =
        '<h3 style="color:#94a3b8;font-size:.85rem;margin:1rem 0 .25rem;text-transform:uppercase;letter-spacing:.08em">' +
        'Chain Charts</h3>';
      ['pkt-sparklines', 'pkt-charts'].forEach(id => {
        const div = document.createElement('div');
        div.id = id;
        section.appendChild(div);
      });
      root.appendChild(section);
    }

    loadChartJs(() => {
      refresh();
      setInterval(refresh, INTERVAL);
    });
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }
})();
