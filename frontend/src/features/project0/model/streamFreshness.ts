/** ws_rpc.rs emits an ingress health report at most every 5 s and only when a frame arrives;
 *  every report carries the application-frame counter, which grows only with subscription data. */
export const INGRESS_HEALTH_INTERVAL_MS = 5_000;
/** Two missed reports plus reporter batching, gateway and render latency (well below 1 s). */
export const STREAM_STALE_AFTER_MS = 2 * INGRESS_HEALTH_INTERVAL_MS + 1_000;

export interface StreamProgress {
  progressAtMs: number | null;
  firstSeenAtMs: number | null;
}
export interface StreamClock extends StreamProgress {
  nowMs: number;
}
export interface StreamProgressState extends StreamProgress {
  frames: bigint | null;
  identity: string | null;
}
export const NO_STREAM_PROGRESS: StreamProgressState = { identity: null, frames: null, progressAtMs: null, firstSeenAtMs: null };

/** UI-side bookkeeping of the application-frame counter. A first value (page load) or a counter that
 *  restarted (new run) has an unknown age and is never treated as fresh. */
export function nextStreamProgress(prev: StreamProgressState, frames: bigint | null, nowMs: number, identity: string): StreamProgressState {
  if (frames === null) return prev.frames === null ? prev : NO_STREAM_PROGRESS;
  if (prev.identity !== identity || prev.frames === null || frames < prev.frames) return { identity, frames, progressAtMs: null, firstSeenAtMs: nowMs };
  if (frames === prev.frames) return prev;
  return { ...prev, frames, progressAtMs: nowMs };
}

export type StreamFreshness = "CHECKING" | "FRESH" | "STALE";

export function streamFreshness(clock: StreamClock): StreamFreshness {
  if (clock.firstSeenAtMs === null) return "CHECKING";
  const since = clock.progressAtMs ?? clock.firstSeenAtMs;
  if (clock.nowMs - since > STREAM_STALE_AFTER_MS) return "STALE";
  return clock.progressAtMs === null ? "CHECKING" : "FRESH";
}
