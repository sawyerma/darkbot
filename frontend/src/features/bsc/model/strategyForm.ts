// V8 Strategy C editor (FE-V8-05/06/09, Bindings §14.2): the native settings as the V8 form shows
// them, and back. Exact text conversions only (wei ↔ BNB, ms ↔ s, the filter string): no floating
// point, no rounding; a value the conversion cannot represent exactly is refused here, and so is
// one outside the bounds, places or sign the owner's settings state reports (StrategySettingSpec,
// StrategyFeatureSpec); the native parser checks again (`bsc_runtime strategy set`, its refusal
// is the receipt).
import type { FeatureSpec, SettingSpec, StrategySettingsView } from "./strategySettings";

/** The 15 filterable features (trading/src/strategy/features.rs NAMES[0..15]) with the V8 groups,
 *  labels and display units of Bindings §14.2; their native unit, places and sign come with the
 *  owner's settings state. provider_gap_flag (NAMES[15]) is a data-quality flag only. */
export interface FilterSpec { key: string; label: string; unit: string }
export const FILTER_GROUPS: ReadonlyArray<{ name: string; items: readonly FilterSpec[] }> = [
  { name: "Trade activity", items: [
    { key: "buy_count", label: "Buy count", unit: "" },
    { key: "sell_count", label: "Sell count", unit: "" },
    { key: "buy_volume_bnb", label: "Buy volume", unit: "BNB" },
    { key: "sell_volume_bnb", label: "Sell volume", unit: "BNB" },
    { key: "net_buy_flow_bnb", label: "Net buy volume", unit: "BNB" },
  ] },
  { name: "Buyers", items: [
    { key: "unique_buyers", label: "Unique buyers", unit: "" },
    { key: "unique_buyer_ratio", label: "Unique buyer ratio", unit: "0–1" },
    { key: "largest_buy_bnb", label: "Largest buy", unit: "BNB" },
    { key: "buyer_volume_concentration", label: "Buyer concentration", unit: "0–1" },
    { key: "first5_buyer_concentration", label: "First five buyers’ share", unit: "0–1" },
  ] },
  { name: "Price and time", items: [
    { key: "first_sell_latency_ms", label: "Time to first sell", unit: "ms" },
    { key: "price_change", label: "Price change", unit: "Share" },
    { key: "max_drawdown", label: "Maximum drawdown", unit: "Share" },
  ] },
  { name: "Token launch", items: [
    { key: "dev_initial_buy", label: "Creator initial buy", unit: "Share" },
    { key: "curve_progress_pct", label: "Curve progress", unit: "%" },
  ] },
];
export const FILTER_KEYS: readonly string[] = FILTER_GROUPS.flatMap(g => g.items.map(i => i.key));

export interface FilterValue { on: boolean; min: string; max: string }
export type Filters = Record<string, FilterValue>;

/** A bound as the native parser (trading/src/strategy/fixed.rs `parse`) takes it: digits, an optional
 *  fraction, an optional minus; the places are its check. */
const DECIMAL = /^-?[0-9]+(?:\.[0-9]+)?$/;

/** BSC_STRATEGY_C_FILTERS → one entry per filterable feature (disabled when not named). */
export function parseFilters(text: string): { filters: Filters } | { error: string } {
  const filters: Filters = Object.fromEntries(FILTER_KEYS.map(k => [k, { on: false, min: "", max: "" }]));
  for (const item of text.split(",").map(s => s.trim()).filter(Boolean)) {
    const parts = item.split(":");
    if (parts.length !== 3) return { error: `filter ${item} is not feature:min:max` };
    const [name = "", min = "", max = ""] = parts;
    if (!FILTER_KEYS.includes(name)) return { error: `filter ${item}: ${name} is not a filterable feature` };
    if (filters[name]?.on) return { error: `filter ${item}: ${name} twice` };
    filters[name] = { on: true, min, max };
  }
  return { filters };
}

/** A bound against its feature's native places and sign (none known: the syntax alone). */
function boundError(key: string, b: string, spec?: FeatureSpec): string | undefined {
  if (!DECIMAL.test(b)) return `${key}: ${b} is no exact decimal`;
  if (!spec) return undefined;
  if (!spec.filterable) return `${key} is no filterable feature of the native owner`;
  if (b.startsWith("-") && !spec.signed) return `${key}: ${b} cannot be negative`;
  if ((b.split(".")[1]?.length ?? 0) > spec.places) return `${key}: ${b} ${spec.places ? `has more than ${spec.places} decimal places` : "is no whole number"}`;
  return undefined;
}

