// V8 Strategy C editor (FE-V8-05/06/09, Bindings §14.2): the native settings as the V8 form shows
// them, and back. Exact text conversions only (wei ↔ BNB, ms ↔ s, the filter string): no floating
// point, no rounding; a value the conversion cannot represent exactly is refused here, every
// other check is the native parser's (`bsc_runtime strategy set`, its refusal is the receipt).
import type { StrategySettingsView } from "./strategySettings";

/** The 15 filterable features (trading/src/strategy/features.rs NAMES[0..15]) with the V8 groups,
 *  labels and units of Bindings §14.2. provider_gap_flag (NAMES[15]) is a data-quality flag only. */
export interface FilterSpec { key: string; label: string; unit: string; places: "int" | "bnb" | "ratio" }
export const FILTER_GROUPS: ReadonlyArray<{ name: string; items: readonly FilterSpec[] }> = [
  { name: "Trade activity", items: [
    { key: "buy_count", label: "Buy count", unit: "", places: "int" },
    { key: "sell_count", label: "Sell count", unit: "", places: "int" },
    { key: "buy_volume_bnb", label: "Buy volume", unit: "BNB", places: "bnb" },
    { key: "sell_volume_bnb", label: "Sell volume", unit: "BNB", places: "bnb" },
    { key: "net_buy_flow_bnb", label: "Net buy volume", unit: "BNB", places: "bnb" },
  ] },
  { name: "Buyers", items: [
    { key: "unique_buyers", label: "Unique buyers", unit: "", places: "int" },
    { key: "unique_buyer_ratio", label: "Unique buyer ratio", unit: "0–1", places: "ratio" },
    { key: "largest_buy_bnb", label: "Largest buy", unit: "BNB", places: "bnb" },
    { key: "buyer_volume_concentration", label: "Buyer concentration", unit: "0–1", places: "ratio" },
    { key: "first5_buyer_concentration", label: "First five buyers’ share", unit: "0–1", places: "ratio" },
  ] },
  { name: "Price and time", items: [
    { key: "first_sell_latency_ms", label: "Time to first sell", unit: "ms", places: "int" },
    { key: "price_change", label: "Price change", unit: "Share", places: "ratio" },
    { key: "max_drawdown", label: "Maximum drawdown", unit: "Share", places: "ratio" },
  ] },
  { name: "Token launch", items: [
    { key: "dev_initial_buy", label: "Creator initial buy", unit: "Share", places: "ratio" },
    { key: "curve_progress_pct", label: "Curve progress", unit: "%", places: "ratio" },
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

/** The filters as the native string: enabled rules in native order, disabled ones left out. */
export function serializeFilters(filters: Filters): { text: string } | { error: string } {
  const out: string[] = [];
  for (const key of FILTER_KEYS) {
    const f = filters[key];
    if (!f?.on) continue;
    // Both bounds empty is a valid rule (`feature::`): the feature must not be NULL.
    const min = f.min.trim().replace(",", "."), max = f.max.trim().replace(",", ".");
    for (const b of [min, max]) if (b && !DECIMAL.test(b)) return { error: `${key}: ${b} is no exact decimal` };
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

/** The V8 form of the nine settable keys (the stored value, else the running one, else empty). */
export interface StrategyForm {
  stake: string; target: string; stop: string; hold: string; observation: string; maxOpen: string;
  creatorSoldBeforeFill: string; creatorSellExit: string; filters: Filters;
}
export const KEY_OF: Record<Exclude<keyof StrategyForm, "filters">, string> & { filters: string } = {
  stake: "BSC_STRATEGY_C_INPUT_WEI", target: "BSC_STRATEGY_C_TARGET_NET_MULTIPLE", stop: "BSC_STRATEGY_C_STOP_NET_MULTIPLE",
  hold: "BSC_STRATEGY_C_HOLD_MS", observation: "BSC_STRATEGY_C_OBSERVATION_MS", maxOpen: "BSC_STRATEGY_C_MAX_OPEN",
  creatorSoldBeforeFill: "BSC_STRATEGY_C_CREATOR_SOLD_BEFORE_FILL", creatorSellExit: "BSC_STRATEGY_C_CREATOR_SELL_EXIT", filters: "BSC_STRATEGY_C_FILTERS",
};

/** The native value of each key the form shows: stored, else what the running strategy took. */
export function nativeValues(view: StrategySettingsView): Record<string, string | undefined> {
  return Object.fromEntries(view.rows.map(r => [r.id, r.stored ?? r.effective]));
}

export function formOf(view: StrategySettingsView): { form: StrategyForm; filterError?: string } {
  const v = nativeValues(view);
  const parsed = parseFilters(v[KEY_OF.filters] ?? "");
  return {
    form: {
      stake: weiToBnb(v[KEY_OF.stake] ?? ""), target: v[KEY_OF.target] ?? "", stop: v[KEY_OF.stop] ?? "",
      hold: msToSeconds(v[KEY_OF.hold] ?? ""), observation: msToSeconds(v[KEY_OF.observation] ?? ""), maxOpen: v[KEY_OF.maxOpen] ?? "",
      creatorSoldBeforeFill: v[KEY_OF.creatorSoldBeforeFill] ?? "", creatorSellExit: v[KEY_OF.creatorSellExit] ?? "",
      filters: "filters" in parsed ? parsed.filters : Object.fromEntries(FILTER_KEYS.map(k => [k, { on: false, min: "", max: "" }])),
    },
    filterError: "error" in parsed ? parsed.error : undefined,
  };
}

/** The native requests the form differs in from the native values, each `KEY=value`. An invalid
 *  filter edit refuses the filters only (`filterError`), never the other keys. */
export function changes(view: StrategySettingsView, form: StrategyForm): { changed: { key: string; value: string }[]; filterError?: string } | { error: string } {
  const v = nativeValues(view);
  const out: { key: string; value: string }[] = [];
  const add = (key: string, value: string) => { if (value !== (v[key] ?? "")) out.push({ key, value }); };
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
  add(KEY_OF.creatorSoldBeforeFill, form.creatorSoldBeforeFill);
  add(KEY_OF.creatorSellExit, form.creatorSellExit);
  // Untouched filters keep the owner's own text (its spacing, its spelling, even one the form cannot show).
  const shown = formOf(view).form.filters;
  if (FILTER_KEYS.every(k => JSON.stringify(form.filters[k]) === JSON.stringify(shown[k]))) return { changed: out };
  const filters = serializeFilters(form.filters);
  if ("error" in filters) return { changed: out, filterError: filters.error };
  add(KEY_OF.filters, filters.text);
  return { changed: out };
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
