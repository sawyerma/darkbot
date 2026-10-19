import { describe, expect, it } from "vitest";
import { NO_STREAM_PROGRESS, nextStreamProgress, streamFreshness, STREAM_STALE_AFTER_MS } from "./streamFreshness";
import { useProject0Store } from "../store/project0Store";
import type { CaptureView } from "./status";
import type { StateMeta } from "@/core/realtime";
describe("stream evidence", () => {
  it("requires growth and ages without incoming frames", () => {
    const first = nextStreamProgress(NO_STREAM_PROGRESS, 5n, 100, "run1/g1");
    expect(streamFreshness({ ...first, nowMs: 100 })).toBe("CHECKING");
    const advanced = nextStreamProgress(first, 6n, 200, "run1/g1");
    expect(streamFreshness({ ...advanced, nowMs: 200 })).toBe("FRESH");
    expect(nextStreamProgress(advanced, 6n, 300, "run1/g1")).toBe(advanced);
    expect(streamFreshness({ ...advanced, nowMs: 201 + STREAM_STALE_AFTER_MS })).toBe("STALE");
    expect(nextStreamProgress(advanced, 1n, 400, "run1/g1").progressAtMs).toBeNull();
  });
  it("resets atomically in the store on a new run OR generation, even with larger counters", () => {
    const store = useProject0Store.getState();
    const capture = (runId: string, frames: bigint) => ({ run: { runId }, transport: { applicationFrames: frames } } as CaptureView);
    const meta = (generation: bigint) => ({ sourceId: "source", generation } as StateMeta);
    store.applyCapture(capture("a", 1n), meta(1n), 100);
    store.applyCapture(capture("a", 2n), meta(1n), 200);
    expect(useProject0Store.getState().streamProgress.progressAtMs).toBe(200);
    store.applyCapture(capture("b", 20n), meta(1n), 300);
    expect(useProject0Store.getState().streamProgress.progressAtMs).toBeNull();
    store.applyCapture(capture("b", 21n), meta(1n), 400);
    store.applyCapture(capture("b", 100n), meta(2n), 500);
    expect(useProject0Store.getState().streamProgress.progressAtMs).toBeNull();
  });
});
