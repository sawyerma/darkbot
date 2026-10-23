// G0.7 self-launch candidate A/B (BSC plan v8/v9/v9.1). The native
// `g07_candidate` store is the only authority; the status reporter projects
// its public mirror as `candidate.*` launch runtime fields. Without them the
// selector stays disabled (BACKEND_UNAVAILABLE). A request only becomes
// effective after the native apply at the next desktop sync.
import type { PublicField } from "./types";

export type CandidateProfile = "SELF_A" | "SELF_B";

export const BACKEND_UNAVAILABLE = "BACKEND_UNAVAILABLE";

export const MISSING_INTEGRATION =
  "No public candidate mirror in the runtime snapshot: create candidates with g07_candidate and run darkbotctl bsc sync on the desktop.";

export interface CandidateOption {
  profile: CandidateProfile;
  label: string;
  enabled: boolean;
  why: string;
}

export interface CandidateControlView {
  available: boolean;
  reason: string;
  generation: bigint;
  options: CandidateOption[];
  rows: { label: string; value: string }[];
  readiness: { profile: CandidateProfile; value: string }[];
  request: { label: string; value: string }[];
}

export interface PlannedControl {
  id: string;
  label: string;
  state: string;
}

const LABELS: Record<CandidateProfile, string> = { SELF_A: "A – Frozen Salt", SELF_B: "B – Fresh Random Salt" };
const PROFILES: CandidateProfile[] = ["SELF_A", "SELF_B"];

export interface CandidateInput {
  fields: PublicField[] | undefined;
  guardState: string | undefined;
  statusOk: boolean;
  busy: boolean;
}

/** The candidate group from the native projection; nothing is inferred locally. */
export function candidateControlView({ fields, guardState, statusOk, busy }: CandidateInput): CandidateControlView {
  const f = new Map((fields ?? []).filter(x => x.name.startsWith("candidate.")).map(x => [x.name, x.value]));
  const backend = f.get("candidate.backend");
  const guard = guardState || "UNKNOWN";
  if (backend !== "AVAILABLE") {
    const reason = backend === "INVALID" ? "BACKEND_INVALID" : BACKEND_UNAVAILABLE;
    return {
      available: false, reason, generation: 0n,
      options: PROFILES.map(profile => ({ profile, label: LABELS[profile], enabled: false, why: reason })),
      rows: ["Selected profile", "Candidate ID", "Predicted token", "Portal / deployment", "DRY / evidence binding"].map(label => ({ label, value: reason }))
        .concat([{ label: "Qualification", value: "UNKNOWN" }, { label: "Compiled guard", value: guard }]),
      readiness: PROFILES.map(profile => ({ profile, value: reason })),
      request: [],
    };
  }
  let generation = 0n;
  try { generation = BigInt(f.get("candidate.selection.generation") ?? "x"); } catch { return candidateControlView({ fields: [{ name: "candidate.backend", value: "INVALID" }], guardState, statusOk, busy }); }
  const selected = f.get("candidate.selection.profile") ?? "NONE";
  const requestState = f.get("candidate.request.state") ?? "NONE";
  const of = (p: string, n: string) => f.get(`candidate.${p}.${n}`) ?? "NONE";
  const options = PROFILES.map(profile => {
    const why = !statusOk ? "STATUS_NOT_CURRENT"
      : guard !== "UNARMED" ? "GUARD_NOT_UNARMED"
        : busy ? "REQUEST_IN_FLIGHT"
          : requestState === "PENDING_DESKTOP_SYNC" ? "REQUEST_PENDING_DESKTOP_SYNC"
            : of(profile, "state") !== "PRESENT" ? "CANDIDATE_ABSENT"
              : selected === profile ? "ALREADY_SELECTED" : "";
    return { profile, label: LABELS[profile], enabled: why === "", why };
  });
  const sel = selected === "SELF_A" || selected === "SELF_B" ? selected : "";
  return {
    available: true, reason: "AVAILABLE", generation, options,
    rows: [
      { label: "Selected profile", value: sel ? `${sel} · generation ${generation}` : `NONE · generation ${generation}` },
      { label: "Candidate ID", value: sel ? of(sel, "candidate_id") : "NONE" },
      { label: "Predicted token", value: sel ? of(sel, "token") : "NONE" },
      { label: "Portal / deployment", value: sel ? `${of(sel, "deployment.portal_version")} · ${of(sel, "deployment.portal_implementation")}` : "NONE" },
      { label: "DRY / evidence binding", value: sel ? `${of(sel, "evidence.dry_record_sha256")} · launch ${of(sel, "evidence.launch_tx_hash")}` : "NONE" },
      { label: "Qualification", value: sel ? of(sel, "qualification") : "UNKNOWN" },
      { label: "Compiled guard", value: guard },
      ...PROFILES.map(p => ({ label: `${p} token`, value: of(p, "state") === "PRESENT" ? of(p, "token") : "ABSENT" })),
    ],
    readiness: PROFILES.map(profile => ({ profile, value: of(profile, "state") === "PRESENT" ? of(profile, "readiness") : "ABSENT" })),
    request: requestState === "NONE" ? [{ label: "Last request", value: "NONE" }] : [
      { label: "Last request", value: `${requestState} · ${f.get("candidate.request.profile") ?? "?"} @ generation ${f.get("candidate.request.expected_generation") ?? "?"}` },
      ...(f.has("candidate.request.detail") ? [{ label: "Native decision", value: f.get("candidate.request.detail") ?? "" }] : []),
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