/** The features in the order of a stored filter string (its rules first, as the owner keeps them;
 *  never re-sorted by the client), then every other feature in native order. */
export function filterOrder(text: string): readonly string[] {
  const named = [...new Set(text.split(",").map(s => s.trim().split(":")[0] ?? "").filter(k => FILTER_KEYS.includes(k)))];
  return [...named, ...FILTER_KEYS.filter(k => !named.includes(k))];
}

/** The filters as the native string: enabled rules in `order` (native order by default), disabled
 *  ones left out; each bound checked against `features` (the owner's catalogue) where it is known. */
export function serializeFilters(filters: Filters, order: readonly string[] = FILTER_KEYS, features?: Readonly<Record<string, FeatureSpec>>): { text: string } | { error: string } {
  const out: string[] = [];
  for (const key of order) {
    const f = filters[key];
    if (!f?.on) continue;
    // Both bounds empty is a valid rule (`feature::`): the feature must not be NULL.
    const min = f.min.trim().replace(",", "."), max = f.max.trim().replace(",", ".");
    for (const b of [min, max]) {
      const error = b ? boundError(key, b, features?.[key]) : undefined;
      if (error) return { error };
    }
    out.push(`${key}:${min}:${max}`);
  }
  return { text: out.join(",") };
}

/** Exact wei → BNB text (trailing zeros cut, at most 18 places). */
export function weiToBnb(wei: string): string {
  if (!/^[0-9]+$/.test(wei)) return "";
  const p = wei.padStart(19, "0");
  const whole = p.slice(0, -18).replace(/^0+(?=\d)/, "");
  const frac = p.slice(-18).replace(/0+$/, "");
  return frac ? `${whole}.${frac}` : whole;
}
/** BNB text → exact wei; more than 18 places or a sign is refused. */
export function bnbToWei(text: string): { wei: string } | { error: string } {
  const t = text.trim().replace(",", ".");
  const m = /^([0-9]+)(?:\.([0-9]{1,18}))?$/.exec(t);
  if (!m) return { error: `${text} is no BNB amount with at most 18 places` };
  const wei = (m[1] + (m[2] ?? "").padEnd(18, "0")).replace(/^0+(?=\d)/, "");
  return { wei };
}
/** ms → seconds text (exact, at most 3 places). */
export function msToSeconds(ms: string): string {
  if (!/^[0-9]+$/.test(ms)) return "";
  const p = ms.padStart(4, "0");
  const whole = p.slice(0, -3).replace(/^0+(?=\d)/, "");
  const frac = p.slice(-3).replace(/0+$/, "");
  return frac ? `${whole}.${frac}` : whole;
}
/** Seconds text → exact ms; more than 3 places is refused. */
export function secondsToMs(text: string): { ms: string } | { error: string } {
  const t = text.trim().replace(",", ".");
  const m = /^([0-9]+)(?:\.([0-9]{1,3}))?$/.exec(t);
  if (!m) return { error: `${text} is no time in seconds with at most 3 places` };
  return { ms: (m[1] + (m[2] ?? "").padEnd(3, "0")).replace(/^0+(?=\d)/, "") };
}

/** The V8 form of the ten settable keys (the stored value, else the running one, else empty). */
export interface StrategyForm {
  stake: string; target: string; stop: string; hold: string; observation: string; maxOpen: string; firstBuyerWindow: string;
  creatorSoldBeforeFill: string; creatorSellExit: string; filters: Filters;
}
export const KEY_OF: Record<Exclude<keyof StrategyForm, "filters">, string> & { filters: string } = {
  stake: "BSC_STRATEGY_C_INPUT_WEI", target: "BSC_STRATEGY_C_TARGET_NET_MULTIPLE", stop: "BSC_STRATEGY_C_STOP_NET_MULTIPLE",
  hold: "BSC_STRATEGY_C_HOLD_MS", observation: "BSC_STRATEGY_C_OBSERVATION_MS", maxOpen: "BSC_STRATEGY_C_MAX_OPEN",
  firstBuyerWindow: "BSC_STRATEGY_C_FIRST_BUYER_WINDOW",
  creatorSoldBeforeFill: "BSC_STRATEGY_C_CREATOR_SOLD_BEFORE_FILL", creatorSellExit: "BSC_STRATEGY_C_CREATOR_SELL_EXIT", filters: "BSC_STRATEGY_C_FILTERS",
};

/** The native value of each key the form shows: the owner's settings state (stored over the
 *  configuration file) where it answered for this generation, else stored, else what the running
 *  strategy took. */
