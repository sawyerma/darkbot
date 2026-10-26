import { describe, expect, it, vi } from "vitest";
import { BinaryWriter, WireType } from "@bufbuild/protobuf/wire";
import captureHex from "../../../../../protocol/golden/project0/capture_state.hex?raw";
import { TopicId, type StateMeta } from "@/core/realtime";
import { useProject0Store } from "../store/project0Store";
import { decodeCaptureState } from "./project0ViewAdapter";

const { listeners } = vi.hoisted(() => ({ listeners: new Map<number, (bytes: Uint8Array, meta: StateMeta) => void>() }));
vi.mock("@/core/realtime", async (original) => ({
  ...(await original<typeof import("@/core/realtime")>()),
  topics: {
    onState: (topic: number, fn: (bytes: Uint8Array, meta: StateMeta) => void) => { listeners.set(topic, fn); return () => {}; },
    onTopicStatus: () => () => {},
    acquire: () => () => {},
  },
}));

const golden = Uint8Array.from(captureHex.trim().match(/../g)!.map((h) => parseInt(h, 16)));
/** The golden state with its frame counter set to `n`: a trailing field the decoder merges
 *  (capture.proto: CaptureState.transport = 3, TransportStatus.application_frames = 3). */
function withFrames(n: bigint): Uint8Array {
  const w = new BinaryWriter();
  w.tag(3, WireType.LengthDelimited).fork().tag(3, WireType.Varint).uint64(n).join();
  return new Uint8Array([...golden, ...w.finish()]);
}

describe("Project 0 adapter", () => {
  it("P0-012: a baseline and its successor within one frame both count for the stream's progress", async () => {
    expect(decodeCaptureState(withFrames(6n)).transport?.applicationFrames).toBe(6n);
    const frames: FrameRequestCallback[] = [];
    vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => frames.push(cb));
    const { ensureProject0Adapter } = await import("./project0Adapter");
    ensureProject0Adapter();
    const meta = { sourceId: "s", generation: 1n } as StateMeta;
    const deliver = listeners.get(TopicId.PROJECT0_CAPTURE_STATE)!;
    deliver(withFrames(5n), meta);
    deliver(withFrames(6n), meta);
    frames.splice(0).forEach(cb => cb(0));
    const s = useProject0Store.getState();
    // The view is the newest state; the progress saw both.
    expect(s.capture.value?.transport?.applicationFrames).toBe(6n);
    expect(s.streamProgress.frames).toBe(6n);
    expect(s.streamProgress.progressAtMs).not.toBeNull();
    vi.unstubAllGlobals();
  });
});
