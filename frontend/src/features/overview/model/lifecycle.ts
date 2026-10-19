import type { ProjectKey } from "@/app/projectRegistry";

// F7-OVR §10.2/§10.3. Process lifecycle is NOT feed connection, readiness or LIVE permission.

export type ProcessState = "RUNNING" | "STOPPED" | "STARTING" | "STOPPING" | "FAILED" | "UNKNOWN";

/** Future input from a supervisor-owned, authenticated, timestamped lifecycle source (F7-OVR-B).
 *  The wire contract has no such source yet, so every caller passes null today. */
export interface ProcessStatusSource {
  state: ProcessState;
  runId: string;
}

export interface ProcessIndicator {
  state: ProcessState;
  label: string;
  reason: string;
}

/** Missing, stale or untrusted status is UNKNOWN and never inferred as stopped or running.
 *  Deliberately takes no feed/readiness input: a connected feed is not process evidence. */
export function processIndicator(source: ProcessStatusSource | null, sourceTopicStatus = "IDLE"): ProcessIndicator {
  if (!source) {
    return { state: "UNKNOWN", label: "Process · UNKNOWN", reason: "No supervisor-owned lifecycle source is exposed (F7-OVR-B)" };
  }
  if (sourceTopicStatus !== "LIVE" || !source.runId) {
    return { state: "UNKNOWN", label: "Process · UNKNOWN", reason: "Lifecycle source not live or unidentified" };
  }
  return { state: source.state, label: `Process · ${source.state}`, reason: `Run ${source.runId}` };
}

/** Only disabled controls exist until the F6 command channel and a qualified owner do. The literal
 *  `false` turns an enabled control into a type error instead of a decorative clickable button. */
export interface DisabledControl {
  readonly enabled: false;
  readonly reason: string;
}

const P0_CONTROLLER = "Lifecycle controller not yet qualified (requires F6 commands, F7-OVR-C)";
const NO_OWNER = "No qualified process owner exposes a controller (F7-OVR-B)";
const LOG_VIEWER = "Authorized, bounded log viewer not yet qualified (F7-OVR-D)";

export function projectControls(key: ProjectKey): { startStop: DisabledControl; logs: DisabledControl } {
  return {
    startStop: { enabled: false, reason: key === "project0" ? P0_CONTROLLER : NO_OWNER },
    logs: { enabled: false, reason: LOG_VIEWER },
  };
}

/** Exactly two global actions. There is no "Start Selected". */
export const GLOBAL_CONTROLS = {
  stopAll: { enabled: false, reason: "Qualified global stop contract pending (F7-OVR-D)" },
  showAllLogs: { enabled: false, reason: "Qualified log viewer pending (F7-OVR-D)" },
} as const satisfies Record<string, DisabledControl>;
