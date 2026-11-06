import { useEffect, useState } from "react";

/** Current time, re-rendering every `ms` while `active` (no timer otherwise). */
export function useNow(active: boolean, ms = 100): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    const id = setInterval(() => setNow(Date.now()), ms);
    return () => clearInterval(id);
  }, [active, ms]);
  return now;
}

/** 950 → "1.0s", 12345 → "12.3s", 125000 → "2m 05s". */
export function formatDuration(msIn: number): string {
  const ms = Math.max(0, msIn);
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  const total = Math.round(ms / 1000);
  return `${Math.floor(total / 60)}m ${String(total % 60).padStart(2, "0")}s`;
}