export function nativeValues(view: StrategySettingsView): Record<string, string | undefined> {
  if (view.basis) return view.basis.values;
  return Object.fromEntries(view.rows.map(r => [r.id, r.stored ?? r.effective]));
}

/** An exact decimal text as an integer at `places`; undefined: no such decimal, or more places. */
function scaled(text: string, places: number): bigint | undefined {
  const m = /^(-?)([0-9]+)(?:\.([0-9]+))?$/.exec(text);
  if (!m || (m[3]?.length ?? 0) > places) return undefined;
  const v = BigInt((m[2] ?? "") + (m[3] ?? "").padEnd(places, "0"));
  return m[1] ? -v : v;
}
const SHOW: Record<string, (v: string) => string> = { wei: v => `${weiToBnb(v)} BNB`, ms: v => `${msToSeconds(v)} s` };
const shown = (spec: SettingSpec, v: string) => (SHOW[spec.unit] ?? (x => x))(v);

/** A value against its key's native bounds and places (StrategySettingSpec); none known: undefined. */
function specError(key: string, value: string, spec?: SettingSpec): string | undefined {
  if (!spec || spec.unit === "enum" || spec.unit === "rules") return undefined;
  const v = scaled(value, spec.places);
  if (v === undefined) return `${key}: ${value} is no ${spec.places ? `number with at most ${spec.places} decimal places` : "whole number"}`;
  const lo = spec.min === undefined ? undefined : scaled(spec.min, spec.places);
  const hi = spec.max === undefined ? undefined : scaled(spec.max, spec.places);
  if (lo !== undefined && v < lo) return `${key}: ${shown(spec, value)} is below the native minimum ${shown(spec, spec.min!)}`;
  if (hi !== undefined && v > hi) return `${key}: ${shown(spec, value)} is above the native maximum ${shown(spec, spec.max!)}`;
  return undefined;
}

/** Per form field, the native range of its key in the form's unit (a hint; the owner's state). */
export function fieldRanges(view: StrategySettingsView): Partial<Record<keyof StrategyForm, string>> {
  const specs = view.basis?.specs ?? {};
  const out: Partial<Record<keyof StrategyForm, string>> = {};
  for (const [field, key] of Object.entries(KEY_OF) as [keyof StrategyForm, string][]) {
    const spec = specs[key];
    if (!spec || (spec.min === undefined && spec.max === undefined)) continue;
    out[field] = `Native range: ${spec.min === undefined ? "…" : shown(spec, spec.min)} – ${spec.max === undefined ? "…" : shown(spec, spec.max)}`;
  }
  return out;
}

/** Per filter, its native unit, places and sign (a hint; the owner's catalogue). */
export function featureHints(view: StrategySettingsView): Record<string, string> {
  return Object.fromEntries(Object.entries(view.basis?.features ?? {}).filter(([, f]) => f.filterable)
    .map(([name, f]) => [name, `Native unit ${f.unit}, at most ${f.places} decimal places${f.signed ? ", may be negative" : ""}`]));
}

export function formOf(view: StrategySettingsView): { form: StrategyForm; filterError?: string } {
  const v = nativeValues(view);
  const parsed = parseFilters(v[KEY_OF.filters] ?? "");
  return {
    form: {
      stake: weiToBnb(v[KEY_OF.stake] ?? ""), target: v[KEY_OF.target] ?? "", stop: v[KEY_OF.stop] ?? "",
      hold: msToSeconds(v[KEY_OF.hold] ?? ""), observation: msToSeconds(v[KEY_OF.observation] ?? ""), maxOpen: v[KEY_OF.maxOpen] ?? "",
      firstBuyerWindow: v[KEY_OF.firstBuyerWindow] ?? "",
      creatorSoldBeforeFill: v[KEY_OF.creatorSoldBeforeFill] ?? "", creatorSellExit: v[KEY_OF.creatorSellExit] ?? "",
      filters: "filters" in parsed ? parsed.filters : Object.fromEntries(FILTER_KEYS.map(k => [k, { on: false, min: "", max: "" }])),
    },
    filterError: "error" in parsed ? parsed.error : undefined,
  };
}

/** The native requests the form differs in from the native values, each `KEY=value`. An invalid
 *  filter edit refuses the filters only (`filterError`), never the other keys. A setting has no
 *  native "unset": an empty value is refused for every key but the filters (empty = no rules) and
 *  never returns to the configuration file's value, so emptying a shown value is refused here. */
