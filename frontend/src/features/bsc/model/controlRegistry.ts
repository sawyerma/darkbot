// BSC Control Registry v1: frontend display specification only.
// Canonical variables: bsc/docs/1_PLAN/BSC_FRONTEND_BINDINGS_V1.md.
// Not a native configuration, a LIVE permission or an evidence source.
export const CONTROL_CONTRACT_VERSION = "bsc.ui.controls.v1" as const;
export type SourceMode = "OFF" | "SHADOW" | "LIVE";
export type RuntimeMode = "REPLAY" | "PAPER" | "SHADOW" | "LIVE";
export type SourceCapability = "HASH_ONLY" | "FULL_SIGNED_TX" | "QUALIFIED_PRIVATE_TARGET" | "UNKNOWN";
export type SourceHealth = "HEALTHY" | "DEGRADED" | "RATE_LIMITED" | "DISCONNECTED" | "UNQUALIFIED";
export interface PlannedControl {
  readonly id: string;
  readonly label: string;
  readonly phase: string;
  readonly owner: string;
  readonly note: string;
}

// One canonical set of stable UI IDs. Do not make a second planned-control list.
export const CANDIDATE_CONTEXTS = [
  { id: "SELF_A", label: "Eigenlaunch A · Frozen Salt", phase: "G0.7", owner: "g07_candidate", note: "Bereits in Launch / BUY; keine zweite Auswahl erzeugen." },
  { id: "SELF_B", label: "Eigenlaunch B · Fresh Salt", phase: "G0.7", owner: "g07_candidate", note: "Bereits in Launch / BUY; separate Qualifikation erforderlich." },
  { id: "FOREIGN_AUTO", label: "Automatisch fremde Launches", phase: "C1–J", owner: "future native ingress", note: "Nicht an den eigenen A/B-Salt gebunden." },
] as const satisfies readonly PlannedControl[];

export const INGRESS_SOURCES = [
  { id: "BSC_INGRESS_GETH", label: "bsc-geth Fast Lane", phase: "G0.4 / C1–J", owner: "native geth", note: "Binärer P2P-Hook; unabhängige Chain-Sicherheitsdienste." },
  { id: "BSC_INGRESS_PUBLICNODE", label: "PublicNode", phase: "G0.4 / C1–J", owner: "source adapter", note: "Full-TX-Korrektheit und Coverage erst nachweisen." },
  { id: "BSC_INGRESS_DRPC", label: "dRPC", phase: "G0.4 / C1–J", owner: "source adapter", note: "Rate-Limits, Gaps und TX-Identität prüfen." },
  { id: "BSC_INGRESS_BLOCKREQ", label: "BlockReq", phase: "G0.4 / C1–J", owner: "source adapter", note: "Verbindung, Rate-Limits und Coverage prüfen." },
  { id: "BSC_INGRESS_48CLUB", label: "48Club Auction Feed", phase: "G0.4 / C1–J", owner: "private-target adapter", note: "Auction-Target ist nicht automatisch FULL_SIGNED_TX; FREE_ONLY." },
] as const satisfies readonly PlannedControl[];

export const VENUE_CONTROLS = [
  { id: "FLAP", label: "Flap", phase: "G0.1 / D / I–J", owner: "native flap venue", note: "Deployment-/Portal-Version bindend." },
  { id: "FOUR_CLASSIC", label: "Four.meme Classic", phase: "G0.1 / E / I–J", owner: "native classic venue", note: "TokenManager2; eigener ABI-Vertrag." },
  { id: "OPENFOUR", label: "OpenFour", phase: "G0.1 / E / I–J", owner: "native openfour venue", note: "Separater Contract und Decoder." },
] as const satisfies readonly PlannedControl[];

export const INCLUSION_ADAPTERS = [
  { id: "LOCAL_GETH", label: "Lokales geth / P2P", phase: "G0.5 / C3 / I–J", owner: "egress adapter", note: "Lokaler Handoff ist keine Aufnahme." },
  { id: "48CLUB", label: "48Club Builder", phase: "G0.6–G0.9 / I–J", owner: "builder adapter", note: "noMerge / positionFirst / Inclusion separat prüfen." },
  { id: "BLOCKRAZOR", label: "BlockRazor Builder", phase: "G0.6–G0.9 / I–J", owner: "builder adapter", note: "Preis, Revert, Expiry und Inclusion separat prüfen." },
  { id: "OWN_BUILDER", label: "Eigener Builder", phase: "KB optional nach Gate 0", owner: "future builder", note: "Nur nach gemessener Erforderlichkeit." },
] as const satisfies readonly PlannedControl[];

export const SOURCE_MODES = ["OFF", "SHADOW", "LIVE"] as const satisfies readonly SourceMode[];
export const RUNTIME_MODES = ["REPLAY", "PAPER", "SHADOW", "LIVE"] as const satisfies readonly RuntimeMode[];
export const SOURCE_CAPABILITIES = ["HASH_ONLY", "FULL_SIGNED_TX", "QUALIFIED_PRIVATE_TARGET", "UNKNOWN"] as const satisfies readonly SourceCapability[];
export const SOURCE_HEALTH = ["HEALTHY", "DEGRADED", "RATE_LIMITED", "DISCONNECTED", "UNQUALIFIED"] as const satisfies readonly SourceHealth[];

