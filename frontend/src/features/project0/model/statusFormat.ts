export const DASH = "—";

export function orDash(v: bigint | string | null | undefined): string {
  return v === null || v === undefined || v === "" ? DASH : v.toString();
}

export function fmtAgeNs(ns: bigint | null | undefined): string {
  if (ns === null || ns === undefined) return DASH;
  if (ns < 1_000_000n) return `${ns} ns`;
  const ms = ns / 1_000_000n;
  return ms < 10_000n ? `${ms} ms` : `${ms / 1000n} s`;
}

export function fmtBytes(b: bigint | null | undefined): string {
  if (b === null || b === undefined) return DASH;
  if (b < 1024n) return `${b} B`;
  const kib = b / 1024n;
  if (kib < 10_240n) return `${kib} KiB`;
  const mib = kib / 1024n;
  return mib < 10_240n ? `${mib} MiB` : `${mib / 1024n} GiB`;
}

/** Exact byte count; shows every change of a continuously measured value. */
export function fmtBytesExact(b: bigint | null | undefined): string {
  return b === null || b === undefined ? DASH : `${b.toLocaleString("en-US")} B`;
}

export function fmtRate(bytesPerS: number): string {
  if (bytesPerS < 1024) return `${bytesPerS.toFixed(0)} B/s`;
  const kib = bytesPerS / 1024;
  return kib < 10_240 ? `${kib.toFixed(1)} KiB/s` : `${(kib / 1024).toFixed(1)} MiB/s`;
}

/** An absent forecast can mean missing disk/rate data; never promise unlimited capacity. */
export function fmtEtaS(s: number | null | undefined): string {
  if (s === null || s === undefined || !Number.isFinite(s)) return "not available";
  if (s < 120) return `${s.toFixed(0)} s`;
  return s < 7_200 ? `${(s / 60).toFixed(0)} min` : `${(s / 3_600).toFixed(1)} h`;
}

/** Wall-clock time with milliseconds, local time zone. */
export function fmtClockMs(unixMs: bigint | number): string {
  const ms = Number(unixMs);
  const t = new Date(ms).toLocaleTimeString("en-GB", { hour12: false });
  return `${t}.${String(ms % 1000).padStart(3, "0")}`;
}

/** Formats one contract value by the unit encoded in its field name. */
export function formatField(name: string, v: unknown): string {
  if (v === null || v === undefined || v === "") return DASH;
  if (typeof v === "boolean") return v ? "yes" : "no";
  const big = typeof v === "bigint" ? v : typeof v === "string" && /^\d+$/.test(v) ? BigInt(v) : null;
  const num = typeof v === "number" ? v : null;
  if (big !== null) {
    if (name.endsWith("_unix_ms")) return fmtClockMs(big);
    if (name.endsWith("_unix_s")) return fmtClockMs(big * 1000n);
    if (name.endsWith("_bytes")) return `${fmtBytes(big)} · ${fmtBytesExact(big)}`;
    if (name.endsWith("_ns")) return fmtAgeNs(big);
    if (name.endsWith("_ms")) return `${big} ms`;
    if (name.endsWith("_seconds") || name.endsWith("_secs")) return `${big} s`;
    if (name.endsWith("_lamports")) return `${big.toLocaleString("en-US")} lamports`;
    return big.toLocaleString("en-US");
  }
  if (num !== null) {
    if (name.endsWith("_bytes_per_s")) return fmtRate(num);
    if (name.endsWith("_usd")) return `${num.toFixed(6)} USD`;
    if (name.endsWith("_s")) return fmtEtaS(num);
    return String(num);
  }
  return String(v);
}
