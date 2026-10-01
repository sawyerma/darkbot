export type ProjectId = "bsc" | "project_0" | "solana";

export type RuntimeMode = "OFFLINE" | "SHADOW" | "LIVE" | "HALT";

export type ProjectStatus = {
  id: ProjectId;
  label: string;
  mode: RuntimeMode;
  ready: boolean;
  detail?: string;
};
