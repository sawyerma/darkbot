// Frontend package (06.11.): the operator's Strategy C settings. The native owner
// (`bsc_runtime strategy set`) stores each request at once with its generation as CAS and checks
// the value with the runtime configuration's own parser; the running strategy took the settings
// of its start (runtime status `strategy[].settings*`). A new value is effective at the next start
// of Strategy C (its switch on, a runtime restart); nothing here validates or decides a value.
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
