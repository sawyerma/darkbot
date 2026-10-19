import { describe, expect, it } from "vitest";
import type { OverviewView, ProjectReadSummary } from "@/core/projects/readSummary";
import { ProjectId } from "@/core/realtime";
import { financeRows, pnlCoverage, systemRows, worstRpcP95 } from "./overviewRows";

function project(id: ProjectId, label: string, over: Partial<ProjectReadSummary> = {}): ProjectReadSummary {
  return {
    project: id, label, domain: "", source: "CONNECTED", readiness: "UNKNOWN", liveAuthorized: null,
    activeBots: null, activeJobs: null, activeCaptures: null, warnings: [], rpcLatency: null,
    finance: null, sourceTimestampUnixNs: null, sourceId: "source", ...over,
  };
}

function view(projects: ProjectReadSummary[], computedAtNs: bigint | null = null): OverviewView {
  return {
    projects,
    projectsConnected: projects.filter((p) => p.source === "CONNECTED").length,
    computedAtUnixNs: computedAtNs,
    global: { available: false, reason: "not qualified", netPnlUsdMicro: null, totalCapitalUsdMicro: null },
  };
}

const NOW = 1_800_000_000_000;
const sys = (status: string, v: OverviewView | null) => systemRows({
  gatewayStatus: "OPEN", gatewayReason: "", topicStatus: status, topicReason: "",
  view: v, projectCount: 3, nowMs: NOW,
});
const fin = (status: string, v: OverviewView | null) => financeRows({ topicStatus: status, view: v, projectCount: 3 });
const values = (rows: { label: string; value: string }[]) => Object.fromEntries(rows.map((r) => [r.label, r.value]));

const finalized = {
  snapshotId: "confirmed-1", quality: "FINALIZED" as const,
  realizedNetPnlUsdMicro: "1500000", totalCapitalUsdMicro: 2_000_000n,
  asOfUnixNs: 1n,
};

describe("DarkBot overview cards", () => {
  it("keeps the agreed labels and ordering", () => {
    expect(sys("IDLE", null).map(r => r.label)).toEqual([
      "Gateway", "Overview Feed", "Connected Projects", "Gateway RTT", "RPC Latency P95", "Last Overview Update",
    ]);
    expect(fin("IDLE", null).map(r => r.label)).toEqual([
      "Realized Net PnL", "Unrealized PnL", "Total Equity", "Daily Net PnL", "PnL Coverage", "Reporting Currency",
    ]);
  });

  it("never invents unreported latency or finance", () => {
    expect(values(sys("IDLE", null))).toMatchObject({
      "Connected Projects": "—", "Gateway RTT": "—", "RPC Latency P95": "—", "Last Overview Update": "—",
    });
    expect(values(fin("IDLE", null))).toEqual({
      "Realized Net PnL": "— USD", "Unrealized PnL": "— USD", "Total Equity": "— USD",
      "Daily Net PnL": "— USD", "PnL Coverage": "—", "Reporting Currency": "USD",
    });
  });

  it("counts only finalized realized-PnL snapshots with explicit provenance", () => {
    const v = view([
      project(ProjectId.PROJECT_0, "Project 0", { finance: finalized }),
      project(ProjectId.BSC, "BSC", { source: "DETACHED", finance: finalized }),
      project(ProjectId.SOLANA, "Solana", { finance: { ...finalized, realizedNetPnlUsdMicro: null } }),
    ]);
    expect(pnlCoverage(v.projects)).toBe(1);
    expect(values(fin("LIVE", v))["PnL Coverage"]).toBe("1 / 3");
    expect(values(fin("STALE", v))["PnL Coverage"]).toBe("—");
  });

  it("does not silently rename Total Capital to Total Equity or Net PnL to Realized Net PnL", () => {
    const v: OverviewView = { ...view([project(ProjectId.PROJECT_0, "Project 0", { finance: finalized })]),
      global: { available: true, reason: "", netPnlUsdMicro: "1500000", totalCapitalUsdMicro: 2_000_000n } };
    expect(values(fin("LIVE", v))).toMatchObject({
      "Total Equity": "— USD", "Realized Net PnL": "— USD", "Daily Net PnL": "— USD",
    });
  });

  it("labels the worst source p95, never an average or an invented aggregate", () => {
    const v = view([
      project(ProjectId.PROJECT_0, "Project 0", { rpcLatency: { p95Ms: 40, windowS: 60, samples: 100 } }),
      project(ProjectId.BSC, "BSC", { rpcLatency: { p95Ms: 55, windowS: 60, samples: 80 } }),
      project(ProjectId.SOLANA, "Solana", { source: "DETACHED", rpcLatency: { p95Ms: 99, windowS: 60, samples: 100 } }),
    ]);
    expect(worstRpcP95(v.projects)).toEqual({ p95Ms: 55, project: "BSC", sources: 2 });
    expect(values(sys("LIVE", v))["RPC Latency P95"]).toBe("55 ms · BSC (worst of 2)");
  });

  it("computes connected count and timestamp from the supplied overview", () => {
    const v = view([project(ProjectId.PROJECT_0, "Project 0")], BigInt(NOW - 3_000) * 1_000_000n);
    expect(values(sys("LIVE", v))).toMatchObject({
      "Connected Projects": "1 / 3", "Last Overview Update": "3s ago",
    });
  });
});
