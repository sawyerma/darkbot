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

export interface SystemView {
  /** false: the process runs on built-in defaults (no effective record). */
  activated: boolean;
  profile: string | null;
  parameterCount: number;
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
