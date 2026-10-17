/**
 * Project_0 V0: presentation contracts only.
 * These are not evidence that a corresponding Rust/HTTP endpoint already exists.
 * Rust owns executable policy, balances, fees, price and status calculations.
 */
export type Project0Tab = "overview" | "liquidator" | "capture" | "analytics" | "wallets";
export type StrategyKind = "CLASSIC" | "RECEIVERSHIP";
export type CaptureMode = "market" | "econ4h" | "shadow";
export type CaptureSource = "logs" | "transactions";
export type VerificationMode = "off" | "index" | "strict" | "crosscheck";
export type RunHorizon =
  | { kind: "fixed"; value: string; unit: "s" | "m" | "h" | "d" }
  | { kind: "until_stopped" };

/** Present in project_0/operations/cli/measurement_contract.py::MARKERS.
 * Only report visibility: never filter source ingestion or execution selection.
 */
export const REPORT_MARKERS = [
  { id: "lending_account_liquidate", label: "Classic · liquidate" },
  { id: "start_liquidation", label: "Receivership · start" },
  { id: "end_liquidation", label: "Receivership · end" },
  { id: "start_deleverage", label: "Deleverage · start" },
  { id: "end_deleverage", label: "Deleverage · end" },
  { id: "lending_pool_handle_bankruptcy", label: "Bankruptcy · settlement" },
] as const;
export type ReportMarker = (typeof REPORT_MARKERS)[number]["id"];

export type DraftOrigin = "manual" | "analytics_unverified";
export interface LiquidatorDraft {
  /** UI-local identity, NOT a wallet, runtime-owner or execution job ID. */
  id: string;
  name: string;
  origin: DraftOrigin;
  enabledStrategies: StrategyKind[];
  capitalUsd: string;
  minimumNetUsd: string;
  maximumFeeUsd: string;
  maximumInFlight: string;
}
export interface FinanceSnapshot {
  snapshotId: string;
  asOfSlot: string;
  totalCapitalUsdMicro: string | null;
  availableCapitalUsdMicro: string | null;
  reservedCapitalUsdMicro: string | null;
  realizedNetPnlUsdMicro: string | null;
  withdrawableProfitUsdMicro: string | null;
  transferredToTreasuryUsdMicro: string | null;
  status: "VERIFIED" | "PARTIAL" | "UNKNOWN";
}
/** No backend endpoint publishes FinanceSnapshot as of V0. */
export interface MonitorSnapshot {
  rpcP95Ms: number | null;
  decisionP95Ms: number | null;
  streamAgeMs: number | null;
  finance: FinanceSnapshot | null;
}
export const UNCONNECTED_MONITOR: MonitorSnapshot = {
  rpcP95Ms: null,
  decisionP95Ms: null,
  streamAgeMs: null,
  finance: null,
};
