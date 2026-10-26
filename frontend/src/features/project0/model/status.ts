/** View models of the Project_0 topics; produced only by api/project0ViewAdapter.ts. */
export type CapturePhaseName = "WAITING_FOR_STREAM" | "CAPTURE" | "DRAIN" | "FINALIZE" | "ENDED" | "UNKNOWN";

export interface RunView {
  runId: string;
  programId: string;
  source: string;
  commitment: string;
  wsHost: string;
  rpcHost: string | null;
  configuredDurationSeconds: bigint;
  readyTimeoutSeconds: bigint;
  processStartedUnixS: bigint;
  sourceCoverageVerified: boolean;
}

export interface PhaseView {
  phase: CapturePhaseName;
  changedUnixMs: bigint;
  streamReadyUnixS: bigint | null;
  stopReason: string;
  drainComplete: boolean | null;
  writeError: string | null;
  endResult: string;
}

export interface TransportView {
  connected: boolean | null;
  lastFrameAgeNs: bigint | null;
  applicationFrames: bigint | null;
  /** A01: reconnects of the ingress before its current session; null before a health report. */
  reconnects: number | null;
  lastSlot: bigint | null;
  lastState: string;
  lastCode: string;
  lastAttempt: number | null;
}

export interface MarketView {
  observing: boolean;
  streamReady: boolean;
  /** Every engine counter by its engine name. */
  counters: ReadonlyMap<string, bigint>;
}

export interface AssessmentView {
  verdict: string;
  coverageComplete: boolean;
  decodeComplete: boolean;
  incompleteReasons: readonly string[];
  gapRecords: bigint;
  distinctReceivers: bigint;
  grossEdgeTotalUsd: number;
  grossEdgeP50Usd: number | null;
  grossEdgeP90Usd: number | null;
  deploymentError: string | null;
}

export interface WriterView {
  sampledUnixMs: bigint;
  queueBytes: bigint;
  queuePeakBytes: bigint;
  admissionLimitBytes: bigint;
  maxBytes: bigint;
  freeBudgetBytes: bigint;
  ingressBytesPerS: number;
  writeBytesPerS: number;
  lagFrames: bigint;
  budgetExhaustionS: number | null;
  diskFreeBytes: bigint | null;
  diskReserveBytes: bigint;
  diskExhaustionS: number | null;
}

/** The process provider budget every RPC of the capture runs through (F-01/C07). */
export interface ProviderView {
  sampledUnixMs: bigint;
  scope: string;
  requests: bigint;
  http429Count: bigint;
  inFlight: bigint;
  queued: bigint;
  maxRps: bigint;
  effectiveRps: number;
  maxConcurrency: bigint;
  burst: bigint;
  bytes: bigint;
  /** NONE | LOCAL_BACKOFF | SERVER_RETRY_AFTER | RETRY_AFTER_UNREPRESENTABLE. */
  cooldownCause: string;
  /** Null when the server delay is unrepresentable (never 0). */
  cooldownRemainingMs: bigint | null;
  /** Null while the budget admits. */
  haltCode: string | null;
  /** Local counters never qualify a provider quota. */
  quotaQualified: boolean;
}

export interface CaptureView {
  run: RunView | null;
  phase: PhaseView | null;
  transport: TransportView | null;
  market: MarketView | null;
  assessment: AssessmentView | null;
  writer: WriterView | null;
  provider: ProviderView | null;
}

/** One operational parameter as the engine's registry states it (DEC-05: value, source, default,
 *  range, change rule and the open measurement question that keeps it provisional). */
export interface ParameterView {
  value: bigint;
  unit: string;
  source: string;
  defaultValue: bigint;
  minimum: bigint;
  maximum: bigint;
  /** ENTSCHEIDUNGEN §3 question (e.g. B-2); null for a value no measurement question covers. */
  provisional: string | null;
  changeRule: string;
}

export interface SystemView {
  /** false: the process runs on built-in defaults (no effective record). */
  activated: boolean;
  profile: string | null;
  /** Origin of the profile: mode:<mode> when the operating mode selected it (DEC-05). */
  profileSource: string | null;
  parameterCount: number;
  /** Every operational parameter by field — the engine's one authority. */
  parameters: ReadonlyMap<string, ParameterView>;
  /** The owner's typed settings with their sources (DEC-05); empty without an effective record. */
  settings: ReadonlyMap<string, { value: string; source: string }>;
  liveAuthorized: boolean | null;
  submissionAuthorized: boolean | null;
}

/** Engine counter names the GUI reads (MarketStats::counters). The contract test checks each
 *  against the engine's golden frame, so an engine rename fails the build instead of showing "—". */
export const USED_COUNTERS = [
  "unique_transactions",
  "successful_transactions",
  "failed_transactions",
  "unknown_status_transactions",
  "liquidation_events",
  "gap_count",
  "reconnect_count",
  "transport_reconnect_attempts",
  "transport_recovered",
  "transport_http_429",
  "last_observed_slot",
] as const;

export type CounterName = (typeof USED_COUNTERS)[number];
