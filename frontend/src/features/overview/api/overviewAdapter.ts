import { fromBinary } from "@bufbuild/protobuf";
import {
  FinanceQuality,
  OverviewAggregatesSchema,
  Readiness,
  SourceState,
  type FinanceSummary,
  type OverviewAggregates,
  type ProjectSummary,
} from "@/core/generated/darkbot/control/v1/overview_pb";
import { ProjectId, TopicId, topics, type StateMeta } from "@/core/realtime";
import type { FinanceView, OverviewView, ProjectReadSummary } from "@/core/projects/readSummary";
import { useOverviewStore } from "../overviewStore";

// Einzige Brücke Transport -> Overview-Store und einziger Ort mit generierten Overview-Typen.

function sourceOf(s: SourceState): ProjectReadSummary["source"] {
  switch (s) {
    case SourceState.NOT_CONNECTED: return "NOT_CONNECTED";
    case SourceState.CONNECTED: return "CONNECTED";
    case SourceState.STALE: return "STALE";
    case SourceState.DETACHED: return "DETACHED";
    default: return "UNKNOWN";
  }
}

function readinessOf(r: Readiness): ProjectReadSummary["readiness"] {
  switch (r) {
    case Readiness.READY: return "READY";
    case Readiness.BLOCKED: return "BLOCKED";
    default: return "UNKNOWN";
  }
}

const SIGNED = /^(?:0|[1-9][0-9]*|-[1-9][0-9]*)$/;
const MIN_I128 = -(1n << 127n), MAX_I128 = (1n << 127n) - 1n;
function checkedMicro(s: string | undefined): string | null {
  if (s === undefined) return null;
  if (s.length > 40 || !SIGNED.test(s)) throw Error("INVALID_SIGNED_DECIMAL");
  const v = BigInt(s);
  if (v < MIN_I128 || v > MAX_I128) throw Error("SIGNED_I128_RANGE");
  return s;
}
function financeOf(f: FinanceSummary | undefined): FinanceView | null {
  if (!f) return null;
  const quality = f.quality === FinanceQuality.FINALIZED ? "FINALIZED" : f.quality === FinanceQuality.PARTIAL ? "PARTIAL" : "UNKNOWN";
  const final = quality === "FINALIZED" && !!f.financeSnapshotId && f.asOfUnixNs > 0n;
  return {
    snapshotId: f.financeSnapshotId || null,
    quality: final ? "FINALIZED" : quality === "FINALIZED" ? "UNKNOWN" : quality,
    realizedNetPnlUsdMicro: final ? checkedMicro(f.realizedNetPnlUsdMicroSigned?.value) : null,
    totalCapitalUsdMicro: final ? f.totalCapitalUsdMicro ?? null : null,
    asOfUnixNs: f.asOfUnixNs > 0n ? f.asOfUnixNs : null,
  };
}

function projectOf(p: ProjectSummary): ProjectReadSummary {
  const connected = p.source === SourceState.CONNECTED && !!p.sourceId;
  return {
    project: p.project,
    label: p.label,
    domain: p.domain,
    source: connected ? "CONNECTED" : p.source === SourceState.CONNECTED ? "UNKNOWN" : sourceOf(p.source),
    readiness: readinessOf(p.readiness),
    liveAuthorized: connected ? p.liveAuthorized ?? null : null,
    activeBots: connected ? p.activeBots ?? null : null,
    activeJobs: connected ? p.activeJobs ?? null : null,
    activeCaptures: connected ? p.activeCaptures ?? null : null,
    warnings: [...p.warnings],
    rpcLatency: connected && p.rpcLatency && p.rpcLatency.samples > 0
      ? { p95Ms: p.rpcLatency.p95Ms ?? null, windowS: p.rpcLatency.windowS, samples: p.rpcLatency.samples }
      : null,
    finance: connected ? financeOf(p.finance) : null,
    sourceTimestampUnixNs: p.sourceTimestampUnixNs !== 0n ? p.sourceTimestampUnixNs : null,
    sourceId: p.sourceId,
  };
}

export function toOverviewView(a: OverviewAggregates): OverviewView {
  const allowed = new Set([ProjectId.PROJECT_0, ProjectId.BSC, ProjectId.SOLANA]);
  const seen = new Set<ProjectId>();
  const projects = a.projects.map(p => {
    if (!allowed.has(p.project) || seen.has(p.project)) throw Error("INVALID_PROJECT_ID");
    seen.add(p.project);
    return projectOf(p);
  });
  const g = a.global;
  const attached = projects.length > 0 && projects.every(p => p.source === "CONNECTED");
  const available = !!g?.available && attached && g.netPnlUsdMicroSigned !== undefined
    && g.totalCapitalUsdMicro !== undefined;
  return {
    projects,
    global: {
      available,
      reason: available ? "" : !attached ? "sources not all attached" :
        (g?.unavailableReason || "no compatible finalized valuation"),
      netPnlUsdMicro: available ? checkedMicro(g!.netPnlUsdMicroSigned?.value) : null,
      totalCapitalUsdMicro: available ? g!.totalCapitalUsdMicro ?? null : null,
    },
    projectsConnected: projects.filter(p => p.source === "CONNECTED").length,
    computedAtUnixNs: a.computedAtUnixNs > 0n ? a.computedAtUnixNs : null,
  };
}

let started = false;

export function ensureOverviewAdapter(): void {
  if (started) return;
  started = true;

  topics.onTopicStatus(TopicId.OVERVIEW_AGGREGATES, (status, reason) =>
    useOverviewStore.getState().setStatus(status, reason),
  );
  topics.onState(TopicId.OVERVIEW_AGGREGATES, (bytes: Uint8Array, meta: StateMeta) => {
    // Fail closed: any decode/validation error MUST propagate to TopicSubscriptions.
    // Otherwise its StateFull seq would be acknowledged as applied despite a bad payload.
    const decoded = fromBinary(OverviewAggregatesSchema, bytes);
    useOverviewStore.getState().apply(toOverviewView(decoded), meta);
  });
}
