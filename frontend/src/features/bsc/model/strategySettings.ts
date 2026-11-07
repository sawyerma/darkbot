// Frontend package (06.11.): the operator's Strategy C settings. The native owner
// (`bsc_runtime strategy set`/`set_all`) stores each request at once with its generation as CAS and
// checks the values with the runtime configuration's own parser; the running strategy took the
// settings of its start (runtime status `strategy[].settings*`). A new value is effective at the
// next start of Strategy C (its switch on, a runtime restart). The owner's settings state over
// bsc/query (BSC-V8-04) adds the configuration file's values, the native bounds and whether the
// stored generation is in effect; nothing here decides a value.
import type { BscQueryOutcome } from "@/core/realtime/bscQuery";
import { Availability, CreatorSellExit, CreatorSoldBeforeFill, SettingsApplyStatus, type StrategyCSettings } from "../api/bscV8Adapter";
import type { NativeRecordView } from "./types";
import { RUNTIME_SCHEMA, SOURCE_SCHEMA, runtimeView } from "./sources";
import { BACKEND_UNAVAILABLE } from "./status";

export const SETTINGS_SCHEMA = "darkbot.bsc.strategy-settings.v1";

/** The form of the LIVE tab: the keys the owner takes (the user's scope of 06.11.), each with
 *  the values the runtime knows where it is a choice; every other key stays the file's. */
export interface StrategySettingInput { id: string; label: string; choices?: readonly string[] }
export const STRATEGY_SETTINGS: readonly StrategySettingInput[] = [
  { id: "BSC_STRATEGY_C_OBSERVATION_MS", label: "Observation time (ms)" },
  { id: "BSC_STRATEGY_C_FILTERS", label: "Entry filters" },
  { id: "BSC_STRATEGY_C_INPUT_WEI", label: "BUY amount (wei)" },
  { id: "BSC_STRATEGY_C_TARGET_NET_MULTIPLE", label: "Target multiple (net)" },
  { id: "BSC_STRATEGY_C_STOP_NET_MULTIPLE", label: "Stop multiple (net)" },
  { id: "BSC_STRATEGY_C_HOLD_MS", label: "Hold / timeout (ms)" },
  { id: "BSC_STRATEGY_C_MAX_OPEN", label: "Max open positions" },
  { id: "BSC_STRATEGY_C_CREATOR_SOLD_BEFORE_FILL", label: "Creator sold before the fill", choices: ["FREE_CANCEL", "TX_LANDS"] },
  { id: "BSC_STRATEGY_C_CREATOR_SELL_EXIT", label: "Creator-sell exit", choices: ["ENABLED", "DISABLED"] },
  { id: "BSC_STRATEGY_C_FIRST_BUYER_WINDOW", label: "Early buyers in window" },
];

export interface StrategySettingRow extends StrategySettingInput {
  /** The operator's stored value; absent: the configuration file's. */
  stored?: string;
  /** The value the running strategy took at its start; absent while it does not run. */
  effective?: string;
  /** Stored, and not what the running strategy took: effective at the next start. */
  pending: boolean;
}
export interface StrategySettingsView {
  available: boolean; reason: string;
  /** The stored generation: the CAS of the next request (0: none stored). */
  generation: bigint; requestId: string;
  /** The running strategy (a fresh runtime status names its start). */
  running: boolean; startedGeneration?: bigint;
  /** Strategy C's state and why it does not run (SETTINGS_REFUSED, GAS_EVIDENCE_REFUSED). */
  state: string; stateReason: string;
  rows: StrategySettingRow[];
  /** The owner's settings state of this generation (bsc/query); absent: not answered (`basisNote`). */
  basis?: SettingsBasis;
  /** Why there is no basis: the query's answer, or another generation than this one. */
  basisNote?: string;
}

