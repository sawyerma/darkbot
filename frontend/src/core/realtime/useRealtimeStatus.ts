import { useEffect, useState } from "react";
import { realtimeClient } from "./RealtimeClient";
import type { RealtimeStatus } from "./protocol";

export function useRealtimeStatus() {
  const [status, setStatus] = useState<RealtimeStatus>("IDLE");
  useEffect(() => {
    const off = realtimeClient.onStatus(setStatus);
    realtimeClient.connect();
    return off;
  }, []);
  return status;
}
