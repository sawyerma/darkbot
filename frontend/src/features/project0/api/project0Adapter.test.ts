import { describe, expect, it, vi } from "vitest";
import { create, toBinary } from "@bufbuild/protobuf";
import { CaptureStateSchema } from "@/core/generated/darkbot/project0/v1/capture_pb";
import { TopicId, type StateMeta } from "@/core/realtime";
import { useProject0Store } from "../store/project0Store";

const { listeners } = vi.hoisted(() => ({ listeners: new Map<number, (bytes: Uint8Array, meta: StateMeta) => void>() }));
vi.mock("@/core/realtime", async (original) => ({
  ...(await original<typeof import("@/core/realtime")>()),
  topics: {
    onState: (topic: number, fn: (bytes: Uint8Array, meta: StateMeta) => void) => { listeners.set(topic, fn); return () => {}; },
    onTopicStatus: () => () => {},
    acquire: () => () => {},
  },
}));

describe("Project 0 adapter", () => {
  it("P0-012: a baseline and its successor within one frame both count for the stream's progress", async () => {
    const frames: FrameRequestCallback[] = [];
    vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => frames.push(cb));
    const { ensureProject0Adapter } = await import("./project0Adapter");
    ensureProject0Adapter();
    const state = (n: bigint) => toBinary(CaptureStateSchema, create(CaptureStateSchema, { run: { runId: "r" }, transport: { applicationFrames: n } }));
    const meta = { sourceId: "s", generation: 1n } as StateMeta;
    const deliver = listeners.get(TopicId.PROJECT0_CAPTURE_STATE)!;
    deliver(state(5n), meta);
    deliver(state(6n), meta);
    frames.splice(0).forEach(cb => cb(0));
    const s = useProject0Store.getState();
    // The view is the newest state; the progress saw both.
    expect(s.capture.value?.transport?.applicationFrames).toBe(6n);
    expect(s.streamProgress.frames).toBe(6n);
    expect(s.streamProgress.progressAtMs).not.toBeNull();
    vi.unstubAllGlobals();
  });
});
