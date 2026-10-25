// The five BSC signal sources (plan §5a.2, order V2 §5). The native
// bsc_runtime source owner is the only authority; the status reporter projects
// its current generation as the native record `darkbot.bsc.source-modes.v1`.
// Without it nothing can be requested (BACKEND_UNAVAILABLE). Effective modes
// are the owner's: LIVE is never effective, it shows BLOCKED with its reason.
import type { NativeRecordView } from "./types";

export const SOURCE_SCHEMA = "darkbot.bsc.source-modes.v1";
export const SOURCE_IDS = ["BSC_INGRESS_GETH", "BSC_INGRESS_PUBLICNODE", "BSC_INGRESS_DRPC", "BSC_INGRESS_BLOCKREQ", "BSC_INGRESS_48CLUB"] as const;
export const BACKEND_UNAVAILABLE = "BACKEND_UNAVAILABLE";

export interface SourceRow { id: string; requested: string; effective: string; blockReason: string }
export interface SourceControlView { available: boolean; reason: string; generation: bigint; rows: SourceRow[] }

/** The source group from the native record; nothing is inferred locally. */
export function sourceControlView(records: readonly NativeRecordView[]): SourceControlView {
  const unavailable = (reason: string): SourceControlView => ({
    available: false, reason, generation: 0n,
    rows: SOURCE_IDS.map(id => ({ id, requested: reason, effective: reason, blockReason: "" })),
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
    });
  }
  const ids = rows.map(r => r.id).sort();
  if (ids.join() !== [...SOURCE_IDS].sort().join()) return unavailable("BACKEND_INVALID");
  return { available: true, reason: "", generation, rows: SOURCE_IDS.map(id => rows.find(r => r.id === id)!) };
}
