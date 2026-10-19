import type { ReactNode } from "react";

// Health = READY ist keine Execution-/LIVE-Freigabe.
export type NoticeState =
  | "UNKNOWN" | "NOT_CONNECTED" | "STALE" | "DETACHED" | "INCOMPLETE"
  | "FAILED" | "BLOCKED" | "READY" | "LIVE" | "OFFLINE_REPLAY";

const LABEL: Record<NoticeState, string> = {
  UNKNOWN: "Unknown",
  NOT_CONNECTED: "Not connected",
  STALE: "Stale",
  DETACHED: "Source detached",
  INCOMPLETE: "Incomplete",
  FAILED: "Failed",
  BLOCKED: "Blocked",
  READY: "Ready",
  LIVE: "Live",
  OFFLINE_REPLAY: "Offline replay",
};
export function StatusBadge({ state, detail }: { state: NoticeState; detail?: string }) {
  return <span className="g-badge" data-state={state} title={detail}>{LABEL[state]}</span>;
}
export function Notice({ children }: { children: ReactNode }) {
  return <p className="g-note" role="status">{children}</p>;
}
