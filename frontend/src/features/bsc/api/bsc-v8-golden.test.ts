import { describe, expect, it } from "vitest";
import { readdirSync, readFileSync } from "node:fs";
import { classify } from "@/core/realtime/bscQuery";
import { Availability, DataQuality, ExitReason, TimeSource, TradePlacement, TradeStatus, TransferStatus, TrainingRunStopKind, V8_CONTRACTS, decodeV8, encodeV8, type V8ContractName } from "./bscV8Adapter";

// BSC-V8-09 / FE-V8-17: the prepared V8 contracts, Rust↔TS. gateway/tests/bsc_v8_goldens.rs
// encodes the bytes with prost; here protobuf-es decodes them, the values survive exactly (u64
// and i64 limits, >u128 signed wei text, absent vs present zero) and re-encode byte for byte.
const dir = new URL("../../../../../protocol/golden/bsc/v8/", import.meta.url);
const golden = (name: string) => Uint8Array.from(Buffer.from(readFileSync(new URL(`${name}.hex`, dir), "utf8").trim(), "hex"));
const U256_MAX = "115792089237316195423570985008687907853269984665640564039457584007913129639935";
const U64_MAX = 18446744073709551615n;
const I64_MIN = -9223372036854775808n;

describe("prepared BSC V8 contracts (Rust golden bytes)", () => {
  it("has a fixture for every contract and no stray fixture", () => {
    const files = readdirSync(dir).filter(f => f.endsWith(".hex")).map(f => f.slice(0, -4)).sort();
    expect(files).toEqual(Object.keys(V8_CONTRACTS).sort());
  });

  it("re-encodes every fixture byte for byte", () => {
    for (const name of Object.keys(V8_CONTRACTS) as V8ContractName[]) {
      const bytes = golden(name);
      expect(Buffer.from(encodeV8(name, decodeV8(name, bytes))).toString("hex"), name).toBe(Buffer.from(bytes).toString("hex"));
    }
  });

  it("keeps the trade row's exact values and its absent ones absent", () => {
    const page = decodeV8("trade_page", golden("trade_page"));
    expect(page.result.case).toBe("trades");
    if (page.result.case !== "trades") return;
    const [closed, open] = page.result.value.rows;
    if (!closed || !open) throw new Error("trade_page: two rows expected");
    expect(closed.status).toBe(TradeStatus.CLOSED);
    expect(closed.pnlWei).toBe(`-${U256_MAX}`);
    expect(closed.roiPpm).toBe(I64_MIN);
    expect(closed.leadBeforeBlockUs).toBe(I64_MIN);
    expect(closed.detectToSendNs).toBe(U64_MAX);
    expect(closed.sendToAckNs).toBeUndefined();
    expect(closed.holdMs).toBe(0n);
    expect(closed.proceedsUnknown).toBe(true);
    expect(closed.placement).toBe(TradePlacement.FIRST);
    expect(closed.exitReason).toBe(ExitReason.TARGET);
    expect(closed.buyTime?.source).toBe(TimeSource.BLOCK_HEADER);
    expect(closed.buyTime?.blockNumber).toBe(U64_MAX);
    expect(open.pnlWei).toBeUndefined();
    expect(open.roiPpm).toBeUndefined();
    expect(open.sellTime?.unixMs).toBeUndefined();
    expect(open.quality).toBe(DataQuality.UNRESOLVED);
    expect(page.result.value.counts?.writtenOff).toBe(U64_MAX);
  });

  it("keeps the 15 filters with their bounds and the read-only first-buyer window", () => {
    const s = decodeV8("strategy_settings_state", golden("strategy_settings_state"));
    expect(s.stored?.filters.map(f => f.feature)).toHaveLength(15);
    expect(s.stored?.filters.find(f => f.feature === "net_buy_flow_bnb")?.min).toBeUndefined();
    expect(s.stored?.filters.find(f => f.feature === "buy_count")).toMatchObject({ enabled: true, min: "3" });
    expect(s.stored?.filters.find(f => f.feature === "curve_progress_pct")?.enabled).toBe(false);
    expect(s.stored?.inputWei).toBe(U256_MAX);
    expect(s.stored?.observationMs).toBe(0n);
    expect(s.stored?.firstBuyerWindow).toBe(100);
    expect(s.stored?.fillDelayBlocks).toBeUndefined();
    expect(s.running).toBeUndefined();
    expect(s.storedRevision).toBe(U64_MAX);
  });

  it("keeps an unknown transfer unknown and an absent fee absent", () => {
    const t = decodeV8("treasury_state", golden("treasury_state"));
    expect(t.sweep?.lastExecution?.status).toBe(TransferStatus.UNKNOWN);
    expect(t.sweep?.lastExecution?.feeWei).toBeUndefined();
    expect(t.balances[0]?.reservedWei).toBeUndefined();
    expect(t.balances[0]?.availableWei).toBe("0");
    expect(t.provenance?.quality).toBe(DataQuality.STALE);
  });

  it("keeps the run plans and an unresolved net result absent", () => {
    const p = decodeV8("training_run_page", golden("training_run_page"));
    expect(p.runs.map(r => r.plan?.stopKind)).toEqual([TrainingRunStopKind.UNTIL_STOPPED, TrainingRunStopKind.DURATION]);
    expect(p.runs[0]?.result?.netWei).toBeUndefined();
    expect(p.runs[1]?.result?.netWei).toBe("-1");
    expect(p.runs[1]?.plan?.maxLaunches).toBe(0n);
  });

  it("reads NOT_IMPLEMENTED as such", () => {
    expect(decodeV8("query_not_implemented", golden("query_not_implemented")).availability).toBe(Availability.NOT_IMPLEMENTED);
    expect(decodeV8("watch_not_implemented", golden("watch_not_implemented")).availability).toBe(Availability.NOT_IMPLEMENTED);
  });
});

describe("bounded BSC query transport (no route yet)", () => {
  const pb = "application/x-protobuf";
  it("treats the missing route as NOT_IMPLEMENTED and never as data", () => {
    expect(classify(405, "text/plain", null)).toMatchObject({ kind: "NOT_IMPLEMENTED" });
    expect(classify(404, "text/plain", null)).toMatchObject({ kind: "NOT_IMPLEMENTED" });
    expect(classify(503, null, null)).toMatchObject({ kind: "UNAVAILABLE", reason: "HTTP 503" });
    // The SPA fallback's HTML is never decoded as an answer.
    expect(classify(200, "text/html", new TextEncoder().encode("<!doctype html>"))).toMatchObject({ kind: "UNAVAILABLE", reason: "QUERY_ANSWER_NOT_PROTOBUF" });
  });
  it("passes the owner's NOT_IMPLEMENTED and its result through", () => {
    expect(classify(200, pb, golden("query_not_implemented"))).toEqual({ kind: "NOT_IMPLEMENTED", reason: "BSC-V8-10" });
    const answer = classify(200, pb, golden("trade_page"));
    expect(answer.kind).toBe("ANSWER");
    if (answer.kind === "ANSWER") expect(answer.result.case).toBe("trades");
  });
});