export function changes(view: StrategySettingsView, form: StrategyForm): { changed: { key: string; value: string }[]; filterError?: string } | { error: string } {
  const v = nativeValues(view);
  const out: { key: string; value: string }[] = [];
  let emptied: string | undefined;
  const add = (key: string, value: string) => {
    if (value === (v[key] ?? "")) return;
    if (value === "") emptied ??= key;
    else out.push({ key, value });
  };
  const stake = form.stake.trim() ? bnbToWei(form.stake) : { wei: "" };
  if ("error" in stake) return stake;
  add(KEY_OF.stake, stake.wei);
  add(KEY_OF.target, form.target.trim().replace(",", "."));
  add(KEY_OF.stop, form.stop.trim().replace(",", "."));
  for (const [field, key] of [["hold", KEY_OF.hold], ["observation", KEY_OF.observation]] as const) {
    const t = form[field].trim() ? secondsToMs(form[field]) : { ms: "" };
    if ("error" in t) return t;
    add(key, t.ms);
  }
  add(KEY_OF.maxOpen, form.maxOpen.trim());
  add(KEY_OF.firstBuyerWindow, form.firstBuyerWindow.trim());
  add(KEY_OF.creatorSoldBeforeFill, form.creatorSoldBeforeFill);
  add(KEY_OF.creatorSellExit, form.creatorSellExit);
  if (emptied) return { error: `${emptied} cannot be empty: clearing never restores the configuration file's value, enter the value itself` };
  for (const { key, value } of out) {
    const error = specError(key, value, view.basis?.specs[key]);
    if (error) return { error };
  }
  // Untouched filters keep the owner's own text (its spacing, its spelling, even one the form cannot show).
  if (sameFilters(form.filters, formOf(view).form.filters)) return { changed: out };
  const filters = serializeFilters(form.filters, filterOrder(v[KEY_OF.filters] ?? ""), view.basis?.features);
  if ("error" in filters) return { changed: out, filterError: filters.error };
  // Every rule disabled is the empty string: no rules (the one key an empty value is valid for).
  if (filters.text !== (v[KEY_OF.filters] ?? "")) out.push({ key: KEY_OF.filters, value: filters.text });
  return { changed: out };
}

const sameFilters = (a: Filters, b: Filters) => FILTER_KEYS.every(k => {
  const x = a[k], y = b[k];
  return x?.on === y?.on && x?.min === y?.min && x?.max === y?.max;
});

/** The wire types' ranges (uint32, uint64): a representation limit of the request, never a
 *  setting's bound (those are the native parser's, StrategySettingSpec). */
const U32_MAX = (1n << 32n) - 1n, U64_MAX = (1n << 64n) - 1n;
const whole = (key: string, text: string, max: bigint): bigint | { error: string } =>
  /^[0-9]+$/.test(text) && BigInt(text) <= max ? BigInt(text) : { error: `${key}: ${text} is no whole number the request can carry` };

/** The settable values of one `bsc.strategy.set_all` (StrategyCSettableValues): an absent key
 *  stays as stored; the filters are always the complete list (every shown feature, enabled or not;
 *  the owner refuses an empty list and drops the disabled rules). */
export interface StrategySettableValues {
  inputWei?: string; targetNetMultiple?: string; stopNetMultiple?: string;
  observationMs?: bigint; holdMs?: bigint; maxOpen?: number; firstBuyerWindow?: number;
  creatorSoldBeforeFill?: "FREE_CANCEL" | "TX_LANDS"; creatorSellExit?: "ENABLED" | "DISABLED";
  filters: { feature: string; enabled: boolean; min?: string; max?: string }[];
}

/** What Save sends for the form (BSC-V8-04): nothing; ONE changed key as `bsc.strategy.set`; or
 *  several as ONE `bsc.strategy.set_all` (one generation, all or none). A set_all carries the
 *  changed keys (an absent key stays as stored) and always the complete filter list, every shown
 *  feature with its enabled flag; so it needs the filters in effect: the owner's settings state
 *  (stored over the configuration file), else stored, else the running strategy's. */
export type SaveRequest =
  | { kind: "none" }
  | { kind: "one"; key: string; value: string }
  | { kind: "all"; changed: { key: string; value: string }[]; filters: string; values: StrategySettableValues }
  | { kind: "refused"; reason: string };
