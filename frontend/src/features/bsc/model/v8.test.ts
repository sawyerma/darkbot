import { describe, expect, it } from "vitest";
import { bnbToWei, changes, filterOrder, formOf, msToSeconds, parseFilters, saveRequest, secondsToMs, serializeFilters, weiToBnb, FILTER_KEYS } from "./strategyForm";
import { blockClock, counts, filterTrades, nsToMs, safeColumns, tradeLabel, tradeViews } from "./trades";
import { share, usToMs, detection, CENSUS_SCHEMA } from "./monitor";
import { capital, pendingTransactions, tradingState } from "./overview";
import { healthView } from "./health";
import { RUNTIME_SCHEMA, SOURCE_SCHEMA } from "./sources";
import { SETTINGS_SCHEMA, strategySettingsView } from "./strategySettings";
import type { BscLiveView, BscWalletsView, LivePositionView, NativeRecordView } from "./types";

const rec = (schema: string, fields: Record<string, string>): NativeRecordView =>
  ({ schema, provenance: "NATIVE_CREATED", fields: Object.entries(fields).map(([name, value]) => ({ name, kind: 7, value, unit: "" })) });
const live = (over: Partial<BscLiveView>): BscLiveView => ({ gate: "BLOCKED", blockers: [], preconditions: [], generation: 0n, totalLimit: 0n, totalUsed: 0n, grantAttemptsUsed: 0n, spentWei: "0",
  qualification: "NONE", qualificationDetail: "", pathState: "OFF", pathReason: "", route: "", buyer: "", released: 0n, unknown: 0n, lastRefusal: "", positions: [], transactions: [],
  realizedPnlWei: "0", gasWei: "0", warnings: [], scope: [], grantableSources: [], attempts: [], exitUndecided: [], stranded: [], lotReadFaults: 0n, lotReadFault: "",
  recoveryPending: [], recovered: 0n, recoveryFaults: 0n, recoveryFault: "", refusals: [], responses: [], included: 0n, reverted: 0n, replaced: 0n, finalized: 0n, orphaned: 0n,
  probes: 0n, ledgerError: "", latencySamples: 0n, leaseAttempt: "", confirmState: "", presenceSigner: "", qualificationRiskScopeSha256: "", ...over });
const pos = (over: Partial<LivePositionView>): LivePositionView => ({ lot: "0xa", token: "0xt", component: "BSC_STRATEGY_C", state: "CLOSED", tokens: "1", inputWei: "1", proceedsWei: "0", gasWei: "0", decisions: [], proceedsUnknown: false, ...over });

