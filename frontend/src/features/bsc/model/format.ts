import { DASH } from "@/shared/ui/graphite/Rows";

// Exakte Darstellung ohne JS-Float (Wire V1.3 §8). Nur Formatierung, keine Rechnung.

const UNSIGNED = /^(0|[1-9][0-9]*)$/;

export function formatWei(wei: string | null, decimals = 18, maxFraction = 6, unit = "BNB"): string {
  if (wei === null || !UNSIGNED.test(wei)) return DASH;
  if (!Number.isInteger(decimals) || decimals < 0 || decimals > 36 ||
      !Number.isInteger(maxFraction) || maxFraction < 0 || maxFraction > 36 ||
      wei.length > 78) return DASH;
  const value = BigInt(wei);
  if (decimals === 0) return `${value.toString()} ${unit}`;
  const base = 10n ** BigInt(decimals);
  const whole = value / base;
  const fraction = (value % base).toString().padStart(decimals, "0").slice(0, Math.min(maxFraction, decimals)).replace(/0+$/, "");
  return `${fraction ? `${whole}.${fraction}` : whole.toString()} ${unit}`;
}

export function formatBps(bps: number | null): string {
  return bps === null || !Number.isSafeInteger(bps) || bps < 0 ? DASH : `${bps} bps`;
}

export function formatVersion(v: bigint | null): string {
  return v === null ? DASH : `v${v.toString()}`;
}

export function formatUnixNs(ns: bigint | null): string {
  if (ns === null) return DASH;
  const ms = Number(ns / 1_000_000n);
  if (!Number.isSafeInteger(ms) || Math.abs(ms) > 8_640_000_000_000_000) return DASH;
  return new Date(ms).toISOString().replace("T", " ").replace(/\.\d{3}Z$/, "Z");
}

export function shortId(id: string, head = 8, tail = 4): string {
  if (!id) return DASH;
  return id.length <= head + tail + 1 ? id : `${id.slice(0, head)}…${id.slice(-tail)}`;
}

export function yesNo(v: boolean | null): string {
  return v === null ? DASH : v ? "Yes" : "No";
}
