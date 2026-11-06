// V8 Health (FE-V8-02, BSC-V8-11): the header pill and the separate Health window, from the native
// states as they arrive. Nothing here measures or decides; a value the owner did not report is
// UNKNOWN (grey), never green. REV.2.0 WATCH (deployment drift, update detection) is not
// implemented: the portals show what the readiness comparison and the runtime record name, and
// otherwise "Not monitored (REV.2.0)".
import type { BscLiveView, BscStatusView, NativeRecordView } from "./types";
import { RUNTIME_SCHEMA, runtimeView, sourceControlView } from "./sources";

/** ok green · warn amber · down red · update red (a change against the qualified binding) ·
 *  off grey (switched off) · unknown grey (no report). */
export type HealthState = "ok" | "warn" | "down" | "update" | "off" | "unknown";
export interface HealthEntry { id: string; name: string; detail: string; state: HealthState; status: string }
export interface HealthGroup { id: "system" | "sources" | "builders" | "portals"; title: string; entries: HealthEntry[] }
export interface HealthView { groups: HealthGroup[]; severity: "ok" | "warn" | "critical"; alerts: number }

/** The gateway connection's own state (core/realtime ConnectionStatus). */
export type GatewayState = "IDLE" | "CONNECTING" | "HANDSHAKE" | "OPEN" | "STALE" | "BLOCKED" | string;

const SOURCE_STATE: Record<string, HealthState> = { HEALTHY: "ok", DEGRADED: "warn", RATE_LIMITED: "warn", UNQUALIFIED: "warn", DISCONNECTED: "down", OFF: "off" };
export const SOURCE_STATUS: Record<string, string> = { HEALTHY: "Connected", DEGRADED: "Quiet", RATE_LIMITED: "Rate limited", UNQUALIFIED: "Not qualified", DISCONNECTED: "Unreachable", OFF: "Disabled" };
const CHAIN_STATE: Record<string, HealthState> = { FOLLOWING: "ok", STALLED: "warn", RATE_LIMITED: "warn", FAILING: "down" };
const CONFIRM_STATE: Record<string, HealthState> = { AGREED: "ok", STALE: "warn", BEHIND: "warn", NO_COMMON_HEIGHT: "warn", DISAGREES: "down" };
const QUALIFIED = (v: string | undefined) => v === "QUALIFIED";

/** A readable source name for the five ingress IDs; an unknown ID shows as itself. */
export function sourceName(id: string): string {
  const name: Record<string, string> = { BSC_INGRESS_GETH: "Geth (local)", BSC_INGRESS_PUBLICNODE: "PublicNode", BSC_INGRESS_DRPC: "dRPC", BSC_INGRESS_BLOCKREQ: "Blockrequest", BSC_INGRESS_48CLUB: "48Club" };
  return name[id] ?? id.replace(/^BSC_INGRESS_/, "");
}
/** A readable venue name for the three venue IDs; an unknown ID shows as itself. */
export function venueName(id: string): string {
  const name: Record<string, string> = { BSC_VENUE_FLAP: "Flap", BSC_VENUE_FOUR_CLASSIC: "Four Classic", BSC_VENUE_OPENFOUR: "OpenFour" };
  return name[id] ?? id.replace(/^BSC_VENUE_/, "");
}
export const age = (ms: string | undefined) => {
  if (ms === undefined || !/^[0-9]+$/.test(ms)) return "unknown";
  const n = Number(ms);
  return n < 1000 ? `${n} ms` : n < 120_000 ? `${Math.round(n / 1000)} s` : `${Math.round(n / 60_000)} min`;
};

export interface HealthInputs {
  gateway: GatewayState;
  statusTopic: string;
  status: BscStatusView | null;
  live: BscLiveView | null;
  records: readonly NativeRecordView[];
  nowMs: number;
}