export function saveRequest(view: StrategySettingsView, form: StrategyForm): SaveRequest {
  const diff = changes(view, form);
  if ("error" in diff) return { kind: "refused", reason: diff.error };
  if (diff.filterError) return { kind: "refused", reason: diff.filterError };
  const [first, ...more] = diff.changed;
  if (!first) return { kind: "none" };
  if (!more.length) return { kind: "one", ...first };
  const v = nativeValues(view);
  const stored = v[KEY_OF.filters];
  if (stored === undefined) return { kind: "refused", reason: "Several changes are saved together with the complete filter list, and the filters in effect are not reported (settings state not loaded, none stored, Strategy C not running): save one change at a time" };
  if (formOf(view).filterError) return { kind: "refused", reason: "The stored filters cannot be shown here, and a combined save would replace them: save one change at a time" };
  const differs = catalogueDiffers(view);
  if (differs) return { kind: "refused", reason: `${differs}: save one change at a time` };
  const order = filterOrder(stored);
  const text = serializeFilters(form.filters, order, view.basis?.features);
  if ("error" in text) return { kind: "refused", reason: text.error };
  const values: StrategySettableValues = {
    filters: order.map(feature => {
      const f = form.filters[feature] ?? { on: false, min: "", max: "" };
      const min = f.min.trim().replace(",", "."), max = f.max.trim().replace(",", ".");
      return { feature, enabled: f.on, ...(f.on && min ? { min } : {}), ...(f.on && max ? { max } : {}) };
    }),
  };
  for (const { key, value } of diff.changed) {
    switch (key) {
      case KEY_OF.stake: values.inputWei = value; break;
      case KEY_OF.target: values.targetNetMultiple = value; break;
      case KEY_OF.stop: values.stopNetMultiple = value; break;
      case KEY_OF.observation: case KEY_OF.hold: case KEY_OF.maxOpen: case KEY_OF.firstBuyerWindow: {
        const n = whole(key, value, key === KEY_OF.observation || key === KEY_OF.hold ? U64_MAX : U32_MAX);
        if (typeof n !== "bigint") return { kind: "refused", reason: n.error };
        if (key === KEY_OF.observation) values.observationMs = n;
        else if (key === KEY_OF.hold) values.holdMs = n;
        else if (key === KEY_OF.maxOpen) values.maxOpen = Number(n);
        else values.firstBuyerWindow = Number(n);
        break;
      }
      case KEY_OF.creatorSoldBeforeFill:
        if (value !== "FREE_CANCEL" && value !== "TX_LANDS") return { kind: "refused", reason: `${key}: ${value} is no choice` };
        values.creatorSoldBeforeFill = value; break;
      case KEY_OF.creatorSellExit:
        if (value !== "ENABLED" && value !== "DISABLED") return { kind: "refused", reason: `${key}: ${value} is no choice` };
        values.creatorSellExit = value; break;
      case KEY_OF.filters: break;
    }
  }
  return { kind: "all", changed: diff.changed, filters: text.text, values };
}

/** Whether the owner's filterable features are not the form's 15 (a set_all sends every shown one). */
function catalogueDiffers(view: StrategySettingsView): string | undefined {
  const features = view.basis?.features;
  if (!features || !Object.keys(features).length) return undefined;
  const native = Object.entries(features).filter(([, f]) => f.filterable).map(([name]) => name);
  const unknown = native.filter(k => !FILTER_KEYS.includes(k)), missing = FILTER_KEYS.filter(k => !native.includes(k));
  return unknown.length || missing.length
    ? `The native filter catalogue differs from the form (${[...unknown.map(k => `+${k}`), ...missing.map(k => `-${k}`)].join(", ")})` : undefined;
}

export const activeFilters = (filters: Filters) => FILTER_KEYS.filter(k => filters[k]?.on).length;

/** Per form field, the running strategy's own value where the stored one differs (FE-V8-06: saved
 *  vs running; the stored value is effective at the next start of Strategy C). */
export function runningNotes(view: StrategySettingsView): Partial<Record<keyof StrategyForm, string>> {
  if (!view.running) return {};
  const out: Partial<Record<keyof StrategyForm, string>> = {};
  const show: Record<string, (v: string) => string> = { [KEY_OF.stake]: v => `${weiToBnb(v)} BNB`, [KEY_OF.hold]: v => `${msToSeconds(v)} s`, [KEY_OF.observation]: v => `${msToSeconds(v)} s` };
  for (const [field, key] of Object.entries(KEY_OF) as [keyof StrategyForm, string][]) {
    const row = view.rows.find(r => r.id === key);
    if (!row?.pending) continue;
    const v = row.effective;
    out[field] = `Saved · running ${v === undefined ? "the configuration file's" : v === "" ? "(empty)" : (show[key] ?? (x => x))(v)} until the next start`;
  }
  return out;
}
