import { describe, expect, it } from "vitest";
import { create, type MessageInitShape } from "@bufbuild/protobuf";
import { bnbToWei, changes, featureHints, fieldRanges, filterOrder, formOf, msToSeconds, parseFilters, saveRequest, secondsToMs, serializeFilters, weiToBnb, FILTER_KEYS } from "./strategyForm";
import { Availability, CreatorSellExit, CreatorSoldBeforeFill, SettingsApplyStatus, V8_CONTRACTS } from "../api/bscV8Adapter";
import { blockClock, counts, filterTrades, nsToMs, safeColumns, tradeLabel, tradeViews } from "./trades";
import { share, usToMs, detection, CENSUS_SCHEMA } from "./monitor";
import { capital, pendingTransactions, tradingState } from "./overview";
import { healthView } from "./health";
import { RUNTIME_SCHEMA, SOURCE_SCHEMA } from "./sources";
import { SETTINGS_SCHEMA, generationText, strategySettingsView, withSettingsState } from "./strategySettings";
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

// TEST_ONLY: the shape of the reporter's `--query` answer for bsc/config/ingress.public.env with
// nothing stored (probed against ca4a5646): effective = the file's values, the parser's bounds, the
// feature catalogue in native order.
const NATIVE_FEATURES: [string, string, number, boolean][] = [["buy_count", "count", 0, false], ["sell_count", "count", 0, false], ["buy_volume_bnb", "bnb", 18, false],
  ["sell_volume_bnb", "bnb", 18, false], ["net_buy_flow_bnb", "bnb", 18, true], ["unique_buyers", "count", 0, false], ["unique_buyer_ratio", "ratio", 4, false],
  ["largest_buy_bnb", "bnb", 18, false], ["buyer_volume_concentration", "ratio", 4, false], ["first5_buyer_concentration", "ratio", 4, false],
  ["first_sell_latency_ms", "milliseconds", 0, false], ["price_change", "ratio", 4, true], ["max_drawdown", "ratio", 4, false], ["dev_initial_buy", "ratio", 4, false],
  ["curve_progress_pct", "percent", 4, false], ["provider_gap_flag", "flag", 0, false]];
const spec = (key: string, unit: string, min?: string, max?: string, places = 0) => ({ key: `BSC_STRATEGY_C_${key}`, unit, min, max, places });
type StateInit = MessageInitShape<typeof V8_CONTRACTS.strategy_settings_state>;
const FILE_VALUES = { inputWei: "10000000000000000", targetNetMultiple: "2", stopNetMultiple: "0.5", observationMs: 3000n, holdMs: 600000n, maxOpen: 5, firstBuyerWindow: 5,
  creatorSoldBeforeFill: CreatorSoldBeforeFill.FREE_CANCEL, creatorSellExit: CreatorSellExit.ENABLED, ratioPlaces: 4, fillDelayBlocks: 1, gasPriceWei: "50000000",
  filters: [{ feature: "buy_count", enabled: true, min: "2" }, { feature: "unique_buyers", enabled: true, min: "2" }] };
const STATE: StateInit = {
  availability: Availability.AVAILABLE, storedRevision: 0n, applyStatus: SettingsApplyStatus.NOT_RUNNING,
  provenance: { owner: "bsc_runtime strategy settings", provenance: "NATIVE_CREATED" },
  effective: FILE_VALUES,
  settingSpecs: [spec("OBSERVATION_MS", "ms", "0", "3600000"), spec("FILTERS", "rules"), spec("INPUT_WEI", "wei", "1", "18446744073709551615"),
    spec("TARGET_NET_MULTIPLE", "ratio", "1.0001", undefined, 4), spec("STOP_NET_MULTIPLE", "ratio", "0.0000", "0.9999", 4), spec("HOLD_MS", "ms", "1", "86400000"),
    spec("MAX_OPEN", "count", "1", "1000"), spec("CREATOR_SOLD_BEFORE_FILL", "enum"), spec("CREATOR_SELL_EXIT", "enum"), spec("FIRST_BUYER_WINDOW", "count", "1", "100")],
  features: NATIVE_FEATURES.map(([name, unit, places, signed], index) => ({ name, index, unit, places, signed, filterable: name !== "provider_gap_flag" })),
};
const settingsState = (over: StateInit = {}) => create(V8_CONTRACTS.strategy_settings_state, { ...STATE, ...over } as StateInit);
const answer = (value: ReturnType<typeof settingsState>) => ({ kind: "ANSWER" as const, requestId: "q", result: { case: "strategySettings" as const, value } });
// TEST_ONLY records: the owner with nothing stored and Strategy C not running.
const off0 = strategySettingsView([rec(SOURCE_SCHEMA, { generation: "4", "sources[0].source_id": "BSC_INGRESS_GETH" })], [], 0);

