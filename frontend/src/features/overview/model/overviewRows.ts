import type { OverviewView, ProjectReadSummary } from "@/core/projects/readSummary";
import { DASH } from "@/shared/ui/graphite/Rows";
import { formatAge, formatUsdMicroSigned } from "./format";

// Presentation only. Do not calculate PnL across projects in the browser.
// Missing measurements are unknown (—), not zero.
export interface OverviewRow {
  label: string;
  value: string;
  note?: string;
}

export interface WorstRpcP95 {
  p95Ms: number;
  project: string;
  sources: number;
}

/** A maximum of independently reported p95s, NOT a global RPC p95. */
export function worstRpcP95(projects: readonly ProjectReadSummary[]): WorstRpcP95 | null {
  let worst: WorstRpcP95 | null = null;
  let sources = 0;
  for (const project of projects) {
    const latency = project.rpcLatency;
    if (project.source !== "CONNECTED" || !latency || latency.p95Ms === null ||
        !Number.isSafeInteger(latency.p95Ms) || latency.p95Ms < 0 || latency.samples <= 0) continue;
    sources += 1;
    if (!worst || latency.p95Ms > worst.p95Ms) {
      worst = { p95Ms: latency.p95Ms, project: project.label, sources: 0 };
    }
  }
  return worst ? { ...worst, sources } : null;
}

/** Requires a finalized, identified, timestamped realized-PnL snapshot. */
export function pnlCoverage(projects: readonly ProjectReadSummary[]): number {
  return projects.filter((project) => {
    const finance = project.finance;
    return project.source === "CONNECTED" && finance?.quality === "FINALIZED" &&
      !!finance.snapshotId && finance.asOfUnixNs !== null && finance.asOfUnixNs > 0n &&
      finance.realizedNetPnlUsdMicro !== null;
  }).length;
}

interface SystemInput {
  gatewayStatus: string;
  gatewayReason: string;
  topicStatus: string;
  topicReason: string;
  view: OverviewView | null;
  projectCount: number;
  nowMs: number;
}

export function systemRows(input: SystemInput): OverviewRow[] {
  const current = input.topicStatus === "LIVE" ? input.view : null;
  const worst = current ? worstRpcP95(current.projects) : null;
  return [
    { label: "Gateway", value: input.gatewayStatus,
      note: input.gatewayStatus !== "OPEN" ? input.gatewayReason || undefined : undefined },
    { label: "Overview Feed", value: input.topicStatus,
      note: input.topicStatus !== "LIVE" ? input.topicReason || undefined : undefined },
    { label: "Connected Projects", value: current ? `${current.projectsConnected} / ${input.projectCount}` : DASH },
    // No browser↔gateway RTT measurement is published by the current wire protocol.
    { label: "Gateway RTT", value: DASH },
    // An explicitly labeled worst-of-projects metric avoids inventing an aggregate percentile.
    { label: "RPC Latency P95", value: worst
      ? `${worst.p95Ms} ms · ${worst.project}${worst.sources > 1 ? ` (worst of ${worst.sources})` : ""}` : DASH },
    // This is the age of the overview calculation, NOT the age of Solana market data.
    { label: "Last Overview Update", value: formatAge(input.nowMs, input.view?.computedAtUnixNs ?? null) },
  ];
}

interface FinanceInput {
  topicStatus: string;
  view: OverviewView | null;
  projectCount: number;
}

export function financeRows(input: FinanceInput): OverviewRow[] {
  const current = input.topicStatus === "LIVE" ? input.view : null;
  // The existing GlobalFinance.net_pnl field is not explicitly REALIZED. The current
  // total_capital field is not necessarily EQUITY. Neither may be relabeled as such.
  // Await qualified canonical backend projections for these four financial metrics.
  return [
    { label: "Realized Net PnL", value: formatUsdMicroSigned(null) },
    { label: "Unrealized PnL", value: formatUsdMicroSigned(null) },
    { label: "Total Equity", value: formatUsdMicroSigned(null) },
    { label: "Daily Net PnL", value: formatUsdMicroSigned(null) },
    { label: "PnL Coverage", value: current ? `${pnlCoverage(current.projects)} / ${input.projectCount}` : DASH },
    { label: "Reporting Currency", value: "USD" },
  ];
}
