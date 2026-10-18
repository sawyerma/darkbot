import { DetailRow, Notice } from "./PanelPrimitives";
import { fmtAgeNs, fmtBytes, orDash } from "../model/statusFormat";
import { selectStatus, useProject0Store } from "../store/project0Store";

export function HealthSection() {
  const live = useProject0Store(selectStatus);
  const v = live.projection;
  const hasValues = v !== null;
  const mode = live.topicStatus === "LIVE" ? "Live via gateway"
    : live.topicStatus === "DETACHED" ? "Source detached · last known values"
    : live.topicStatus === "STALE" ? "Stale · last known values"
    : "Backend not connected";
  return <section className="p0-health" aria-label="System Health">
    <h2>System Health <span className="p0-quiet">· {mode}</span></h2>
    <DetailRow title="Software & program identity" value="Not checked"/>
    <DetailRow title="RPC / WebSocket / slot progression" value={hasValues ? `last slot ${orDash(v.lastObservedSlot)} · frame age ${fmtAgeNs(v.lastFrameAgeNs)}` : "Not checked"}/>
    <DetailRow title="Wallet / provider / capacity" value="Not checked"/>
    <DetailRow title="RPC latency · p50 / p95 / p99" value="— / — / — ms"/>
    <DetailRow title="Ingress → decision · p50 / p95 / p99" value="— / — / — ms"/>
    <DetailRow title="Stream age / writer queue" value={hasValues ? `${fmtAgeNs(v.lastFrameAgeNs)} / ${fmtBytes(v.queueBytes)} (lag ${orDash(v.lagFrames)})` : "— / —"}/>
    <DetailRow title="Market verdict (live, pre-verification)" value={hasValues ? orDash(v.marketVerdict) : "—"}/>
    <DetailRow title="Gaps / reconnects / liquidation events" value={hasValues ? `${orDash(v.gapCount)} / ${orDash(v.reconnectCount)} / ${orDash(v.liquidationEvents)}` : "— / — / —"}/>
    <DetailRow title="Gateway topic project0.status" value={`${live.topicStatus}${live.reason ? ` · ${live.reason}` : ""}`}/>
    <Notice>{hasValues
      ? `Read-only status via gateway, run ${v.runId}. Verdict is pre-verification and never MARKET_DATA_VALID during capture.`
      : "No live Boot Audit API is connected. Checks cannot be marked PASS."}</Notice>
  </section>;
}
