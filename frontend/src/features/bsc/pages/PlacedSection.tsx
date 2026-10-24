import type { ReactNode } from "react";
import { Line, Section } from "@/shared/ui/graphite/Rows";
import { missingRecords, nativeRows, SECTIONS, sectionRecords, shortSchema, type PanelKey } from "../model/placement";
import type { NativeRecordView } from "../model/types";
import { PublicRows, timestamp, ValueState } from "./EvidenceRows";
import { FilteredEvidence } from "./FilteredEvidence";

/**
 * One numbered §13 section of an existing BSC panel: typed rows first, then every public native
 * leaf placed here (rows, exact units, DBREC name as hint), then record families without data.
 */
export function PlacedSection({ panel, section, records, children }: { panel: PanelKey; section: number; records: readonly NativeRecordView[]; children?: ReactNode }) {
  const title = SECTIONS[panel][section - 1] ?? "";
  const rows = nativeRows(records, panel, section);
  return <Section title={`${section}. ${title}`}>
    {children}
    {sectionRecords(records, panel, section).map(r =>
      <Line key={r.schema} label={`Record · ${shortSchema(r.schema)}`} value={<ValueState state={r.provenance} />}
        hint={`observed ${timestamp(r.evidence?.observedUnixS)} · ${r.evidence?.path || "—"}`} />)}
    {rows.length > 12 ? <FilteredEvidence rows={rows} label={`Filter ${title}`} /> : <PublicRows rows={rows} />}
    {missingRecords(records, panel, section).map(m =>
      <Line key={m.schemas} label={m.schemas} value={<ValueState state={m.status} />} hint={m.reason} />)}
  </Section>;
}
