import type { TopicStatus } from "@/core/realtime";
import type { CaptureView, CounterName } from "./status";
import { streamFreshness, type StreamClock } from "./streamFreshness";

export function counterOf(c: CaptureView | null, name: CounterName): bigint | null {
  return c?.market?.counters.get(name) ?? null;
}

const PHASE_LABEL = {
  WAITING_FOR_STREAM: "Waiting for stream",
  CAPTURE: "Capturing",
  DRAIN: "Draining",
  FINALIZE: "Finalizing",
  ENDED: "Ended",
  UNKNOWN: "Unknown",
} as const;

export function phaseLabel(c: CaptureView | null): string {
  return c?.phase ? PHASE_LABEL[c.phase.phase] : "Unknown";
}

// Ingress transport states (ws_rpc.rs) in which no subscription stream is delivering.
const RECONNECTING = new Set(["STARTING", "RECOVERING", "DEGRADED"]);

function reconnectLabel(c: CaptureView): string | null {
  const t = c.transport;
  if (!t || !RECONNECTING.has(t.lastState)) return null;
  if (t.lastState === "DEGRADED") return `Reconnecting · ${t.lastCode}`;
  return (t.lastAttempt ?? 1) > 1 ? `Reconnecting · attempt ${t.lastAttempt}` : null;
}

function waitingLabel(c: CaptureView): string {
  // Cumulative engine counter: stays visible between attempts instead of flickering with them.
  const rateLimited = counterOf(c, "transport_http_429") ?? 0n;
  if (rateLimited > 0n) return `Provider rate limit · HTTP 429 ×${rateLimited}`;
  return reconnectLabel(c) ?? "Waiting for stream";
}

function endedLabel(c: CaptureView): string {
  const r = c.phase?.endResult ?? "";
  if (c.phase?.stopReason === "OPERATOR_STOP") return "Stopped · unverified";
  if (r === "CAPTURE_COMPLETE") return "Completed";
  if (r.startsWith("MARKET_OBSERVER_STREAM_NOT_READY")) {
    return (counterOf(c, "transport_http_429") ?? 0n) > 0n ? "No market stream · HTTP 429" : "No market stream";
  }
  return "Incomplete";
}

/** Text of the page's health badge. Gateway/topic liveness, capture phase, Solana transport, stream
 *  freshness (UI clock) and data qualification are judged separately; nothing here claims valid data,
 *  and an old snapshot is always named as such. */
export function healthLabel(topicStatus: TopicStatus, c: CaptureView | null, clock?: StreamClock): string {
  if (topicStatus === "UNAVAILABLE") return "Unavailable";
  if (topicStatus === "ERROR") return "Error";
  if (!c) return "No capture data";
  // A finished run stays readable after its reporter exits by design: that is not an error.
  if (c.phase?.phase === "ENDED") {
    const suffix = topicStatus === "DETACHED" ? " · no capture running" : topicStatus === "STALE" ? " · stale" : "";
    return `Last run · ${endedLabel(c)}${suffix}`;
  }
  if (topicStatus === "DETACHED") return "Capture process gone · last known";
  if (topicStatus === "STALE") return "Stale · last known";
  if (topicStatus !== "LIVE") return "Not live · last known";

  const phase = c.phase?.phase;
  if (phase === "WAITING_FOR_STREAM") return waitingLabel(c);
  if (phase === "DRAIN" || phase === "FINALIZE") return "Finishing";
  if (phase !== "CAPTURE") return "Unknown";
  const t = c.transport;
  if (t?.lastState === "INCOMPLETE_ABORT") return `Stream aborted · ${t.lastCode}`;
  const reconnecting = reconnectLabel(c);
  if (reconnecting) return reconnecting;
  const fresh = clock ? streamFreshness(clock) : "CHECKING";
  if (fresh === "STALE") return "Solana stream stale";
  if (fresh === "CHECKING") return "Checking stream";
  const a = c.assessment;
  if (a && !(a.coverageComplete && a.decodeComplete)) return "Degraded";
  return "Live · unverified";
}