describe("V8 Strategy C settings state (bsc/query, BSC-V8-04): the form basis, native bounds, apply status", () => {
  it("shows the configuration file's values and saves several changes as one set_all with nothing stored", () => {
    const v = withSettingsState(off0, answer(settingsState()));
    expect(v.basis?.values).toEqual({ BSC_STRATEGY_C_OBSERVATION_MS: "3000", BSC_STRATEGY_C_FILTERS: "buy_count:2:,unique_buyers:2:", BSC_STRATEGY_C_INPUT_WEI: "10000000000000000",
      BSC_STRATEGY_C_TARGET_NET_MULTIPLE: "2", BSC_STRATEGY_C_STOP_NET_MULTIPLE: "0.5", BSC_STRATEGY_C_HOLD_MS: "600000", BSC_STRATEGY_C_MAX_OPEN: "5",
      BSC_STRATEGY_C_CREATOR_SOLD_BEFORE_FILL: "FREE_CANCEL", BSC_STRATEGY_C_CREATOR_SELL_EXIT: "ENABLED", BSC_STRATEGY_C_FIRST_BUYER_WINDOW: "5" });
    const { form } = formOf(v);
    expect({ ...form, filters: undefined }).toEqual({ stake: "0.01", target: "2", stop: "0.5", hold: "600", observation: "3", maxOpen: "5", firstBuyerWindow: "5",
      creatorSoldBeforeFill: "FREE_CANCEL", creatorSellExit: "ENABLED", filters: undefined });
    expect(form.filters.buy_count).toEqual({ on: true, min: "2", max: "" });
    expect(generationText(v)).toBe("Saved generation 0 · applies at the next start");
    expect(saveRequest(v, form)).toEqual({ kind: "none" });
    // Without the state the same two changes have no filter basis; with it, one set_all at CAS 0.
    expect(saveRequest(off0, { ...formOf(off0).form, maxOpen: "6", firstBuyerWindow: "8" })).toHaveProperty("kind", "refused");
    const r = saveRequest(v, { ...form, maxOpen: "6", firstBuyerWindow: "8" });
    if (r.kind !== "all") throw new Error(JSON.stringify(r));
    expect([r.filters, r.values.maxOpen, r.values.firstBuyerWindow, r.values.inputWei]).toEqual(["buy_count:2:,unique_buyers:2:", 6, 8, undefined]);
    expect(r.values.filters).toHaveLength(15);
    expect(r.values.filters.slice(0, 3)).toEqual([{ feature: "buy_count", enabled: true, min: "2" }, { feature: "unique_buyers", enabled: true, min: "2" }, { feature: "sell_count", enabled: false }]);
    // Emptying a file value is refused like a stored one: no native unset.
    expect(saveRequest(v, { ...form, hold: "" })).toEqual({ kind: "refused", reason: expect.stringContaining("BSC_STRATEGY_C_HOLD_MS cannot be empty") });
  });
  it("refuses a value outside the native bounds, places or sign before any request", () => {
    const v = withSettingsState(off0, answer(settingsState()));
    const { form } = formOf(v);
    const refused = (f: Partial<typeof form>) => { const r = saveRequest(v, { ...form, ...f }); return r.kind === "refused" ? r.reason : r.kind; };
    expect(refused({ firstBuyerWindow: "101" })).toBe("BSC_STRATEGY_C_FIRST_BUYER_WINDOW: 101 is above the native maximum 100");
    expect(refused({ firstBuyerWindow: "0" })).toBe("BSC_STRATEGY_C_FIRST_BUYER_WINDOW: 0 is below the native minimum 1");
    expect(refused({ firstBuyerWindow: "100" })).toBe("one");
    expect(refused({ target: "1.00001" })).toBe("BSC_STRATEGY_C_TARGET_NET_MULTIPLE: 1.00001 is no number with at most 4 decimal places");
    expect(refused({ target: "1" })).toBe("BSC_STRATEGY_C_TARGET_NET_MULTIPLE: 1 is below the native minimum 1.0001");
    expect(refused({ stop: "1" })).toBe("BSC_STRATEGY_C_STOP_NET_MULTIPLE: 1 is above the native maximum 0.9999");
    expect(refused({ stake: "0" })).toBe("BSC_STRATEGY_C_INPUT_WEI: 0 BNB is below the native minimum 0.000000000000000001 BNB");
    expect(refused({ hold: "86400.001" })).toBe("BSC_STRATEGY_C_HOLD_MS: 86400.001 s is above the native maximum 86400 s");
    const rule = (key: string, min: string, max = "") => ({ filters: { ...form.filters, [key]: { on: true, min, max } } });
    expect(refused(rule("sell_count", "-1"))).toBe("sell_count: -1 cannot be negative");
    expect(refused(rule("sell_count", "1.5"))).toBe("sell_count: 1.5 is no whole number");
    expect(refused({ maxOpen: "2.5" })).toBe("BSC_STRATEGY_C_MAX_OPEN: 2.5 is no whole number");
    expect(refused(rule("unique_buyer_ratio", "", "0.12345"))).toBe("unique_buyer_ratio: 0.12345 has more than 4 decimal places");
    expect(saveRequest(v, { ...form, ...rule("net_buy_flow_bnb", "-0.5") })).toEqual({ kind: "one", key: "BSC_STRATEGY_C_FILTERS", value: "buy_count:2:,unique_buyers:2:,net_buy_flow_bnb:-0.5:" });
    expect(fieldRanges(v)).toMatchObject({ firstBuyerWindow: "Native range: 1 – 100", target: "Native range: 1.0001 – …", hold: "Native range: 0.001 s – 86400 s" });
    expect(featureHints(v)).toMatchObject({ net_buy_flow_bnb: "Native unit bnb, at most 18 decimal places, may be negative", curve_progress_pct: "Native unit percent, at most 4 decimal places" });
    expect(featureHints(v)).not.toHaveProperty("provider_gap_flag");
    // A native catalogue other than the form's 15 features: a set_all would not send each of them.
    const other = withSettingsState(off0, answer(settingsState({ features: ([...NATIVE_FEATURES.slice(0, 14), ["new_feature", "count", 0, false]] as [string, string, number, boolean][])
      .map(([name, unit, places, signed], index) => ({ name, index, unit, places, signed, filterable: true })) })));
    expect(saveRequest(other, { ...formOf(other).form, maxOpen: "6", firstBuyerWindow: "8" })).toEqual({ kind: "refused",
      reason: "The native filter catalogue differs from the form (+new_feature, -curve_progress_pct): save one change at a time" });
    expect(saveRequest(other, { ...formOf(other).form, maxOpen: "6" })).toHaveProperty("kind", "one");
  });
  it("keeps the stored and running basis for another generation or no answer, and shows the owner's apply status", () => {
    const note = (v: ReturnType<typeof withSettingsState>) => [v.basis, v.basisNote];
    expect(note(withSettingsState(off0, answer(settingsState({ storedRevision: 3n }))))).toEqual([undefined, "the settings state is of generation 3, the snapshot names 0"]);
    expect(note(withSettingsState(off0, { kind: "UNAVAILABLE", reason: "HTTP 503" }))).toEqual([undefined, "HTTP 503"]);
    expect(withSettingsState(off0, { kind: "LOADING" })).toBe(off0);
    expect(withSettingsState(off0, answer(settingsState({ availability: Availability.UNAVAILABLE, unavailableReason: "the Strategy C settings owner: x" }))).basisNote).toBe("the Strategy C settings owner: x");
    const gone = strategySettingsView([], [], 0);
    expect(withSettingsState(gone, answer(settingsState()))).toBe(gone);
    const at = (over: StateInit) => generationText(withSettingsState(off0, answer(settingsState(over))));
    expect(at({ applyStatus: SettingsApplyStatus.IN_EFFECT, runningRevision: 0n })).toBe("Saved generation 0 · in effect");
    expect(at({ applyStatus: SettingsApplyStatus.SAVED_APPLIES_NEXT_START, runningRevision: 0n })).toBe("Saved generation 0 · running it with older configuration file values until the next start");
    expect(at({ applyStatus: SettingsApplyStatus.SAVED_APPLIES_NEXT_START, runningRevision: 2n })).toBe("Saved generation 0 · running generation 2 until the next start");
    expect(at({ provenance: { qualityReasons: ["RUNTIME_STATUS_STALE"] } })).toBe("Saved generation 0 · applies at the next start (runtime status stale)");
    expect(at({ applyStatus: SettingsApplyStatus.REFUSED, refusalReason: "SETTINGS_REFUSED: x" })).toBe("Saved generation 0 · refused at the start: SETTINGS_REFUSED: x");
    expect(at({ applyStatus: SettingsApplyStatus.UNSPECIFIED, provenance: { qualityReasons: ["RUNTIME_STATUS_INVALID: y"] } })).toBe("Saved generation 0 · running state unknown (RUNTIME_STATUS_INVALID: y)");
    // A stored generation: the state of that generation is the basis (stored over the file), its revision the CAS.
    const six = strategySettingsView([rec(SOURCE_SCHEMA, { generation: "6", "sources[0].source_id": "BSC_INGRESS_GETH" }),
      rec(SETTINGS_SCHEMA, { generation: "6", "settings[0].id": "BSC_STRATEGY_C_MAX_OPEN", "settings[0].value": "4" })], [], 0);
    const effective6 = withSettingsState(six, answer(settingsState({ storedRevision: 6n, effective: { ...FILE_VALUES, maxOpen: 4 } })));
    expect([formOf(effective6).form.maxOpen, formOf(effective6).form.stake, effective6.basis?.revision]).toEqual(["4", "0.01", 6n]);
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
