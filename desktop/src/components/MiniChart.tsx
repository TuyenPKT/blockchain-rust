// MiniChart.tsx — canvas sparkline chart (no deps)
import { useRef, useEffect } from "react";
import { colors } from "../theme";

export interface ChartPoint { x: number; y: number; label?: string; }

interface MiniChartProps {
  points:    ChartPoint[];
  color?:    string;
  height?:   number;
  filled?:   boolean;
  unit?:     string;
}

export function MiniChart({ points, color = colors.accent, height = 80, filled = true }: MiniChartProps) {
  const ref = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = ref.current;
    if (!canvas || points.length < 2) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const W   = canvas.offsetWidth;
    const H   = canvas.offsetHeight;
    canvas.width  = W * dpr;
    canvas.height = H * dpr;
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, W, H);

    const pad  = { top: 8, bottom: 8, left: 4, right: 4 };
    const minY = Math.min(...points.map(p => p.y));
    const maxY = Math.max(...points.map(p => p.y));
    const rangeY = maxY - minY || 1;
    const n = points.length;

    function px(i: number) { return pad.left + (i / (n - 1)) * (W - pad.left - pad.right); }
    function py(v: number) { return H - pad.bottom - ((v - minY) / rangeY) * (H - pad.top - pad.bottom); }

    // Grid lines (3 horizontal)
    ctx.strokeStyle = colors.border;
    ctx.lineWidth = 1;
    for (let i = 0; i <= 2; i++) {
      const y = pad.top + (i / 2) * (H - pad.top - pad.bottom);
      ctx.beginPath();
      ctx.setLineDash([4, 4]);
      ctx.moveTo(pad.left, y);
      ctx.lineTo(W - pad.right, y);
      ctx.stroke();
    }
    ctx.setLineDash([]);

    // Fill area
    if (filled) {
      const grad = ctx.createLinearGradient(0, pad.top, 0, H);
      grad.addColorStop(0, color + "44");
      grad.addColorStop(1, color + "00");
      ctx.beginPath();
      ctx.moveTo(px(0), H - pad.bottom);
      points.forEach((p, i) => ctx.lineTo(px(i), py(p.y)));
      ctx.lineTo(px(n - 1), H - pad.bottom);
      ctx.closePath();
      ctx.fillStyle = grad;
      ctx.fill();
    }

    // Line
    ctx.beginPath();
    ctx.strokeStyle = color;
    ctx.lineWidth   = 2;
    ctx.lineJoin    = "round";
    ctx.lineCap     = "round";
    points.forEach((p, i) => {
      if (i === 0) ctx.moveTo(px(i), py(p.y));
      else ctx.lineTo(px(i), py(p.y));
    });
    ctx.stroke();

    // Last point dot
    const lx = px(n - 1);
    const ly = py(points[n - 1].y);
    ctx.beginPath();
    ctx.arc(lx, ly, 4, 0, Math.PI * 2);
    ctx.fillStyle = color;
    ctx.fill();
    ctx.beginPath();
    ctx.arc(lx, ly, 6, 0, Math.PI * 2);
    ctx.strokeStyle = color + "55";
    ctx.lineWidth = 2;
    ctx.stroke();
  }, [points, color, filled, colors.border]);

  return (
    <canvas
      ref={ref}
      style={{ width: "100%", height, display: "block" }}
    />
  );
}