describe("V8 Strategy C form: exact conversions, the native filter string", () => {
  it("converts wei and BNB exactly, 18 places at most, no sign", () => {
    expect(weiToBnb("100000000000000000")).toBe("0.1");
    expect(weiToBnb("1")).toBe("0.000000000000000001");
    expect(weiToBnb("115792089237316195423570985008687907853269984665640564039457584007913129639935")).toBe("115792089237316195423570985008687907853269984665640564039457.584007913129639935");
    expect(bnbToWei("0.1")).toEqual({ wei: "100000000000000000" });
    expect(bnbToWei("0,000000000000000001")).toEqual({ wei: "1" });
    expect(bnbToWei("0.0000000000000000001")).toHaveProperty("error");
    expect(bnbToWei("-1")).toHaveProperty("error");
    expect(bnbToWei("1e3")).toHaveProperty("error");
  });
  it("converts ms and seconds exactly, 3 places at most", () => {
    expect(msToSeconds("3000")).toBe("3");
    expect(msToSeconds("1")).toBe("0.001");
    expect(secondsToMs("60.5")).toEqual({ ms: "60500" });
    expect(secondsToMs("0.0001")).toHaveProperty("error");
  });
  it("parses and serializes BSC_STRATEGY_C_FILTERS in native order, disabled rules left out", () => {
    const p = parseFilters("unique_buyer_ratio:0.5:1, buy_count:3:");
    expect("filters" in p && p.filters.buy_count).toEqual({ on: true, min: "3", max: "" });
    expect("filters" in p && serializeFilters(p.filters)).toEqual({ text: "buy_count:3:,unique_buyer_ratio:0.5:1" });
    expect(parseFilters("provider_gap_flag::1")).toHaveProperty("error");
    expect(parseFilters("buy_count:1:,buy_count:2:")).toHaveProperty("error");
    expect(parseFilters("buy_count:1")).toHaveProperty("error");
    const empty = Object.fromEntries(FILTER_KEYS.map(k => [k, { on: false, min: "", max: "" }]));
    // As the native parser: an enabled rule without bounds (`feature::`, NULL fails) and leading zeros.
    expect(serializeFilters({ ...empty, sell_count: { on: true, min: "", max: "" } })).toEqual({ text: "sell_count::" });
    expect(serializeFilters({ ...empty, buy_count: { on: true, min: "03", max: "" } })).toEqual({ text: "buy_count:03:" });
    expect(serializeFilters({ ...empty, buy_count: { on: true, min: "1.", max: "" } })).toHaveProperty("error");
    expect(serializeFilters({ ...empty, net_buy_flow_bnb: { on: true, min: "-0.5", max: "" } })).toEqual({ text: "net_buy_flow_bnb:-0.5:" });
    expect(FILTER_KEYS).toHaveLength(15);
    expect(FILTER_KEYS).not.toContain("provider_gap_flag");
  });
  it("names one native request per changed key and keeps an unchanged rule set's own spelling", () => {
    const settings = strategySettingsView([rec(SOURCE_SCHEMA, { generation: "4", "sources[0].source_id": "BSC_INGRESS_GETH" }),
      rec(SETTINGS_SCHEMA, { generation: "4", "settings[0].id": "BSC_STRATEGY_C_INPUT_WEI", "settings[0].value": "100000000000000000",
        "settings[1].id": "BSC_STRATEGY_C_FILTERS", "settings[1].value": "buy_count:3: , unique_buyers:2:" })], [], 0);
    const { form } = formOf(settings);
    expect(form.stake).toBe("0.1");
    expect(changes(settings, form)).toEqual({ changed: [] });
    expect(changes(settings, { ...form, stake: "0.25" })).toEqual({ changed: [{ key: "BSC_STRATEGY_C_INPUT_WEI", value: "250000000000000000" }] });
    expect(changes(settings, { ...form, stake: "0.1", hold: "1.5", target: "1.50" })).toEqual({ changed: [
      { key: "BSC_STRATEGY_C_TARGET_NET_MULTIPLE", value: "1.50" }, { key: "BSC_STRATEGY_C_HOLD_MS", value: "1500" }] });
    // An invalid filter edit refuses the filters only; a stored rule set the form cannot show blocks nothing.
    const bad = { ...form, filters: { ...form.filters, sell_count: { on: true, min: "x", max: "" } } };
    expect(changes(settings, { ...bad, stake: "0.25" })).toEqual({ changed: [{ key: "BSC_STRATEGY_C_INPUT_WEI", value: "250000000000000000" }], filterError: "sell_count: x is no exact decimal" });
    const odd = strategySettingsView([rec(SOURCE_SCHEMA, { generation: "1", "sources[0].source_id": "BSC_INGRESS_GETH" }),
      rec(SETTINGS_SCHEMA, { generation: "1", "settings[0].id": "BSC_STRATEGY_C_FILTERS", "settings[0].value": "not:a:filter:rule" })], [], 0);
    expect(changes(odd, { ...formOf(odd).form, maxOpen: "2" })).toEqual({ changed: [{ key: "BSC_STRATEGY_C_MAX_OPEN", value: "2" }] });
  });
  // TEST_ONLY records: the owner and its stored generation 6 (input, early-buyer window, two rules in the owner's order).
  const stored6 = (filters?: string) => strategySettingsView([rec(SOURCE_SCHEMA, { generation: "6", "sources[0].source_id": "BSC_INGRESS_GETH" }),
    rec(SETTINGS_SCHEMA, { generation: "6", "settings[0].id": "BSC_STRATEGY_C_INPUT_WEI", "settings[0].value": "100000000000000000",
      "settings[1].id": "BSC_STRATEGY_C_FIRST_BUYER_WINDOW", "settings[1].value": "5",
      ...(filters === undefined ? {} : { "settings[2].id": "BSC_STRATEGY_C_FILTERS", "settings[2].value": filters }) })], [], 0);
  it("takes the early-buyer window as the tenth key and never offers emptying as a return to the file's value", () => {
    const v = stored6("unique_buyers:2:,buy_count:3:");
    const { form } = formOf(v);
    expect(form.firstBuyerWindow).toBe("5");
    expect(changes(v, { ...form, firstBuyerWindow: " 8 " })).toEqual({ changed: [{ key: "BSC_STRATEGY_C_FIRST_BUYER_WINDOW", value: "8" }] });
    // No native unset: an emptied scalar is refused, an untouched empty one is no change.
    expect(changes(v, { ...form, stake: "" })).toEqual({ error: expect.stringContaining("BSC_STRATEGY_C_INPUT_WEI cannot be empty") });
    expect(changes(v, { ...form, firstBuyerWindow: "", maxOpen: "2" })).toHaveProperty("error");
    expect(changes(v, { ...form, hold: "" })).toEqual({ changed: [] });
    // Every rule off is the empty filter string: no rules (valid); a rule edit keeps the owner's order.
    const off = Object.fromEntries(FILTER_KEYS.map(k => [k, { on: false, min: "", max: "" }]));
    expect(changes(v, { ...form, filters: off })).toEqual({ changed: [{ key: "BSC_STRATEGY_C_FILTERS", value: "" }] });
    expect(changes(v, { ...form, filters: { ...form.filters, sell_count: { on: true, min: "", max: "2" } } })).toEqual({ changed: [{ key: "BSC_STRATEGY_C_FILTERS", value: "unique_buyers:2:,buy_count:3:,sell_count::2" }] });
    expect(filterOrder(" buy_count:3: ,x:1:,unique_buyers::").slice(0, 3)).toEqual(["buy_count", "unique_buyers", "sell_count"]);
    expect(filterOrder("")).toEqual(FILTER_KEYS);
  });
  it("saves one change as bsc.strategy.set and several as one set_all with the complete filter list", () => {
    const v = stored6("unique_buyers:2:,buy_count:3:");
    const { form } = formOf(v);
    expect(saveRequest(v, form)).toEqual({ kind: "none" });
    expect(saveRequest(v, { ...form, maxOpen: "2" })).toEqual({ kind: "one", key: "BSC_STRATEGY_C_MAX_OPEN", value: "2" });
    const r = saveRequest(v, { ...form, maxOpen: "2", firstBuyerWindow: "8", hold: "1.5", creatorSellExit: "DISABLED",
      filters: { ...form.filters, unique_buyers: { on: false, min: "2", max: "" }, sell_count: { on: true, min: "", max: "0,5" } } });
    if (r.kind !== "all") throw new Error(r.kind);
    expect(r.changed.map(c => c.key)).toEqual(["BSC_STRATEGY_C_HOLD_MS", "BSC_STRATEGY_C_MAX_OPEN", "BSC_STRATEGY_C_FIRST_BUYER_WINDOW", "BSC_STRATEGY_C_CREATOR_SELL_EXIT", "BSC_STRATEGY_C_FILTERS"]);
    expect(r.filters).toBe("buy_count:3:,sell_count::0.5");
    // Absent keys stay as stored; typed values; every feature sent, the owner's order first, disabled ones without bounds.
    expect({ ...r.values, filters: undefined }).toEqual({ holdMs: 1500n, maxOpen: 2, firstBuyerWindow: 8, creatorSellExit: "DISABLED", filters: undefined });
    expect(r.values.filters).toHaveLength(15);
    expect(r.values.filters.slice(0, 3)).toEqual([{ feature: "unique_buyers", enabled: false }, { feature: "buy_count", enabled: true, min: "3" }, { feature: "sell_count", enabled: true, max: "0.5" }]);
    // A count the request cannot carry is refused here; any other bound is the native parser's.
    expect(saveRequest(v, { ...form, maxOpen: "2.5", firstBuyerWindow: "8" })).toEqual({ kind: "refused", reason: expect.stringContaining("no whole number") });
    expect(saveRequest(v, { ...form, maxOpen: "4294967296", firstBuyerWindow: "8" })).toHaveProperty("kind", "refused");
    expect(saveRequest(v, { ...form, maxOpen: "2.5" })).toEqual({ kind: "one", key: "BSC_STRATEGY_C_MAX_OPEN", value: "2.5" });
  });
  it("refuses a combined save while the filters in effect are unknown or unshowable", () => {
    const none = stored6();
    const f = formOf(none).form;
    expect(saveRequest(none, { ...f, maxOpen: "2", firstBuyerWindow: "8" })).toEqual({ kind: "refused", reason: expect.stringContaining("filters in effect are not reported") });
    expect(saveRequest(none, { ...f, firstBuyerWindow: "8" })).toEqual({ kind: "one", key: "BSC_STRATEGY_C_FIRST_BUYER_WINDOW", value: "8" });
    // Running without a stored rule set: the running strategy's filters are the basis.
    const running = strategySettingsView([rec(SOURCE_SCHEMA, { generation: "6", "sources[0].source_id": "BSC_INGRESS_GETH" }), rec(SETTINGS_SCHEMA, { generation: "6" }),
      rec(RUNTIME_SCHEMA, { "_provenance.observed_unix_ms": "1800000000000", status_interval_ms: "1000", status_stale_intervals: "3", "strategy[0].state": "SHADOW", "strategy[0].settings_generation": "6",
        "strategy[0].settings[0].id": "BSC_STRATEGY_C_FILTERS", "strategy[0].settings[0].value": "buy_count:3:" })], [], 1_800_000_000_000);
    const rf = formOf(running).form;
    expect(saveRequest(running, { ...rf, maxOpen: "2", firstBuyerWindow: "8" })).toMatchObject({ kind: "all", filters: "buy_count:3:" });
    const odd = stored6("not:a:filter:rule");
    expect(saveRequest(odd, { ...formOf(odd).form, maxOpen: "2", firstBuyerWindow: "8" })).toEqual({ kind: "refused", reason: expect.stringContaining("stored filters cannot be shown") });
  });
});

