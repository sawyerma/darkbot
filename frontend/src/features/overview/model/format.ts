import { DASH } from "@/shared/ui/graphite/Rows";

// USD-Micro exakt formatieren (10^-6). Kein Float, keine Rundung über BigInt hinaus.
const SIGNED = /^(?:0|[1-9][0-9]*|-[1-9][0-9]*)$/;

function microToUsd(abs: bigint): string {
  const whole = abs / 1_000_000n;
  const frac = (abs % 1_000_000n).toString().padStart(6, "0").replace(/0+$/, "").padEnd(2, "0");
  return `${whole.toLocaleString("en-US")}.${frac}`;
}

export function formatUsdMicroSigned(v: string | null): string {
  if (v === null || !SIGNED.test(v)) return `${DASH} USD`;
  const n = BigInt(v);
  const sign = n < 0n ? "−" : "";
  return `${sign}${microToUsd(n < 0n ? -n : n)} USD`;
}

export function formatUsdMicro(v: bigint | null): string {
  if (v === null || v < 0n) return `${DASH} USD`;
  return `${microToUsd(v)} USD`;
}

export function formatCount(n: number | null): string {
  return n === null ? DASH : String(n);
}

export function formatLatency(l: { p95Ms: number | null; windowS: number; samples: number } | null): string {
  if (!l || l.p95Ms === null || l.samples <= 0 || !Number.isSafeInteger(l.p95Ms) || l.p95Ms < 0) return DASH;
  return `${l.p95Ms} ms`;
}

export function latencyHint(l: { p95Ms: number | null; windowS: number; samples: number } | null): string | undefined {
  if (!l || l.p95Ms === null || l.samples <= 0) return undefined;
  return `p95 · ${l.windowS}s window · ${l.samples} samples`;
}

export function formatAge(nowMs: number, ns: bigint | null): string {
  if (ns === null) return DASH;
  const stamp = ns / 1_000_000n;
  if (stamp < 0n || stamp > BigInt(Number.MAX_SAFE_INTEGER)) return DASH;
  const elapsed = nowMs - Number(stamp);
  if (!Number.isFinite(elapsed) || elapsed < -5_000) return DASH;
  const ageS = Math.max(0, Math.floor(elapsed / 1000));
  return ageS < 60 ? `${ageS}s ago` : ageS < 3600 ? `${Math.floor(ageS / 60)}m ago` : `${Math.floor(ageS / 3600)}h ago`;
}
