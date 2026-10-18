export const DASH = "—";
export function orDash(v: bigint | string | null | undefined): string {
  return v === null || v === undefined || v === "" ? DASH : v.toString();
}
export function fmtAgeNs(ns: bigint | null): string {
  if (ns === null) return DASH;
  if (ns < 1_000_000n) return `${ns} ns`;
  const ms = ns / 1_000_000n;
  return ms < 10_000n ? `${ms} ms` : `${ms / 1000n} s`;
}
export function fmtBytes(b: bigint | null): string {
  if (b === null) return DASH;
  if (b < 1024n) return `${b} B`;
  const kib = b / 1024n;
  return kib < 10_240n ? `${kib} KiB` : `${kib / 1024n} MiB`;
}