export function healthView({ gateway, statusTopic, status, live, records, nowMs }: HealthInputs): HealthView {
  const { view: runtime } = runtimeView(records, nowMs);
  const control = sourceControlView(records, nowMs);
  const rt = records.find(r => r.schema === RUNTIME_SCHEMA);
  const f = new Map(rt?.fields.map(x => [x.name, x.value]) ?? []);
  const fresh = runtime.state === "RUNNING";

  const system: HealthEntry[] = [
    { id: "gateway", name: "Gateway", detail: "User interface connection",
      // The connection alone is not green: the BSC status must be current too.
      state: gateway === "OPEN" ? (statusTopic === "LIVE" ? "ok" : "warn") : gateway === "STALE" || gateway === "CONNECTING" || gateway === "HANDSHAKE" ? "warn" : gateway === "BLOCKED" || gateway === "CLOSED" ? "down" : "unknown",
      status: gateway === "OPEN" ? (statusTopic === "LIVE" ? "Connected" : `Connected · status ${statusTopic.toLowerCase()}`) : gateway === "BLOCKED" ? "Refused" : gateway === "CLOSED" ? "Disconnected" : gateway === "STALE" ? "Stale" : gateway === "IDLE" ? "Not connected" : "Connecting" },
    { id: "engine", name: "Trading engine", detail: runtime.state === "NOT_RUNNING" ? "Rust runtime process · no status record" : `Rust runtime process · mode ${runtime.mode}`,
      state: runtime.state === "RUNNING" ? "ok" : runtime.state === "STALE" ? "warn" : "unknown",
      status: runtime.state === "RUNNING" ? "Running" : runtime.state === "STALE" ? "Status stale" : "Not reported" },
  ];
  if (f.has("canonical[0].state")) {
    const state = f.get("canonical[0].state") ?? "";
    system.push({ id: "chain", name: "Blockchain · RPC", detail: `Primary chain reader · head ${f.get("canonical[0].head") ?? "?"} · ${age(f.get("canonical[0].head_age_ms"))} old`,
      state: fresh ? CHAIN_STATE[state] ?? "warn" : "warn", status: fresh ? state || "UNKNOWN" : "Status stale" });
    const confirm = f.get("canonical[0].confirm_state") ?? "";
    system.push({ id: "confirm", name: "Blockchain · second source", detail: f.has("canonical[0].confirm_endpoint") ? `Independent chain confirmation · head ${f.get("canonical[0].confirm_head") ?? "?"}` : "No second source configured",
      state: !f.has("canonical[0].confirm_endpoint") ? "off" : fresh ? CONFIRM_STATE[confirm] ?? "unknown" : "warn",
      status: !f.has("canonical[0].confirm_endpoint") ? "Not configured" : fresh ? confirm || "UNKNOWN" : "Status stale" });
  } else {
    system.push({ id: "chain", name: "Blockchain · RPC", detail: "Primary chain reader", state: "unknown", status: "Not reported" });
    system.push({ id: "confirm", name: "Blockchain · second source", detail: "Independent chain confirmation", state: "unknown", status: "Not reported" });
  }

  // The five sources as the source-mode owner names them, each with the runtime's health.
  const sources: HealthEntry[] = control.rows.map(row => {
    let i = 0; while (f.has(`sources[${i}].source_id`) && f.get(`sources[${i}].source_id`) !== row.id) i++;
    const has = f.get(`sources[${i}].source_id`) === row.id;
    const health = has && fresh ? f.get(`sources[${i}].health`) ?? "" : "";
    const off = row.effective === "OFF";
    return { id: row.id, name: row.id === "BSC_INGRESS_GETH" ? sourceName(row.id) : `${sourceName(row.id)} · WSS`,
      detail: has ? `Last signal: ${age(f.get(`sources[${i}].last_event_age_ms`))} · ${(f.get(`sources[${i}].capability`) ?? "UNKNOWN").replace("_", "-").toLowerCase()}` : "Last signal: unknown",
      state: off ? "off" : health ? SOURCE_STATE[health] ?? "warn" : "unknown",
      status: off ? "Disabled" : health ? SOURCE_STATUS[health] ?? health : fresh ? "Not reported" : "Status unknown" };
  });

  // Builders: the inclusion adapters the owner offers; their qualification from the builder
  // evidence and the bound builder policy, never a mere answer.
  const b = status?.builder;
  const adapters = live?.grantAdapters?.length ? live.grantAdapters : [];
  const policyRefused = live?.builderPolicyRefusal;
  const builderName = (a: string) => a.startsWith("CLUB48") ? "48Club · Builder" : a.startsWith("BLOCKRAZOR") ? "BlockRazor · Builder" : a;
  const builders: HealthEntry[] = adapters.map(a => {
    const qualified = QUALIFIED(b?.price) && QUALIFIED(b?.inclusion);
    const known = !!b && (b.price !== "" || b.inclusion !== "");
    return { id: a, name: builderName(a),
      detail: known ? `Qualification · price ${b?.price || "UNKNOWN"} · inclusion ${b?.inclusion || "UNKNOWN"}` : "Transaction submission interface · no qualification evidence",
      state: policyRefused ? "warn" : !known ? "unknown" : qualified ? "ok" : "warn",
      status: policyRefused ? "Policy refused" : !known ? "Not reported" : qualified ? "Qualified" : "Not qualified" };
  });

  // Portals: the readiness comparison's version and code hash, the runtime's Four deployment; no
  // WATCH owner (REV.2.0), so nothing is claimed as unchanged without such an observation.
  const r = status?.readiness;
  const portals: HealthEntry[] = control.venues.map(v => {
    if (v.id === "BSC_VENUE_FLAP" && r && (r.portalVersionBound || r.portalVersionObserved)) {
      const changed = r.codeHashChanged === true || (!!r.portalVersionBound && !!r.portalVersionObserved && r.portalVersionBound !== r.portalVersionObserved);
      // A change the readiness comparison saw is evidence; "unchanged" is only as of that comparison, no WATCH.
      return { id: v.id, name: "Flap · Portal", detail: `Version ${r.portalVersionBound || "?"} → ${r.portalVersionObserved || "?"} (readiness comparison)`,
        state: changed ? "update" : "unknown", status: changed ? "Version review" : r.codeHashChanged === false ? "Unchanged at readiness" : "Not compared" };
    }
    if (v.id === "BSC_VENUE_FOUR_CLASSIC" && f.has("canonical[0].four_deployment")) {
      return { id: v.id, name: "Four Classic · Portal", detail: `Deployment ${f.get("canonical[0].four_deployment")} (runtime)`, state: "unknown", status: "Not monitored (REV.2.0)" };
    }
    return { id: v.id, name: `${venueName(v.id)} · Portal`, detail: "Contract / deployment status", state: "unknown", status: "Not monitored (REV.2.0)" };
  });

  const groups: HealthGroup[] = [
    { id: "system", title: "System & blockchain", entries: system },
    { id: "sources", title: "Signal sources · WSS & Geth", entries: sources },
    { id: "builders", title: "Builder", entries: builders },
    { id: "portals", title: "Portals & contract versions", entries: portals },
  ];
  const all = groups.flatMap(g => g.entries);
  const alerts = all.filter(e => e.state === "down" || e.state === "warn" || e.state === "update").length;
  // Green only with every reported entry healthy or off and nothing missing: an unknown entry or an
  // empty group (no sources, no builders) is not healthy. The portals stay unknown until WATCH (REV.2.0)
  // and do not hold the pill amber on their own.
  const missing = groups.some(g => g.id !== "portals" && (!g.entries.length || g.entries.some(e => e.state === "unknown")));
  const severity = all.some(e => e.state === "down" || e.state === "update") ? "critical" : alerts > 0 || missing ? "warn" : "ok";
  return { groups, severity, alerts };
}
