// Places each public native leaf in exactly one panel section (FRONTEND_PHASENPLAN §13) with
// the rules of frontend/contracts/bsc-fields.json (generated copy). Nothing is computed from values.
import { NATIVE_BINDINGS, PANEL_SECTIONS, type NativeBinding } from "./placement.generated";
import { formatNative } from "./format";
import type { NativeRecordView, PublicField } from "./types";

export type PanelKey = keyof typeof PANEL_SECTIONS;
export const SECTIONS: Record<PanelKey, readonly string[]> = PANEL_SECTIONS;

/** The census symbol of a wire name: nested rows parent[i].x -> parent[].x, repeated leaves x[i] -> x. */
export function censusName(wire: string): string {
  return wire.replace(/\[\d+\]\./g, "[].").replace(/\[\d+\]$/, "");
}
const rank = (m: NativeBinding["match"]) => m === "*" ? 0 : typeof m === "string" ? m.length : Number.MAX_SAFE_INTEGER;
const hits = (m: NativeBinding["match"], s: string) => m === "*" || (typeof m === "string" ? s.startsWith(m.slice(7)) : m.includes(s));

/** Same resolution as check:bsc-fields: explicit list, then longest prefix, then '*'. */
export function bindingOf(schema: string, wireName: string): NativeBinding | null {
  const name = censusName(wireName);
  let best: NativeBinding | null = null;
  for (const b of NATIVE_BINDINGS) {
    if (b.schemas.includes(schema) && hits(b.match, name) && (!best || rank(b.match) > rank(best.match))) best = b;
  }
  return best;
}
/** Only leaves transported as NativeRecord render here; typed fields keep their typed rows. */
const native = (b: NativeBinding | null): b is NativeBinding => !!b && b.status !== "EXCLUDED" && b.wire.startsWith("NativeRecord");

const UNIT_SUFFIX: Record<string, RegExp> = {
  wei: /_wei$/, unix_milliseconds: /_unix_ms$/, unix_seconds: /_unix_s$/, nanoseconds: /_ns$/,
  milliseconds: /_ms$/, basis_points: /_bps$/, seconds: /_seconds$/,
};
/** Readable label; the technical DBREC name stays visible as hint. */
export function labelOf(name: string, unit: string): string {
  const suffix = UNIT_SUFFIX[unit];
  return name.split(".").map(part => {
    const index = /\[(\d+)\]$/.exec(part);
    const base = (suffix ? part.replace(/\[\d+\]$/, "").replace(suffix, "") : part.replace(/\[\d+\]$/, "")).replace(/\[\]$/, "");
    const words = base.replace(/^_+/, "").replace(/_/g, " ");
    return index ? `${words} #${Number(index[1]) + 1}` : words;
  }).join(" · ").trim().replace(/^./, c => c.toUpperCase());
}
export const shortSchema = (schema: string) => schema.replace(/^darkbot\./, "");

export function nativeRows(records: readonly NativeRecordView[], panel: PanelKey, section: number): PublicField[] {
  const rows: PublicField[] = [];
  for (const r of records) for (const f of r.fields) {
    const b = bindingOf(r.schema, f.name);
    if (native(b) && b.panel === panel && b.section === section) {
      rows.push({ name: labelOf(f.name, f.unit), value: formatNative(f.value, f.unit), hint: `${f.name} · ${shortSchema(r.schema)}` });
    }
  }
  return rows;
}
/** Records that contribute rows to a section, for one provenance line each. */
export function sectionRecords(records: readonly NativeRecordView[], panel: PanelKey, section: number): NativeRecordView[] {
  return records.filter(r => r.fields.some(f => { const b = bindingOf(r.schema, f.name); return native(b) && b.panel === panel && b.section === section; }));
}
/** Record families placed in this section without any present record: NO_RECORD or FUTURE_PHASE_BLOCKED. */
export function missingRecords(records: readonly NativeRecordView[], panel: PanelKey, section: number): { schemas: string; status: string; reason?: string }[] {
  const present = new Set(records.map(r => r.schema));
  const out = new Map<string, { schemas: string; status: string; reason?: string }>();
  for (const b of NATIVE_BINDINGS) {
    if (b.panel !== panel || b.section !== section || b.match !== "*" || b.status === "EXCLUDED") continue;
    if (!b.wire.startsWith("NativeRecord") && b.status !== "FUTURE_PHASE_BLOCKED") continue;
    if (b.schemas.some(s => present.has(s))) continue;
    const key = b.schemas.map(shortSchema).join(" / ");
    out.set(key, { schemas: key, status: b.status === "FUTURE_PHASE_BLOCKED" ? "FUTURE_PHASE_BLOCKED" : "NO_RECORD", reason: b.reason });
  }
  return [...out.values()];
}
