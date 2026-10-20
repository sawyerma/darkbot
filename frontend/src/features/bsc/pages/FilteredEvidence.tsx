import { useId, useState } from "react";
import type { PublicField } from "../model/types";
import { filterReportFields } from "../model/reportFilters";
import { PublicRows } from "./EvidenceRows";

export function FilteredEvidence({ rows = [], label }: { rows?: PublicField[]; label: string }) {
  const [query, setQuery] = useState("");
  const id = useId();
  const visible = filterReportFields(rows, query);
  return <div className="bsc-report-fields">
    <div className="bsc-filter">
      <label htmlFor={id}>{label}</label>
      <input id={id} type="search" value={query} onChange={event => setQuery(event.target.value)}
        placeholder="Field or value" autoComplete="off" />
      <button className="g-secondary" disabled={!query} onClick={() => setQuery("")}>Clear filter</button>
    </div>
    <p className="g-note" role="status">{visible.length} / {rows.length} fields · display filter only</p>
    <PublicRows rows={[...visible]} />
    {!rows.length ? <p className="g-note">No native fields available.</p>
      : !visible.length ? <p className="g-note">No matching fields. Clear the filter to show the full report.</p> : null}
  </div>;
}
