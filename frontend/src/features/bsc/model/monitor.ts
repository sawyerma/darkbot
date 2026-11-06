// V8 Live market monitoring (FE-V8-12, BSC-V8-11): launch detection from the native G0.4 census
// (darkbot.bsc.g04-census.v1) and the five sources from the runtime status
// (darkbot.bsc.runtime-status.v1) and the census' per-source first sightings. Each market keeps
// its own denominator; the census names no separate Four.meme Classic/OpenFour timing, so Four is
// one market with its Classic/OpenFour launch counts. A missing sample is "—".
import type { NativeRecordView } from "./types";
import { RUNTIME_SCHEMA, runtimeView } from "./sources";

export const CENSUS_SCHEMA = "darkbot.bsc.g04-census.v1";
export type Market = "flap" | "four";

export interface Detection { total?: string; full?: string; hash?: string; after?: string; missed?: string; p50Us?: string; p95Us?: string; p99Us?: string; detail: string; clockSuspect: boolean; observed?: number }

function census(records: readonly NativeRecordView[]): Map<string, string> | null {
  const r = records.find(x => x.schema === CENSUS_SCHEMA);
  return r ? new Map(r.fields.map(f => [f.name, f.value])) : null;
}

export function detection(records: readonly NativeRecordView[], market: Market): Detection {
  const c = census(records);
  if (!c) return { detail: "No G0.4 census record", clockSuspect: false };
  const g = (k: string) => c.get(k);
  const observed = Number(g("_provenance.observed_unix_ms"));
  const base = { clockSuspect: g("clock_suspect") === "true", observed: Number.isFinite(observed) && observed > 0 ? observed : undefined };
  if (market === "flap") return { ...base, total: g("flap_launches"), full: g("full_before"), hash: g("hash_only_before"), after: g("after"), missed: g("missed"),
    p50Us: g("lead_p50_us"), p95Us: g("lead_p95_us"), p99Us: g("lead_p99_us"), detail: `clock unknown ${g("clock_unknown") ?? "?"} · evicted ${g("evicted") ?? "?"}` };
  return { ...base, total: g("four_launches"), full: g("four_full_before"), hash: g("four_hash_only_before"), after: g("four_after"), missed: g("four_missed"),
    p50Us: g("four_lead_p50_us"), detail: `Classic ${g("classic_launches") ?? "?"} (actionable ${g("classic_actionable") ?? "?"}) · OpenFour ${g("openfour_launches") ?? "?"} (actionable ${g("openfour_actionable") ?? "?"}) · lead max ${g("four_lead_max_us") ?? "?"} µs · clock unknown ${g("four_clock_unknown") ?? "?"}` };
}

/** µs as ms text with one place (exact integer arithmetic), "—" without a sample. */
export function usToMs(us: string | undefined): string {
  if (us === undefined || !/^-?[0-9]+$/.test(us)) return "—";
  const v = BigInt(us), neg = v < 0n, a = neg ? -v : v;
  const tenths = (a + 50n) / 100n;
  return `${neg ? "−" : ""}${tenths / 10n}.${tenths % 10n} ms`;
}
/** full/total as a percent with one place; "—" without a denominator. */
export function share(part: string | undefined, total: string | undefined): string {
  if (!part || !total || !/^[0-9]+$/.test(part) || !/^[0-9]+$/.test(total) || total === "0") return "—";
  const permille = (BigInt(part) * 1000n + BigInt(total) / 2n) / BigInt(total);
  return `${permille / 10n}.${permille % 10n} %`;
}

export interface SourceLine { id: string; health: string; first?: string; lagP50Us?: string; queueMeanNs?: string; dropped?: string; lastAgeMs?: string; fresh: boolean }
/** The runtime's sources with the census' first full sightings before the block (Flap census). */
export function sourceLines(records: readonly NativeRecordView[], nowMs: number): SourceLine[] {
  const fresh = runtimeView(records, nowMs).view.state === "RUNNING";
  const rt = new Map(records.find(r => r.schema === RUNTIME_SCHEMA)?.fields.map(x => [x.name, x.value]) ?? []);
  const c = census(records) ?? new Map<string, string>();
  const censusOf = new Map<string, number>();
  for (let i = 0; c.has(`sources[${i}].source_id`); i++) censusOf.set(c.get(`sources[${i}].source_id`) ?? "", i);
  const out: SourceLine[] = [];
  for (let i = 0; rt.has(`sources[${i}].source_id`); i++) {
    const id = rt.get(`sources[${i}].source_id`) ?? "";
    const j = censusOf.get(id);
    out.push({ id, fresh, health: fresh ? rt.get(`sources[${i}].health`) ?? "UNKNOWN" : "STALE",
      first: j === undefined ? undefined : c.get(`sources[${j}].first_full_before`), lagP50Us: j === undefined ? undefined : c.get(`sources[${j}].later_lag_p50_us`),
      queueMeanNs: rt.get(`sources[${i}].queue_mean_ns`), dropped: rt.get(`sources[${i}].dropped`), lastAgeMs: fresh ? rt.get(`sources[${i}].last_event_age_ms`) : undefined });
  }
  return out;
}
/** ns as ms text with three places (the reference's queue column). */
export function nsMs3(ns: string | undefined): string {
  if (ns === undefined || !/^[0-9]+$/.test(ns)) return "—";
  const v = (BigInt(ns) + 500n) / 1000n;
  return `${v / 1000n}.${(v % 1000n).toString().padStart(3, "0")} ms`;
}
