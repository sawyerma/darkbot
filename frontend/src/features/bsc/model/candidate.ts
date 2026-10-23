// G0.7 self-launch candidate A/B (BSC plan v8/v9). The native
// `g07_candidate` store is the only authority. No gateway action and no
// status field carry its selection yet, so the selector is shown disabled
// and nothing here pretends a switch succeeded or knows candidate values.

export type CandidateProfile = "SELF_A" | "SELF_B";

export const BACKEND_UNAVAILABLE = "BACKEND_UNAVAILABLE";

export const MISSING_INTEGRATION =
  "Missing: gateway action bsc.candidate.select (typed params, generation CAS), its native call into g07_candidate, and selection/candidate fields in bsc/v1/status.proto projected by the status reporter.";

export interface CandidateOption {
  profile: CandidateProfile;
  label: string;
  enabled: boolean;
}

export interface CandidateControlView {
  available: boolean;
  reason: string;
  options: CandidateOption[];
  rows: { label: string; value: string }[];
  readiness: { profile: CandidateProfile; value: string }[];
}

export interface PlannedControl {
  id: string;
  label: string;
  state: string;
}

/** The candidate group as long as no native backend answers for it. */
export function candidateControlView(guardState: string | undefined): CandidateControlView {
  return {
    available: false,
    reason: BACKEND_UNAVAILABLE,
    options: [
      { profile: "SELF_A", label: "A – Frozen Salt", enabled: false },
      { profile: "SELF_B", label: "B – Fresh Random Salt", enabled: false },
    ],
    rows: [
      { label: "Selected profile", value: BACKEND_UNAVAILABLE },
      { label: "Candidate ID", value: BACKEND_UNAVAILABLE },
      { label: "Predicted token", value: BACKEND_UNAVAILABLE },
      { label: "Portal / deployment", value: BACKEND_UNAVAILABLE },
      { label: "DRY / evidence binding", value: BACKEND_UNAVAILABLE },
      { label: "Qualification", value: "UNKNOWN" },
      { label: "Compiled guard", value: guardState || "UNKNOWN" },
    ],
    readiness: [
      { profile: "SELF_A", value: BACKEND_UNAVAILABLE },
      { profile: "SELF_B", value: BACKEND_UNAVAILABLE },
    ],
  };
}

/** Later scopes and sources: shown with their real state, never switchable. */
export const PLANNED_SCOPES: PlannedControl[] = [
  { id: "FOREIGN_AUTO", label: "Auto: fremde Launches", state: "PLANNED · not implemented" },
];

export const PLANNED_SOURCES: PlannedControl[] = [
  "BSC_INGRESS_GETH",
  "PUBLICNODE",
  "DRPC",
  "BLOCKREQ",
  "48CLUB",
].map(id => ({ id, label: id, state: "PLANNED · OFF/SHADOW/LIVE not implemented" }));
