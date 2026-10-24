import { create } from "zustand";
import type { StateMeta, TopicStatus } from "@/core/realtime";
import type { OverviewView, ProjectReadSummary } from "@/core/projects/readSummary";
import type { ProjectId } from "@/core/realtime";

export interface OverviewStore {
  view: OverviewView | null;
  meta: StateMeta | null;
  status: TopicStatus;
  reason: string;
  apply: (view: OverviewView, meta: StateMeta) => void;
  setStatus: (status: TopicStatus, reason: string) => void;
}

export const useOverviewStore = create<OverviewStore>()((set) => ({
  view: null,
  meta: null,
  status: "IDLE",
  reason: "",
  apply: (view, meta) => set(prev => ({
    view: { ...view, projects: view.projects.map(p => {
      const old = prev.view?.projects.find(q => q.project === p.project);
      return old && sameProject(old, p) ? old : p;
    }) },
    meta,
  })),
  setStatus: (status, reason) => set({ status, reason }),
}));

export const selectStatus = (s: OverviewStore) => s.status;
export const selectReason = (s: OverviewStore) => s.reason;
export const selectView = (s: OverviewStore) => s.view;
export const selectGlobal = (s: OverviewStore) => s.view?.global ?? null;
export const selectConnected = (s: OverviewStore) => s.view?.projectsConnected ?? null;
export const selectComputedAt = (s: OverviewStore) => s.view?.computedAtUnixNs ?? null;

// Pro Karte ein eigener Selektor: nur die betroffene Karte rendert neu.
export const selectProject =
  (project: ProjectId) =>
  (s: OverviewStore): ProjectReadSummary | null =>
    s.view?.projects.find((p) => p.project === project) ?? null;

function sameProject(a: ProjectReadSummary, b: ProjectReadSummary): boolean {
  if (a.project !== b.project || a.label !== b.label || a.domain !== b.domain ||
      a.source !== b.source || a.readiness !== b.readiness || a.liveAuthorized !== b.liveAuthorized ||
      a.activeBots !== b.activeBots || a.activeJobs !== b.activeJobs ||
      a.activeCaptures !== b.activeCaptures || a.sourceId !== b.sourceId ||
      a.sourceTimestampUnixNs !== b.sourceTimestampUnixNs ||
      a.warnings.length !== b.warnings.length ||
      a.warnings.some((v,i) => v !== b.warnings[i]) ||
      a.jobs.length !== b.jobs.length ||
      a.jobs.some((j,i) => { const k = b.jobs[i]; return !k || j.job !== k.job || j.state !== k.state || j.launchId !== k.launchId ||
        j.resultCode !== k.resultCode || j.controllerAvailable !== k.controllerAvailable; })) return false;
  const al=a.rpcLatency, bl=b.rpcLatency;
  if (!!al !== !!bl || (al && bl && (al.p95Ms !== bl.p95Ms || al.samples !== bl.samples || al.windowS !== bl.windowS))) return false;
  const af=a.finance, bf=b.finance;
  return !!af === !!bf && (!af || !bf || (
    af.snapshotId === bf.snapshotId && af.quality === bf.quality &&
    af.realizedNetPnlUsdMicro === bf.realizedNetPnlUsdMicro &&
    af.totalCapitalUsdMicro === bf.totalCapitalUsdMicro && af.asOfUnixNs === bf.asOfUnixNs
  ));
}
