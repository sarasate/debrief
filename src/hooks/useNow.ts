import { useEffect, useState } from "react";

/** Current time, re-rendered every `ms`. */
export function useNow(ms: number) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), ms);
    return () => clearInterval(id);
  }, [ms]);
  return now;
}

/** "NOW", "42S AGO", "2M AGO", "3H AGO", "4D AGO". */
export function agoLabel(then: number | null, now: number): string {
  if (then == null) return "—";
  const s = Math.max(0, Math.floor((now - then) / 1000));
  if (s < 10) return "NOW";
  if (s < 60) return `${s}S AGO`;
  if (s < 3600) return `${Math.floor(s / 60)}M AGO`;
  if (s < 86400) return `${Math.floor(s / 3600)}H AGO`;
  return `${Math.floor(s / 86400)}D AGO`;
}
