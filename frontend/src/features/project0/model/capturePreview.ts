import { REPORT_MARKERS, type CaptureMode, type CaptureSource, type ReportMarker, type VerificationMode } from "./contracts";

/** `--chain-verify` values `p0ctl run start` accepts; it refuses index/crosscheck (CHAIN_VERIFY_*_BLOCKED). */
export const RUN_START_VERIFY: ReadonlySet<VerificationMode> = new Set(["off", "strict"]);
/** run start is the C0-E1 measurement (market + shadow); it has no mode option. */
const RUN_START_MODE: CaptureMode = "econ4h";
/** Required on every run start (p0ctl.rs BOOT_NETWORK_AUDIT_REQUIRED / MAINNET_APPROVAL_REQUIRED): operator input, never generated. */
const APPROVAL = "--network-audit <FILE> --mainnet-approved <TOKEN>";
/** runtime_config.rs::duration_seconds: unsigned integer with unit s/m/h/d, > 0 and <= u64::MAX / 4 / 1e9 seconds. */
const SCALE: Readonly<Record<string, bigint>> = { s: 1n, m: 60n, h: 3600n, d: 86400n };
const MAX_DURATION_S = (2n ** 64n - 1n) / 4n / 1_000_000_000n;

export interface RunStartDraft { duration: string; source: CaptureSource; mode: CaptureMode; verify: VerificationMode; markers: readonly ReportMarker[] }

function durationValid(text: string): boolean {
  const [, digits, unit] = /^(\d+)([smhd])$/.exec(text) ?? [];
  const scale = unit === undefined ? undefined : SCALE[unit];
  if (digits === undefined || scale === undefined) return false;
  const seconds = BigInt(digits) * scale;
  return seconds > 0n && seconds <= MAX_DURATION_S;
}

/** `--report-markers` value in contract order (market/markers.rs Marker::ALL). */
export function markerSelector(markers: readonly ReportMarker[]): string {
  if (markers.length === 0) return "none";
  const chosen = REPORT_MARKERS.filter(m => markers.includes(m.id)).map(m => m.id);
  return chosen.length === REPORT_MARKERS.length ? "all" : chosen.join(",");
}

/** The `p0ctl run start` command for the panel draft, or why run start would refuse it. */
export function runStartPreview(d: RunStartDraft): { command: string | null; problem: string | null } {
  if (!durationValid(d.duration)) return { command: null, problem: "Duration must be a positive whole number within the run start limit" };
  if (d.mode !== RUN_START_MODE) return { command: null, problem: "p0ctl run start has no mode option; it always records market + shadow" };
  if (!RUN_START_VERIFY.has(d.verify)) return { command: null, problem: `CHAIN_VERIFY_${d.verify}_BLOCKED by p0ctl run start` };
  return { command: `p0ctl run start --duration ${d.duration} ${APPROVAL} --capture-source ${d.source} --chain-verify ${d.verify} --report-markers ${markerSelector(d.markers)}`, problem: null };
}