describe("V8 trades: labels from the ledger state and the native PnL sign only", () => {
  it("never shows a zero, unknown or lower-bound loss as a WIN", () => {
    expect(tradeLabel(pos({ state: "OPEN" }))).toBe("RUN");
    expect(tradeLabel(pos({ state: "EXITING" }))).toBe("RUN");
    expect(tradeLabel(pos({ state: "PENDING" }))).toBe("PEND");
    expect(tradeLabel(pos({ state: "FAILED", pnlWei: "-5" }))).toBe("FAIL");
    expect(tradeLabel(pos({ pnlWei: "7" }))).toBe("WIN");
    expect(tradeLabel(pos({ pnlWei: "-7" }))).toBe("LOSS");
    expect(tradeLabel(pos({ pnlWei: "0" }))).toBe("DONE");
    expect(tradeLabel(pos({ pnlWei: undefined }))).toBe("UNKN");
    expect(tradeLabel(pos({ pnlWei: "-7", proceedsUnknown: true }))).toBe("UNKN");
    expect(tradeLabel(pos({ pnlWei: "7", proceedsUnknown: true }))).toBe("WIN");
    expect(tradeLabel(pos({ state: "WRITTEN_OFF", pnlWei: "-1001" }))).toBe("WOFF");
  });
  it("puts active lots first and joins only the attempt of the lot's own BUY", () => {
    const l = live({ positions: [pos({ lot: "0xA1", pnlWei: "1" }), pos({ lot: "0xB2", state: "OPEN" })],
      attempts: [{ tx: "0xb2", trigger: "BSC_INGRESS_GETH", ingressToSendNs: 420_000n } as never] });
    const rows = tradeViews(l);
    expect(rows.map(r => r.lot)).toEqual(["0xB2", "0xA1"]);
    expect(rows[0]?.attempt?.trigger).toBe("BSC_INGRESS_GETH");
    expect(rows[1]?.attempt).toBeUndefined();
    expect(counts(rows)).toEqual({ run: 1, pend: 0, win: 1, loss: 0 });
    expect(filterTrades(rows, { search: "0xa1", status: "all" }).map(r => r.lot)).toEqual(["0xA1"]);
    // Two lots of one token: each shows only its own BUY, never the other's transactions.
    const two = tradeViews(live({ positions: [pos({ lot: "0x01", token: "0xt" }), pos({ lot: "0x02", token: "0xt", state: "WRITTEN_OFF" })],
      transactions: [{ txHash: "0x01", token: "0xt" }, { txHash: "0x02", token: "0xt" }, { txHash: "0x03", token: "0xt" }] as never }));
    expect(two.map(r => [r.lot, r.transactions.map(t => t.txHash), r.sharedToken])).toEqual([["0x01", ["0x01"], true], ["0x02", ["0x02"], true]]);
    expect(counts(two).loss).toBe(1);
  });
  it("keeps only known columns, token always, and formats ns exactly", () => {
    expect(safeColumns(["pnl", "bogus", "pnl"])).toEqual(["token", "pnl"]);
    expect(safeColumns("x")).toEqual(["token"]);
    expect(nsToMs(420_000n)).toBe("0.42 ms");
    expect(nsToMs(undefined)).toBe("—");
    // A block time beyond the JS date range is no time, never a thrown RangeError.
    expect(blockClock(18446744073709551615n)).toBeNull();
    expect(blockClock(1793000000000n)).toEqual({ time: "07:33:20", date: "2026-10-26" });
  });
});

