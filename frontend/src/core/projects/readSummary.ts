import type { ProjectId } from "@/core/realtime";

// Gemeinsame Read-only-Zusammenfassung pro Projekt. Einzige Typen, die projektübergreifend
// geteilt werden (ESLint-Boundary Regel 4). Keine Fachlogik, keine Wallet-Details.

export type SourceStateView = "NOT_CONNECTED" | "CONNECTED" | "STALE" | "DETACHED" | "UNKNOWN";
export type ReadinessView = "UNKNOWN" | "BLOCKED" | "READY";
export type FinanceQualityView = "UNKNOWN" | "PARTIAL" | "FINALIZED";

export interface LatencyView {
  p95Ms: number | null;
  windowS: number;
  samples: number;
}

export interface FinanceView {
  snapshotId: string | null;
  quality: FinanceQualityView;
  realizedNetPnlUsdMicro: string | null; // signierter Decimal-String
  totalCapitalUsdMicro: bigint | null;
  asOfUnixNs: bigint | null;
}

export interface ProjectReadSummary {
  project: ProjectId;
  label: string;
  domain: string;
  source: SourceStateView;
  readiness: ReadinessView;
  liveAuthorized: boolean | null;
  activeBots: number | null;
  activeJobs: number | null;
  activeCaptures: number | null;
  warnings: string[];
  rpcLatency: LatencyView | null;
  finance: FinanceView | null;
  sourceTimestampUnixNs: bigint | null;
  sourceId: string;
}

export interface GlobalFinanceView {
  available: boolean;
  reason: string;
  netPnlUsdMicro: string | null;
  totalCapitalUsdMicro: bigint | null;
}

export interface OverviewView {
  projects: ProjectReadSummary[];
  global: GlobalFinanceView;
  projectsConnected: number;
  computedAtUnixNs: bigint | null;
}