/** A settable key's bounds as the native parser enforces them (StrategySettingSpec). */
export interface SettingSpec { unit: string; min?: string; max?: string; places: number }
/** A filterable feature as the native owner defines it (StrategyFeatureSpec). */
export interface FeatureSpec { index: number; unit: string; places: number; signed: boolean; filterable: boolean }
/** The native settings state (StrategyCSettingsState) of the stored generation. */
export interface SettingsBasis {
  /** Each key's value in effect for the next start (stored over the configuration file), as the
   *  native canonical string `bsc.strategy.set` takes (the filters: the enabled rules in their order). */
  values: Record<string, string | undefined>;
  /** stored_revision: the compare-and-swap of the next save. */
  revision: bigint;
  /** SettingsApplyStatus without its prefix: IN_EFFECT, SAVED_APPLIES_NEXT_START, NOT_RUNNING, REFUSED, UNSPECIFIED. */
  apply: string; refusal: string; runningRevision?: bigint;
  /** The owner's quality reasons (RUNTIME_STATUS_STALE, RUNTIME_STATUS_INVALID: …). */
  reasons: readonly string[];
  specs: Readonly<Record<string, SettingSpec>>;
  features: Readonly<Record<string, FeatureSpec>>;
}

const decimal = (v: string | undefined) => v !== undefined && /^[0-9]+$/.test(v) ? BigInt(v) : undefined;

/** `warnings`: the reporter's (bsc.status); `STRATEGY_SETTINGS_INVALID` names a store it could not
 *  read (review LOW 3: never shown as an empty one). */
export function strategySettingsView(records: readonly NativeRecordView[], warnings: readonly string[] = [], nowMs = Date.now()): StrategySettingsView {
  const stored = records.find(r => r.schema === SETTINGS_SCHEMA);
  const s = new Map(stored?.fields.map(x => [x.name, x.value]) ?? []);
  const set = new Map<string, string>();
  for (let i = 0; s.has(`settings[${i}].id`); i++) set.set(s.get(`settings[${i}].id`) ?? "", s.get(`settings[${i}].value`) ?? "");
  const generation = stored ? decimal(s.get("generation")) : 0n;
  const fresh = runtimeView(records, nowMs).view.state === "RUNNING";
  const runtime = fresh ? records.find(r => r.schema === RUNTIME_SCHEMA) : undefined;
  const f = new Map(runtime?.fields.map(x => [x.name, x.value]) ?? []);
  const took = new Map<string, string>();
  for (let i = 0; f.has(`strategy[0].settings[${i}].id`); i++) took.set(f.get(`strategy[0].settings[${i}].id`) ?? "", f.get(`strategy[0].settings[${i}].value`) ?? "");
  const startedGeneration = decimal(f.get("strategy[0].settings_generation"));
  const running = startedGeneration !== undefined;
  const rows = STRATEGY_SETTINGS.map(k => {
    const value = set.get(k.id);
    const effective = running ? took.get(k.id) : undefined;
    return { ...k, stored: value, effective, pending: running && value !== undefined && value !== effective };
  });
  const owner = records.some(r => r.schema === SOURCE_SCHEMA);
  const unreadable = warnings.includes("STRATEGY_SETTINGS_INVALID");
  return {
    available: owner && generation !== undefined && !unreadable,
    reason: !owner ? BACKEND_UNAVAILABLE : unreadable ? "STRATEGY_SETTINGS_INVALID" : generation === undefined ? "BACKEND_INVALID" : "",
    generation: generation ?? 0n, requestId: s.get("request_id") ?? "",
    running, startedGeneration,
    state: fresh ? f.get("strategy[0].state") ?? "OFF" : "UNKNOWN", stateReason: f.get("strategy[0].reason") ?? "",
    rows,
  };
}