describe("V8 monitoring and overview from the native records", () => {
  it("shares and leads exactly, '—' without a sample", () => {
    expect(share("48", "116")).toBe("41.4 %");
    expect(share("1", "0")).toBe("—");
    expect(usToMs("157600")).toBe("157.6 ms");
    expect(usToMs("-100000")).toBe("−100.0 ms");
    expect(usToMs(undefined)).toBe("—");
    expect(detection([], "flap").total).toBeUndefined();
    expect(detection([rec(CENSUS_SCHEMA, { flap_launches: "116", full_before: "48" })], "flap")).toMatchObject({ total: "116", full: "48" });
  });
  it("is ON only with a clear gate and an armed path, and counts one wallet once", () => {
    expect(tradingState(null).label).toBe("Trading UNKNOWN");
    expect(tradingState(live({ gate: "CLEAR", pathState: "BLOCKED" })).label).toBe("Trading OFF");
    expect(tradingState(live({ gate: "CLEAR", pathState: "ARMED" })).label).toBe("Trading ON");
    // A stale or detached LIVE state is never ON.
    expect(tradingState(live({ gate: "CLEAR", pathState: "ARMED" }), "DETACHED").label).toBe("Trading UNKNOWN");
    const w = { launcherAddress: "0xAA", buyerAddress: "0xaa", launcherBalanceWei: "5", buyerBalanceWei: "5" } as BscWalletsView;
    expect(capital(w).totalWei).toBe("5");
    expect(capital({ ...w, buyerAddress: "0xbb" }).totalWei).toBe("10");
    expect(capital({ ...w, buyerAddress: "0xbb", launcherBalanceWei: undefined }).totalWei).toBeUndefined();
    expect(pendingTransactions(live({ transactions: [{ state: "UNKNOWN" }, { state: "INCLUDED" }, { state: "RELEASED" }] as never }))).toBe(2);
  });
});

