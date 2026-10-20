import type { PublicField } from "./types";

/** Display-only search. Preserve the native values and ordering exactly. */
export function filterReportFields(rows: readonly PublicField[], query: string): readonly PublicField[] {
  const needle = query.trim().toLowerCase();
  return needle ? rows.filter(row => `${row.name} ${row.value}`.toLowerCase().includes(needle)) : rows;
}
