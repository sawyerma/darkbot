import { describe, it, expect } from "vitest";
import { create, type MessageInitShape } from "@bufbuild/protobuf";
import {
  AckReason, AckStatus, ErrorCode, ServerMessageSchema, StateQuality, WelcomeStatus,
  type ClientMessage, type ServerMessage, type Welcome,
} from "../generated/darkbot/control/v1/stream_pb";
import { TopicId } from "../generated/darkbot/control/v1/topics_pb";
import { DEFAULT_LIMITS, type ConnectionInfo, type GatewayConnection } from "./GatewayConnection";
import { TopicSubscriptions } from "./TopicSubscriptions";
import { bindingOf } from "./topicTable";

const TOPIC = TopicId.BSC_LAUNCH_STATE;

/** The shared subscription layer over a scripted connection: real frames, no socket. */
function harness() {
  let frame: (m: ServerMessage) => void = () => {};
  let welcome: (w: Welcome) => void = () => {};
  let status: (i: ConnectionInfo) => void = () => {};
  const sent: ClientMessage["msg"][] = [];
  let id = 0;
  let isOpen = false; // like GatewayConnection: OPEN only from the Welcome on
  const conn = {
    onFrame: (fn: typeof frame) => { frame = fn; return () => {}; },
    onWelcome: (fn: typeof welcome) => { welcome = fn; return () => {}; },
    onStatus: (fn: typeof status) => { status = fn; return () => {}; },
    get isOpen() { return isOpen; },
    limits: DEFAULT_LIMITS,
    nextRequestId: () => ++id,
    send: (m: { msg: ClientMessage["msg"] }) => { sent.push(m.msg); return true; },
    blockProtocol: () => { throw Error("connection must not be blocked"); },
    forceReconnect: () => { throw Error("no reconnect expected"); },
  } as unknown as GatewayConnection;
  const topics = new TopicSubscriptions(conn);
  const b = bindingOf(TOPIC)!;
  const open = () => { isOpen = true; welcome(create(ServerMessageSchema, { msg: { case: "welcome", value: {
    status: WelcomeStatus.ACCEPTED, protocolMajor: 1,
    topics: [{ topic: TOPIC, class: b.cls, project: b.project, wireName: b.wireName, payloadType: b.payloadType, available: true }],
  } } }).msg.value as Welcome); };
  const push = (msg: MessageInitShape<typeof ServerMessageSchema>["msg"]) => frame(create(ServerMessageSchema, { msg }));
  const subscribes = () => sent.filter(m => m.case === "subscribe" || m.case === "unsubscribe");
  const ack = () => {
    const s = sent.filter(m => m.case === "subscribe").at(-1)!;
    push({ case: "controlAck", value: { requestId: s.value.requestId, topic: TOPIC, status: AckStatus.ACCEPTED, reason: AckReason.UNSPECIFIED } });
  };
  const state = (seq: bigint) => push({ case: "stateFull", value: { topic: TOPIC, generation: 5n, seq, quality: StateQuality.OK, sourceId: "bsc", payload: new Uint8Array([seq > 255n ? 255 : Number(seq)]) } });
  const tooLarge = () => push({ case: "error", value: { topic: TOPIC, code: ErrorCode.FRAME_TOO_LARGE, retryable: false, message: "state too large" } });
  const down = () => { isOpen = false; status({ status: "STALE" } as ConnectionInfo); };
  return { topics, open, push, subscribes, ack, state, tooLarge, down };
}

describe("FRAME_TOO_LARGE recovery on the shared subscription (Wire V1.3 §2)", () => {
  it("is never LIVE while too large, keeps the subscription and recovers with the next valid state", () => {
    const h = harness();
    const applied: number[] = [];
    let seen: [string, string] = ["", ""];
    h.topics.onState(TOPIC, (bytes) => applied.push(bytes[0]!));
    h.topics.onTopicStatus(TOPIC, (s, r) => { seen = [s, r]; });
    h.topics.acquire(TOPIC);
    h.open();
    h.ack();
    h.state(1n);
    expect(seen[0]).toBe("LIVE");
    const before = h.subscribes().length;
    h.tooLarge();
    expect(seen).toEqual(["UNAVAILABLE", "FRAME_TOO_LARGE state too large"]);
    h.tooLarge(); // a repeated notice changes nothing and triggers no retry
    expect(h.subscribes().length).toBe(before);
    h.state(3n);
    expect(seen[0]).toBe("LIVE");
    expect(applied).toEqual([1, 3]);
    expect(h.subscribes().length).toBe(before);
  });
  it("re-subscribes a too-large topic once after a reconnect, unlike a fatal topic error", () => {
    const h = harness();
    h.topics.onState(TOPIC, () => {});
    h.topics.acquire(TOPIC);
    h.open();
    h.ack();
    h.tooLarge();
    h.down();
    const before = h.subscribes().length;
    h.open();
    expect(h.subscribes().length).toBe(before + 1);
    h.ack();
    h.state(4n);
    // A fatal topic error stays UNAVAILABLE across a reconnect: no resubscribe loop.
    let seen = "";
    h.topics.onTopicStatus(TOPIC, (s) => { seen = s; });
    expect(seen).toBe("LIVE");
    h.push({ case: "error", value: { topic: TOPIC, code: ErrorCode.TOPIC_UNAVAILABLE, retryable: false, message: "" } });
    h.down();
    const fatal = h.subscribes().length;
    h.open();
    expect(h.subscribes().length).toBe(fatal);
    expect(seen).toBe("UNAVAILABLE");
  });
});