describe("V8 Health: native states, nothing unknown is green", () => {
  const now = 1_793_000_000_000;
  const runtime = (over: Record<string, string>) => rec(RUNTIME_SCHEMA, { "_provenance.observed_unix_ms": String(now), status_interval_ms: "1000", status_stale_intervals: "3", mode: "SHADOW", ...over });
  const modes = rec(SOURCE_SCHEMA, { generation: "1", "sources[0].source_id": "BSC_INGRESS_GETH", "sources[0].effective": "SHADOW", "sources[1].source_id": "BSC_INGRESS_DRPC", "sources[1].effective": "OFF" });
  it("maps the source health and an off source", () => {
    const v = healthView({ gateway: "OPEN", statusTopic: "LIVE", status: null, live: null, nowMs: now,
      records: [modes, runtime({ "sources[0].source_id": "BSC_INGRESS_GETH", "sources[0].health": "DISCONNECTED", "canonical[0].state": "FOLLOWING", "canonical[0].confirm_endpoint": "x", "canonical[0].confirm_state": "DISAGREES" })] });
    const sources = v.groups.find(g => g.id === "sources")!.entries;
    expect(sources.map(e => [e.id, e.state])).toEqual([["BSC_INGRESS_GETH", "down"], ["BSC_INGRESS_DRPC", "off"]]);
    expect(v.groups[0]!.entries.find(e => e.id === "confirm")?.state).toBe("down");
    expect(v.severity).toBe("critical");
  });
  it("shows a stale runtime as warn and no runtime as unknown, never ok", () => {
    const stale = healthView({ gateway: "OPEN", statusTopic: "LIVE", status: null, live: null, nowMs: now + 60_000, records: [modes, runtime({ "canonical[0].state": "FOLLOWING" })] });
    expect(stale.groups[0]!.entries.find(e => e.id === "engine")?.state).toBe("warn");
    expect(stale.groups[0]!.entries.find(e => e.id === "chain")?.state).toBe("warn");
    const none = healthView({ gateway: "IDLE", statusTopic: "IDLE", status: null, live: null, nowMs: now, records: [] });
    expect(none.groups.flatMap(g => g.entries).some(e => e.state === "ok")).toBe(false);
    expect(none.severity).not.toBe("ok");
    expect(none.groups.find(g => g.id === "portals")!.entries).toEqual([]);
    // Sources healthy but no builder reported: not green; a gateway with a stale status: amber.
    const partial = healthView({ gateway: "OPEN", statusTopic: "STALE", status: null, live: null, nowMs: now,
      records: [modes, runtime({ "sources[0].source_id": "BSC_INGRESS_GETH", "sources[0].health": "HEALTHY", "canonical[0].state": "FOLLOWING" })] });
    expect(partial.groups[0]!.entries.find(e => e.id === "gateway")?.state).toBe("warn");
    expect(partial.severity).not.toBe("ok");
  });
});
