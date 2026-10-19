import { useEffect, useState } from "react";

/** Presentation-only clock. Refreshes while visible and catches up when foregrounded. */
export function useNowMs(): number {
  const [nowMs, setNowMs] = useState(() => Date.now());
  useEffect(() => {
    let timer: number | undefined;
    const onVisibilityChange = () => {
      window.clearInterval(timer);
      if (document.visibilityState === "visible") {
        setNowMs(Date.now());
        timer = window.setInterval(() => setNowMs(Date.now()), 1_000);
      }
    };
    onVisibilityChange();
    document.addEventListener("visibilitychange", onVisibilityChange);
    return () => {
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", onVisibilityChange);
    };
  }, []);
  return nowMs;
}