/** A typed settings basis as the native canonical strings (enum UNSPECIFIED, an absent value: none). */
function nativeText(s: StrategyCSettings): Record<string, string | undefined> {
  const n = (x: bigint | number | undefined) => x?.toString();
  const choice = (name: string | undefined) => name === undefined || name === "UNSPECIFIED" ? undefined : name;
  return {
    BSC_STRATEGY_C_OBSERVATION_MS: n(s.observationMs),
    BSC_STRATEGY_C_FILTERS: s.filters.filter(r => r.enabled).map(r => `${r.feature}:${r.min ?? ""}:${r.max ?? ""}`).join(","),
    BSC_STRATEGY_C_INPUT_WEI: s.inputWei,
    BSC_STRATEGY_C_TARGET_NET_MULTIPLE: s.targetNetMultiple,
    BSC_STRATEGY_C_STOP_NET_MULTIPLE: s.stopNetMultiple,
    BSC_STRATEGY_C_HOLD_MS: n(s.holdMs),
    BSC_STRATEGY_C_MAX_OPEN: n(s.maxOpen),
    BSC_STRATEGY_C_CREATOR_SOLD_BEFORE_FILL: choice(CreatorSoldBeforeFill[s.creatorSoldBeforeFill]),
    BSC_STRATEGY_C_CREATOR_SELL_EXIT: choice(CreatorSellExit[s.creatorSellExit]),
    BSC_STRATEGY_C_FIRST_BUYER_WINDOW: n(s.firstBuyerWindow),
  };
}

/** The view with the owner's settings state (`POST /api/v1/bsc/query`, StrategyCSettingsState) as
 *  its form basis: the values in effect for the next start, every key over the configuration file.
 *  Only for the generation the snapshot names (the CAS of a save must be the basis's own); any
 *  other answer leaves the stored and running values alone with its reason. */
export function withSettingsState(view: StrategySettingsView, q: BscQueryOutcome | { kind: "IDLE" | "LOADING" }): StrategySettingsView {
  if (!view.available) return view;
  if (q.kind !== "ANSWER") return "reason" in q ? { ...view, basisNote: q.reason } : view;
  if (q.result.case !== "strategySettings") return { ...view, basisNote: `QUERY_ANSWER_MISMATCH ${q.result.case ?? ""}` };
  const s = q.result.value;
  if (s.availability !== Availability.AVAILABLE || !s.effective) return { ...view, basisNote: s.unavailableReason || "OWNER_UNAVAILABLE" };
  if (s.storedRevision !== view.generation) return { ...view, basisNote: `the settings state is of generation ${s.storedRevision}, the snapshot names ${view.generation}` };
  return { ...view, basis: {
    values: nativeText(s.effective), revision: s.storedRevision,
    apply: SettingsApplyStatus[s.applyStatus] ?? String(s.applyStatus), refusal: s.refusalReason, runningRevision: s.runningRevision,
    reasons: s.provenance?.qualityReasons ?? [],
    specs: Object.fromEntries(s.settingSpecs.map(x => [x.key, { unit: x.unit, min: x.min, max: x.max, places: x.places }])),
    features: Object.fromEntries(s.features.map(f => [f.name, { index: f.index, unit: f.unit, places: f.places, signed: f.signed, filterable: f.filterable }])),
  } };
}

/** The saved generation and whether it is in effect: the owner's apply status where its state
 *  answered, else the snapshot's running generation. */
export function generationText(view: StrategySettingsView): string {
  if (!view.available) return `Native owner ${view.reason}`;
  const saved = `Saved generation ${view.generation.toString()}`;
  const b = view.basis;
  if (!b) return `${saved}${view.running ? ` · ${view.startedGeneration?.toString() ?? "?"} in effect` : " · applies at the next start"}`;
  switch (b.apply) {
    case "IN_EFFECT": return `${saved} · in effect`;
    case "SAVED_APPLIES_NEXT_START": return `${saved} · running ${b.runningRevision === undefined ? "another configuration"
      : b.runningRevision === b.revision ? "it with older configuration file values" : `generation ${b.runningRevision.toString()}`} until the next start`;
    case "NOT_RUNNING": return `${saved} · applies at the next start${b.reasons.includes("RUNTIME_STATUS_STALE") ? " (runtime status stale)" : ""}`;
    case "REFUSED": return `${saved} · refused at the start: ${b.refusal}`;
    default: return `${saved} · running state unknown${b.reasons.length ? ` (${b.reasons.join("; ")})` : ""}`;
  }
}
