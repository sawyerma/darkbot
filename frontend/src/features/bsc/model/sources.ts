// The five BSC signal sources (plan §5a.2, order V2 §5). The native
// bsc_runtime source owner is the only authority; the status reporter projects
// its current generation as the native record `darkbot.bsc.source-modes.v1`.
// Without it nothing can be requested (BACKEND_UNAVAILABLE). Effective modes
// are the owner's: LIVE is never effective, it shows BLOCKED with its reason.
import type { NativeRecordView } from "./types";

export const SOURCE_SCHEMA = "darkbot.bsc.source-modes.v1";
export const RUNTIME_SCHEMA = "darkbot.bsc.runtime-status.v1";
export const SOURCE_IDS = ["BSC_INGRESS_GETH", "BSC_INGRESS_PUBLICNODE", "BSC_INGRESS_DRPC", "BSC_INGRESS_BLOCKREQ", "BSC_INGRESS_48CLUB"] as const;
/** Switchable runtime components of the same native owner (C2 state, ...). */
export const COMPONENT_IDS = ["BSC_STATE_C2"] as const;
/** A component the owner's record does not name yet (a generation from before it existed). */
export const NOT_RECORDED = "NOT_RECORDED";
export const BACKEND_UNAVAILABLE = "BACKEND_UNAVAILABLE";

export interface SourceRow { id: string; requested: string; effective: string; blockReason: string; health: string; counters: string }
/** The canonical chain follower (G0.4 denominator); absent when not configured. */
export interface CanonicalView { state: string; head: string; headAgeMs: string; chain: string; launches: string; join: string }
/** The C2 chain-state owner (component BSC_STATE_C2 effective SHADOW); absent while it does not run. */
export interface ChainStateView { coherent: string; head: string; held: string; effects: string; faults: string; checkpoint: string }
export interface RuntimeView { state: "RUNNING" | "STALE" | "NOT_RUNNING"; mode: string; launches: string; journal: string; observedMs?: number; canonical?: CanonicalView; chainState?: ChainStateView }
export interface SourceControlView { available: boolean; reason: string; generation: bigint; rows: SourceRow[]; components: SourceRow[]; runtime: RuntimeView }

/** Rewrites a runtime status may miss before it is STALE. */
export const STALE_AFTER_INTERVALS = 3;

/** The runtime's own status record; stale once older than STALE_AFTER_INTERVALS × its own
 *  rewrite interval (`status_interval_ms`, never assumed running; without it nothing is fresh). */
export function runtimeView(records: readonly NativeRecordView[], nowMs: number): { view: RuntimeView; health: Map<string, { health: string; counters: string }> } {
  const health = new Map<string, { health: string; counters: string }>();
  const record = records.find(r => r.schema === RUNTIME_SCHEMA);
  if (!record) return { view: { state: "NOT_RUNNING", mode: "UNKNOWN", launches: "UNKNOWN", journal: "UNKNOWN" }, health };
  const f = new Map(record.fields.map(x => [x.name, x.value]));
  const observed = Number(f.get("_provenance.observed_unix_ms"));
  const interval = Number(f.get("status_interval_ms"));
  const fresh = Number.isFinite(observed) && observed > 0 && Number.isFinite(interval) && interval > 0
    && nowMs - observed <= STALE_AFTER_INTERVALS * interval;
  for (let i = 0; f.has(`sources[${i}].source_id`); i++) {
    const g = (k: string) => f.get(`sources[${i}].${k}`) ?? "?";
    health.set(f.get(`sources[${i}].source_id`) ?? "", {
      health: fresh ? g("health") : "STALE",
      counters: `full ${g("full")} · hash ${g("hash_only")} · rejected ${g("rejected")} · first ${g("first_full")} · launches ${g("launch_sightings")}/${g("launch_first")} · dropped ${g("dropped")}`,
    });
  }
  const c = (k: string) => f.get(`canonical[0].${k}`) ?? "?";
  const canonical: CanonicalView | undefined = f.has("canonical[0].state") ? {
    state: fresh ? c("state") : "STALE",
    head: f.get("canonical[0].head") ?? "UNKNOWN",
    headAgeMs: f.get("canonical[0].head_age_ms") ?? "UNKNOWN",
    chain: `blocks ${c("blocks")} · skipped ${c("blocks_skipped")} · reorgs ${c("reorgs")} · resyncs ${c("resyncs")} · rpc errors ${c("rpc_errors")} · rate limited ${c("rate_limited")} · header fetch lag ${f.get("canonical[0].head_fetch_lag_ms") ?? "UNKNOWN"} ms · before this start (C2 backfill, not counted) ${c("backfill_blocks")} blocks, ${c("backfill_launches")} launches`,
    launches: `Flap ${c("flap_launches")} (C1 scope ${c("c1_scope_launches")}) · Four ${c("four_launches")} · orphaned ${c("orphaned_launches")} · waiting ${c("waiting")} · tx not rebuildable ${c("tx_unverifiable")} · tx answers refused ${c("tx_answer_refused")} · external buys ${c("external_buys")} (unknown ${c("buys_unknown")})`,
    join: `full before block time ${c("seen_full_before")} (C1 scope ${c("c1_scope_full_before")}) · hash only before ${c("seen_hash_only_before")} · after ${c("seen_after")} · missed ${c("missed")} · clock unknown ${c("clock_unknown")}`,
  } : undefined;
  const s = (k: string) => f.get(`state[0].${k}`) ?? "?";
  const chainState: ChainStateView | undefined = f.has("state[0].coherent") ? {
    coherent: !fresh ? "STALE" : s("coherent") === "true" ? "COHERENT" : "NOT_COHERENT",
    head: f.get("state[0].head") ?? "UNANCHORED",
    held: `retained blocks ${s("retained_blocks")} · txs ${s("held_txs")} (candidates ${s("held_candidates")}, uncertain ${s("held_uncertain")}) · senders ${s("held_senders")} (unknown ${s("unknown_senders")}, quarantined ${s("quarantined_senders")}) · watched ${s("watched")}`,
    effects: `blocks ${s("blocks")} · included ${s("included")} (reverted ${s("reverted")}) · finalized ${s("finalized_blocks")} · reorgs ${s("reorgs")} (beyond window ${s("reorgs_beyond_window")}, too deep ${s("reorgs_too_deep")}) · gaps ${s("gaps")} · orphaned txs ${s("orphaned_txs")} · uncertain ${s("uncertain_marked")} · pending ${s("pending_observations")} (repeat ${s("repeat_sightings")}, earlier ${s("earlier_first_seen")}, late ${s("late_sightings")}, stale ${s("stale_pending")}, competing ${s("competing_candidates")}, replaced ${s("replaced")}) · nonce facts ${s("nonce_facts")} (reconciled ${s("reconciliations")}, refused ${s("reconciliation_refusals")}, ignored ${s("ignored_nonce_facts")})`,
    faults: `conflicts ${s("conflicts")} (refused inputs ${s("refused_inputs")}, last ${f.get("state[0].last_conflict") ?? "none"}) · overflow txs ${s("overflow_txs")} · overflow candidates ${s("overflow_candidates")} · evicted ${s("evicted")} · unmatched orphaned blocks ${s("unmatched_orphaned_blocks")}`,
    checkpoint: `restore ${s("checkpoint_restore")} (${s("restored_blocks")} blocks) · quarantines ${s("quarantines")} · writes ${s("checkpoint_writes")} · errors ${s("checkpoint_errors")}`,
  } : undefined;
  return { view: { state: fresh ? "RUNNING" : "STALE", mode: f.get("mode") ?? "UNKNOWN", launches: f.get("launches") ?? "UNKNOWN", journal: f.get("journal_path") ?? "UNKNOWN", observedMs: Number.isFinite(observed) ? observed : undefined, canonical, chainState }, health };
}

