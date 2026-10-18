import type { TopicStatus } from "@/core/realtime";
import type { CaptureView, CounterName } from "./status";

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

/** Text of the page's health badge, derived only from reported values. */
export function healthLabel(topicStatus: TopicStatus, c: CaptureView | null): string {
  if (topicStatus === "UNAVAILABLE") return "Unavailable";
  if (topicStatus === "ERROR") return "Error";
  // Completion is durable historical information, even after the reporter exits.
  // Keep connection quality visible alongside it.
  if (c?.phase?.phase === "ENDED") {
    const result = c.phase.endResult === "CAPTURE_COMPLETE" ? "Completed" : "Incomplete";
    return topicStatus === "DETACHED" ? `${result} · Source detached`
      : topicStatus === "STALE" ? `${result} · Stale` : result;
  }
  if (topicStatus === "DETACHED") return "Source detached";
  if (topicStatus === "STALE") return "Stale";
  if (topicStatus !== "LIVE" || !c) return "Unknown";
  const phase = c.phase?.phase;
  if (phase === "WAITING_FOR_STREAM") return "Waiting for stream";
  if (phase === "DRAIN" || phase === "FINALIZE") return "Finishing";
  if (phase === "CAPTURE") {
    const a = c.assessment;
    if (c.transport?.connected === false || (a && !(a.coverageComplete && a.decodeComplete))) return "Degraded";
    return a && c.transport?.connected === true ? "Live" : "Checking";
  }
  return "Unknown";
}