export const FUTURE_OBSERVATIONS = [
  { id: "INGRESS_COVERAGE", label: "Hash / Full-TX / Actionable Coverage", phase: "G0.4 / F", owner: "source census", note: "Head-Lead ist keine Producer-Zeit." },
  { id: "DECISION_TRACE", label: "First actionable / Dedup / Nonce", phase: "C1–H", owner: "native journal", note: "Ein wirtschaftlicher Intent pro Kandidat." },
  { id: "QUOTE_ECONOMICS", label: "Quote / Gebühren / Steuern / EV", phase: "G / H", owner: "native economics", note: "Exakte Integer, Replay-Parität, Kosten nach Fees." },
  { id: "PERFORMANCE", label: "Hotpath- und Adapter-Latenzen", phase: "C3 / I–J", owner: "native measurements", note: "T_send und ursprüngliches Producer-T_buy unterscheiden." },
  { id: "PRODUCER_WITNESS", label: "Original Producer-Timing-Witness", phase: "G0.8 / J", owner: "producer evidence", note: "Ohne Witness T_buy−T0 = UNKNOWN." },
  { id: "RECONCILIATION", label: "G0.8 / Reorg / Kosten", phase: "G0.8 / C2 / J", owner: "native verifier", note: "Acquisition, canonical ordering, Gebühren, UNRESOLVED." },
  { id: "REPLAY_RECOVERY", label: "Replay / Snapshot / Journal", phase: "C0 / F / I", owner: "native storage", note: "Binär, Gaps, Restart, Backpressure." },
] as const satisfies readonly PlannedControl[];

// Reserved future TYPED BINARY control-status projection names; not active
// gateway/protobuf fields or authorization. No UI field is an authority.
export const CONTROL_FIELDS = {
  version: "control.contract.version",
  backend: "control.backend.status",
  generation: "control.config.generation",
  scopeRequested: "control.scope.requested",
  scopeEffective: "control.scope.effective",
  runtimeRequested: "control.runtime.requested",
  runtimeEffective: "control.runtime.effective",
  guard: "control.safety.guard",
  exactApproval: "control.safety.exact_approval",
  nonceIntent: "control.safety.nonce_intent",
  budgetWei: "control.safety.max_total_wei",
  producerWitness: "control.witness.execution_gap",
} as const;

export function ingressFields(id: (typeof INGRESS_SOURCES)[number]["id"]) {
  const base = `control.ingress.${id}`;
  return {
    requested: `${base}.requested`, effective: `${base}.effective`,
    capability: `${base}.capability`, health: `${base}.health`,
    qualified: `${base}.qualified`, firstFullNs: `${base}.first_full_ns`,
    firstActionableNs: `${base}.first_actionable_ns`,
    gaps: `${base}.gaps`, drops: `${base}.drops`,
  } as const;
}
export function venueFields(id: (typeof VENUE_CONTROLS)[number]["id"]) {
  const base = `control.venue.${id}`;
  return {
    requested: `${base}.requested`, effective: `${base}.effective`,
    deployment: `${base}.deployment_sha256`,
    qualified: `${base}.qualified`, health: `${base}.health`,
  } as const;
}
export function inclusionFields(id: (typeof INCLUSION_ADAPTERS)[number]["id"]) {
  const base = `control.inclusion.${id}`;
  return {
    readiness: `${base}.readiness`, qualified: `${base}.qualified`,
    coverage: `${base}.coverage`, latencyP99Ns: `${base}.latency_p99_ns`,
    freshnessMs: `${base}.provider_freshness_ms`,
  } as const;
}

// Reserved future action names; no dispatch implementation or user authority.
export const FUTURE_ACTIONS = [
  { id: "CONFIG_UPDATE", label: "Quellen / Venues / Scope ändern", phase: "I–J", owner: "native control", note: "Generation-CAS; nie Signatur- oder BUY-Freigabe." },
  { id: "MODE_REQUEST", label: "Laufzeitmodus ändern", phase: "I–J", owner: "native control", note: "Requested / effective plus Safety-Gates." },
  { id: "REQUALIFY", label: "Erneut qualifizieren", phase: "G0.6–G0.7", owner: "offline qualification", note: "Keine automatische Signatur." },
  { id: "DRY", label: "DRY durchführen", phase: "G0.6–G0.7", owner: "offline qualification", note: "Kein Freigabeersatz; gefrorene Nachweise erhalten." },
  { id: "APPROVE_EXACT_V4", label: "Exakten V4-Vertrag genehmigen", phase: "separate user approval", owner: "economic boundary", note: "Eigenständige ausdrückliche Entscheidung erforderlich." },
  { id: "ARM", label: "Arm", phase: "separate user approval", owner: "native guard", note: "Kein UI-Dispatch." },
  { id: "SEND_G07", label: "G0.7-BUY senden", phase: "separate user approval", owner: "native G0.7", note: "Kein UI-Dispatch, kein simuliertes Ergebnis." },
  { id: "EMERGENCY_STOP", label: "Not-Aus / Disable", phase: "J", owner: "native lifecycle", note: "Nur mit künftig verifiziertem nativen Stop." },
] as const satisfies readonly PlannedControl[];
