// V8 Training modules (FE-V8-04, BSC-V8-01): the native profile catalog (bsc/query
// TrainingProfiles) and the requests of its four commands. A profile is a named, versioned
// Strategy C configuration of the native owner: create and clone at the catalog version, update and
// delete at the profile's revision; a revision is always sent whole (name, venues, every value).
// The owner checks names, venues and values again; nothing is stored in the browser.
import type { BscQueryOutcome } from "@/core/realtime/bscQuery";
import { Availability, TrainingProfileStatus } from "../api/bscV8Adapter";
import { nativeText, type FeatureSpec, type SettingSpec } from "./strategySettings";
import { profileValues, type StrategyForm, type StrategySettableValues } from "./strategyForm";

export interface ProfileView {
  id: string; name: string; revision: bigint; configSha256: string;
  /** Its venues as the owner stores them (native order). */
  venues: readonly string[];
  /** ACTIVE or ARCHIVED (a deleted profile is never shown). */
  status: string;
  inUse: boolean; inUseReason: string; runIds: readonly string[];
  /** Every value of this revision as the native canonical strings. */
  values: Readonly<Record<string, string | undefined>>;
  requestId: string; createdMs: bigint; updatedMs: bigint;
}
export type ProfilesView =
  | { ok: true; profiles: readonly ProfileView[]; catalogVersion: bigint; liveProfileId: string }
  | { ok: false; reason?: string };

/** The owner's catalog; not answered: `ok` false (the query's own reason names why). */
export function profilesOf(q: BscQueryOutcome | { kind: "IDLE" | "LOADING" }): ProfilesView {
  if (q.kind !== "ANSWER") return { ok: false };
  if (q.result.case !== "trainingProfiles") return { ok: false, reason: `QUERY_ANSWER_MISMATCH ${q.result.case ?? ""}` };
  const t = q.result.value;
  if (t.availability !== Availability.AVAILABLE) return { ok: false, reason: t.unavailableReason || "OWNER_UNAVAILABLE" };
  return {
    ok: true, catalogVersion: t.catalogVersion, liveProfileId: t.liveProfileId,
    profiles: t.profiles.filter(p => p.status !== TrainingProfileStatus.DELETED).map(p => ({
      id: p.profileId, name: p.name, revision: p.revision, configSha256: p.configSha256, venues: p.venues,
      status: TrainingProfileStatus[p.status] ?? String(p.status), inUse: p.inUse, inUseReason: p.inUseReason, runIds: p.runIds,
      values: p.values ? nativeText(p.values) : {}, requestId: p.requestId, createdMs: p.createdUnixMs, updatedMs: p.updatedUnixMs,
    })),
  };
}

/** A module as the operator edits it: its name, its venues and its form. */
export interface ModuleDraft { name: string; venues: readonly string[]; form: StrategyForm }

/** The params of a create or update (`others`: every other shown module, for the name check), or
 *  why it cannot be sent. A name another module has, no name or no venue is refused here; the
 *  owner checks the name's form and every value again. */
export function moduleParams(draft: ModuleDraft, others: readonly ProfileView[], order: readonly string[],
  specs?: Readonly<Record<string, SettingSpec>>, features?: Readonly<Record<string, FeatureSpec>>):
  { name: string; venues: string[]; values: StrategySettableValues; filters: string } | { error: string } {
  if (!draft.name.trim()) return { error: "Enter a module name" };
  if (others.some(p => p.name === draft.name)) return { error: `A module named “${draft.name}” exists` };
  if (!draft.venues.length) return { error: "Choose a market" };
  const v = profileValues(draft.form, order, specs, features);
  return "error" in v ? v : { name: draft.name, venues: [...draft.venues], ...v };
}

/** The market choices of a module (V8: All markets, or one): every offered venue sent explicitly,
 *  each one alone, and the module's own set where it is none of them. */
export function marketOptions(offered: readonly string[], current: readonly string[], label: (id: string) => string): { key: string; label: string; venues: readonly string[] }[] {
  const out = [
    ...(offered.length > 1 ? [{ key: offered.join(","), label: "All markets", venues: offered }] : []),
    ...offered.map(id => ({ key: id, label: label(id), venues: [id] })),
  ];
  const key = current.join(",");
  if (current.length && !out.some(o => o.key === key)) out.push({ key, label: current.map(label).join(" + "), venues: current });
  return out;
}

/** A clone's name: the source's with “(copy)”, numbered while another module has it. */
export function copyName(name: string, all: readonly ProfileView[]): string {
  for (let n = 1; ; n++) {
    const candidate = n === 1 ? `${name} (copy)` : `${name} (copy ${n})`;
    if (!all.some(p => p.name === candidate)) return candidate;
  }
}

/** The profile a decided request wrote (`outcome=APPLIED … profile_id=<id>`), from its receipt line. */
export function appliedProfile(detail: string): string | undefined {
  if (!/(^|\s)outcome=APPLIED(\s|$)/.test(detail)) return undefined;
  const id = /(^|\s)profile_id=(\S+)/.exec(detail)?.[2];
  return id && id !== "-" ? id : undefined;
}
