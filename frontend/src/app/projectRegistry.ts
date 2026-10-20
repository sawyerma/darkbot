import { lazy, type ComponentType, type LazyExoticComponent } from "react";
import { ProjectId, TopicId } from "@/core/realtime";

// Statisch typisierte Projektliste (Phasenplan §3/§7.1). Ein neues Projekt = ein Eintrag hier
// plus ein Feature-Modul. Kein Laufzeit-Plugin, kein dynamischer Import von außen.
// Ein Registry-Eintrag erzeugt KEINE Berechtigung: capabilities beschreiben nur die UI.

export type ProjectKey = "overview" | "project0" | "bsc" | "solana";

export type Project0Panel = "overview" | "liquidator" | "capture" | "analytics" | "wallets";
export type BscPanel = "overview" | "launch" | "monitoring" | "analytics" | "wallets";

export interface ProjectRegistration<P extends string = string> {
  readonly key: ProjectKey;
  readonly projectId: ProjectId;
  readonly route: string;
  readonly label: string;
  readonly navOrder: number;
  readonly page: LazyExoticComponent<ComponentType>;
  readonly topics: {
    // Für die ganze Route, solange sie gemountet ist (z. B. *.status mit pending_commands[]).
    readonly base: readonly TopicId[];
    // Zusätzlich pro sichtbarem Panel; die Panel-Komponente hält sie per useTopic().
    readonly panels: Readonly<Record<P, readonly TopicId[]>>;
  };
  readonly capabilities: {
    readonly readStatus: boolean;
    // Project 0 supports read-only capture lifecycle commands only.
    readonly commands: boolean;
  };
}

const overview: ProjectRegistration<never> = {
  key: "overview",
  projectId: ProjectId.UNSPECIFIED,
  route: "/",
  label: "Overview",
  navOrder: 0,
  page: lazy(() => import("@/features/overview/OverviewPage")),
  topics: { base: [TopicId.OVERVIEW_AGGREGATES], panels: {} },
  capabilities: { readStatus: true, commands: false },
};

const project0: ProjectRegistration<Project0Panel> = {
  key: "project0",
  projectId: ProjectId.PROJECT_0,
  route: "/project-0",
  label: "Project 0",
  navOrder: 1,
  page: lazy(() => import("@/features/project0/Project0Page")),
  topics: {
    base: [TopicId.PROJECT0_STATUS, TopicId.PROJECT0_CAPTURE_STATE],
    panels: {
      overview: [TopicId.PROJECT0_FINANCE_SUMMARY],
      liquidator: [TopicId.PROJECT0_LIQUIDATOR_STATE],
      capture: [TopicId.PROJECT0_CAPTURE_STATE],
      analytics: [TopicId.PROJECT0_ANALYTICS_JOBS],
      wallets: [TopicId.PROJECT0_WALLETS_STATE],
      // EVENT-Topics (execution.events, capture.events) hält das Panel explizit per
      // useTopic(..., { startPolicy }) – nie automatisch über die Route.
    },
  },
  capabilities: { readStatus: true, commands: true },
};

const bsc: ProjectRegistration<BscPanel> = {
  key: "bsc",
  projectId: ProjectId.BSC,
  route: "/bsc",
  label: "BSC",
  navOrder: 2,
  page: lazy(() => import("@/features/bsc/BscPage")),
  topics: {
    base: [TopicId.BSC_STATUS],
    panels: {
      overview: [],
      launch: [TopicId.BSC_LAUNCH_STATE],
      monitoring: [TopicId.BSC_MONITORING_STATE],
      analytics: [],
      wallets: [TopicId.BSC_WALLETS_STATE],
    },
  },
  capabilities: { readStatus: true, commands: true },
};

const solana: ProjectRegistration<never> = {
  key: "solana",
  projectId: ProjectId.SOLANA,
  route: "/solana",
  label: "Solana",
  navOrder: 3,
  page: lazy(() => import("@/features/solana/SolanaPage")),
  // solana.status erst, wenn ein qualifizierter Adapter existiert (Wire V1.3 §6).
  topics: { base: [], panels: {} as Record<never, never> },
  capabilities: { readStatus: false, commands: false },
};

export const PROJECT_REGISTRY = [overview, project0, bsc, solana] as const;

export const NAV_ENTRIES = [...PROJECT_REGISTRY]
  .sort((a, b) => a.navOrder - b.navOrder)
  .map((p) => ({ to: p.route, label: p.label, end: p.route === "/" }));

export function registrationByKey(key: ProjectKey): (typeof PROJECT_REGISTRY)[number] {
  const r = PROJECT_REGISTRY.find((p) => p.key === key);
  if (!r) throw new Error(`unknown project key ${key}`);
  return r;
}

export function registrationByPath(pathname: string): (typeof PROJECT_REGISTRY)[number] | undefined {
  if (pathname === "/") return overview;
  return PROJECT_REGISTRY.find((p) => p.route !== "/" && (pathname === p.route || pathname.startsWith(`${p.route}/`)));
}

export const project0Registration = project0;
export const bscRegistration = bsc;