/** The source group from the native record; nothing is inferred locally. */
export function sourceControlView(records: readonly NativeRecordView[], nowMs = Date.now()): SourceControlView {
  const { view: runtime, health } = runtimeView(records, nowMs);
  const healthOf = (id: string) => health.get(id) ?? { health: runtime.state === "NOT_RUNNING" ? "NOT_RUNNING" : "UNKNOWN", counters: "" };
  const unavailable = (reason: string): SourceControlView => ({
    available: false, reason, generation: 0n, runtime,
    rows: SOURCE_IDS.map(id => ({ id, requested: reason, effective: reason, blockReason: "", ...healthOf(id) })),
    components: COMPONENT_IDS.map(id => ({ id, requested: reason, effective: reason, blockReason: "", health: "", counters: "" })),
  });
  const record = records.find(r => r.schema === SOURCE_SCHEMA);
  if (!record) return unavailable(BACKEND_UNAVAILABLE);
  const f = new Map(record.fields.map(x => [x.name, x.value]));
  let generation: bigint;
  try { generation = BigInt(f.get("generation") ?? "x"); } catch { return unavailable("BACKEND_INVALID"); }
  const rows: SourceRow[] = [];
  for (let i = 0; f.has(`sources[${i}].source_id`); i++) {
    rows.push({
      id: f.get(`sources[${i}].source_id`) ?? "",
      requested: f.get(`sources[${i}].requested`) ?? "UNKNOWN",
      effective: f.get(`sources[${i}].effective`) ?? "UNKNOWN",
      blockReason: f.get(`sources[${i}].block_reason`) ?? "",
      ...healthOf(f.get(`sources[${i}].source_id`) ?? ""),
    });
  }
  const ids = rows.map(r => r.id).sort();
  if (ids.join() !== [...SOURCE_IDS].sort().join()) return unavailable("BACKEND_INVALID");
  const found = new Map<string, SourceRow>();
  for (let i = 0; f.has(`components[${i}].component_id`); i++) {
    const id = f.get(`components[${i}].component_id`) ?? "";
    if (!(COMPONENT_IDS as readonly string[]).includes(id) || found.has(id)) return unavailable("BACKEND_INVALID");
    found.set(id, { id, requested: f.get(`components[${i}].requested`) ?? "UNKNOWN", effective: f.get(`components[${i}].effective`) ?? "UNKNOWN",
      blockReason: f.get(`components[${i}].block_reason`) ?? "", health: "", counters: "" });
  }
  const components = COMPONENT_IDS.map(id => found.get(id) ?? { id, requested: NOT_RECORDED, effective: NOT_RECORDED, blockReason: "", health: "", counters: "" });
  return { available: true, reason: "", generation, runtime, rows: SOURCE_IDS.map(id => rows.find(r => r.id === id)!), components };
}
