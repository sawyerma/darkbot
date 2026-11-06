import { useEffect, useState } from "react";
import { bscQuery, type BscQuery, type BscQueryOutcome } from "@/core/realtime/bscQuery";

export type QueryState = { kind: "IDLE" } | { kind: "LOADING" } | BscQueryOutcome;

/** One bounded query per view and key change (BSC-V8-10): no polling, no retry, no cache. A
 *  result arriving after the key changed is dropped; a `null` query is not sent (IDLE). */
export function useBscQuery(key: string, query: BscQuery | null): QueryState {
  const [state, setState] = useState<{ key: string; value: QueryState }>({ key, value: { kind: query ? "LOADING" : "IDLE" } });
  useEffect(() => {
    let current = true;
    if (!query) { setState({ key, value: { kind: "IDLE" } }); return () => { current = false; }; }
    setState({ key, value: { kind: "LOADING" } });
    void bscQuery(query).then(value => { if (current) setState({ key, value }); });
    return () => { current = false; };
    // The key names the query; the object itself is rebuilt on every render.
  }, [key]);
  return state.key === key ? state.value : { kind: query ? "LOADING" : "IDLE" };
}

/** The empty-state text of a query without its native owner. */
export function queryReason(q: QueryState, owner: string): string {
  if (q.kind === "IDLE") return "not loaded";
  if (q.kind === "LOADING") return "Loading…";
  if (q.kind === "NOT_IMPLEMENTED") return `Not implemented yet: ${owner} (${q.reason}).`;
  if (q.kind === "UNAVAILABLE") return `Unavailable: ${q.reason}.`;
  return "";
}
