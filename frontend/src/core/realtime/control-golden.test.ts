import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { fromBinary, toBinary, type DescMessage } from "@bufbuild/protobuf";
import { ServerMessageSchema } from "../generated/darkbot/control/v1/stream_pb";
import { CommandRequestSchema, CommandResponseSchema, CommandPhase } from "../generated/darkbot/control/v1/commands_pb";
import { OverviewAggregatesSchema } from "../generated/darkbot/control/v1/overview_pb";

// Rust writes these bytes (gateway/tests/control_goldens.rs); TS must read them exactly.
const golden = (name: string) => Uint8Array.from(Buffer.from(readFileSync(new URL(`../../../../protocol/golden/control/${name}.hex`, import.meta.url), "utf8").trim(), "hex"));
const U64_MAX = 18446744073709551615n;
const roundTrip = <S extends DescMessage>(schema: S, name: string) => {
  const bytes = golden(name);
  const msg = fromBinary(schema, bytes);
  expect(toBinary(schema, msg)).toEqual(bytes);
  return msg;
};

describe("control protocol golden bytes (Wire V1.3 §8/§9)", () => {
  it("keeps u64/i64 limits exact", () => {
    const state = roundTrip(ServerMessageSchema, "state_full");
    expect(state.msg.case).toBe("stateFull");
    if (state.msg.case !== "stateFull") return;
    expect([state.msg.value.generation, state.msg.value.seq]).toEqual([U64_MAX, U64_MAX]);
    expect(state.msg.value.sourceTimestampUnixNs).toBe(-9223372036854775808n);
    const hb = roundTrip(ServerMessageSchema, "heartbeat");
    if (hb.msg.case !== "heartbeat") throw Error("heartbeat expected");
    expect(hb.msg.value.heartbeatId).toBe(U64_MAX);
    expect(hb.msg.value.stateWatermarks[0]?.generation).toBe(U64_MAX);
    roundTrip(ServerMessageSchema, "welcome");
  });
  it("reads commands with the CAS version and the ledger phase", () => {
    const req = roundTrip(CommandRequestSchema, "command_request");
    expect(req.expected?.version).toEqual({ case: "profileVersion", value: U64_MAX });
    expect(req.issuedAtUnixNs).toBe(-9223372036854775808n);
    expect(roundTrip(CommandResponseSchema, "command_response").phase).toBe(CommandPhase.PENDING_RECONCILE);
  });
  it("re-encodes overview.aggregates byte-identically (semantics: overview-golden.test.ts)", () => {
    expect(roundTrip(OverviewAggregatesSchema, "overview_aggregates").projects.length).toBe(1);
  });
});
