// useAnimatedNumber.ts — animate số tăng/giảm mượt
import { useState, useEffect, useRef } from "react";

export function useAnimatedNumber(target: number, durationMs = 600): number {
  const [display, setDisplay] = useState(target);
  const prevRef   = useRef(target);
  const frameRef  = useRef<number>(0);

  useEffect(() => {
    const start = prevRef.current;
    const diff  = target - start;
    if (diff === 0) return;

    const startTime = performance.now();

    function step(now: number) {
      const t = Math.min((now - startTime) / durationMs, 1);
      // ease-out cubic
      const eased = 1 - Math.pow(1 - t, 3);
      setDisplay(Math.round(start + diff * eased));
      if (t < 1) {
        frameRef.current = requestAnimationFrame(step);
      } else {
        prevRef.current = target;
      }
    }

    frameRef.current = requestAnimationFrame(step);
    return () => cancelAnimationFrame(frameRef.current);
  }, [target, durationMs]);

  return display;
}
