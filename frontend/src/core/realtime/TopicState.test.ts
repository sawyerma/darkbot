import { describe, it, expect } from "vitest";
import { create } from "@bufbuild/protobuf";
import { AckReason, AckStatus, ControlAckSchema, StateFullSchema, StateQuality, StateWatermarkSchema } from "../generated/darkbot/control/v1/stream_pb";
import { TopicClass, TopicId } from "../generated/darkbot/control/v1/topics_pb";
import { TopicTracker } from "./TopicState";

const frame = (generation: bigint, seq: bigint, quality = StateQuality.OK) =>
  create(StateFullSchema, { topic: TopicId.BSC_STATUS, generation, seq, quality });
const tracker = () => {
  const t = new TopicTracker(TopicId.BSC_STATUS, TopicClass.STATE);
  t.markSubscribing();
  return t;
};

describe("STATE apply rules on the client (Wire V1.3 §2, Phasenplan §9.4)", () => {
  it("accepts a new publisher generation with a lower seq and never re-applies an old one", () => {
    const t = tracker();
    expect(t.onStateFull(frame(1n, 50n))).toBe("APPLY");
    expect(t.onStateFull(frame(2n, 1n))).toBe("APPLY");
    expect(t.applied).toEqual({ generation: 2n, seq: 1n });
    expect(t.onStateFull(frame(1n, 99n))).toBe("DISCARD_OLD_GENERATION");
    expect(t.onStateFull(frame(2n, 0n))).toBe("DISCARD_OLD_SEQ");
    expect(t.applied).toEqual({ generation: 2n, seq: 1n });
  });
  it("applies a quality overlay on the same (generation, seq) and never shows it as LIVE", () => {
    const t = tracker();
    t.onStateFull(frame(3n, 7n));
    expect(t.status).toBe("LIVE");
    expect(t.onStateFull(frame(3n, 7n, StateQuality.SOURCE_DETACHED))).toBe("APPLY");
    expect([t.status, t.statusReason]).toEqual(["DETACHED", "SOURCE_DETACHED"]);
    t.onStateFull(frame(3n, 7n, StateQuality.STALE));
    expect(t.status).toBe("STALE");
    t.onStateFull(frame(3n, 8n, StateQuality.UNKNOWN));
    expect([t.status, t.statusReason]).toEqual(["STALE", "SOURCE_UNVERIFIED"]);
  });
  it("marks a missed local frame from the heartbeat watermark as STALE", () => {
    const t = tracker();
    t.onStateFull(frame(4n, 2n));
    const wm = (generation: bigint, highestSeqSent: bigint) => create(StateWatermarkSchema, { topic: TopicId.BSC_STATUS, generation, highestSeqSent });
    expect(t.onWatermark(wm(4n, 2n))).toBe(false);
    expect(t.status).toBe("LIVE");
    expect(t.onWatermark(wm(4n, 3n))).toBe(true);
    expect([t.status, t.statusReason]).toEqual(["STALE", "WATERMARK_AHEAD"]);
  });
  it("turns a lost connection into STALE and a rejected subscription into a stable UNAVAILABLE", () => {
    const t = tracker();
    t.onStateFull(frame(1n, 1n));
    t.markConnectionLost();
    expect([t.status, t.statusReason]).toEqual(["STALE", "CONNECTION_LOST"]);
    const r = tracker();
    expect(r.onAck(create(ControlAckSchema, { status: AckStatus.REJECTED, reason: AckReason.TOPIC_UNAVAILABLE }))).toBe("REJECTED");
    expect(r.status).toBe("UNAVAILABLE");
  });
});
