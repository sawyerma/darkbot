import { AllValues } from "./AllValues";
import { DetailRow, Notice } from "./PanelPrimitives";
import type { CounterName } from "../model/status";
import {
  DASH, fmtAgeNs, fmtBytes, fmtBytesExact, fmtClockMs, fmtEtaS, fmtRate, orDash,
} from "../model/statusFormat";
import { counterOf, phaseLabel } from "../model/statusSelectors";
import { selectCapture, selectSystem, useProject0Store } from "../store/project0Store";

export function HealthSection() {
  const capture = useProject0Store(selectCapture);
  const system = useProject0Store(selectSystem);
  const c = capture.value;
  const run = c?.run ?? null;
  const phase = c?.phase ?? null;
  const t = c?.transport ?? null;
  const a = c?.assessment ?? null;
  const w = c?.writer ?? null;
  const n = (name: CounterName) => orDash(counterOf(c, name));
  const mode = capture.topicStatus === "LIVE" ? "Live via gateway"
    : capture.topicStatus === "DETACHED" ? "Source detached · last known values"
    : capture.topicStatus === "STALE" ? "Stale · last known values"
    : "Backend not connected";
  const frame = capture.meta ? `g${capture.meta.generation}/s${capture.meta.seq}` : DASH;
  // Both timestamps come from the engine clock; browser clock skew is irrelevant.
  const sampleDelta = w && capture.meta
    ? capture.meta.sourceTimestampUnixNs / 1_000_000n - w.sampledUnixMs : null;
  const sampleAge = sampleDelta === null ? DASH
    : sampleDelta < 0n ? "clock moved backwards" : `${sampleDelta} ms`;
  const connection = t?.connected == null ? "unknown" : t.connected ? "connected" : "disconnected";
  const s = system.value;

  return <section className="p0-health" aria-label="System Health">
    <h2>System Health <span className="p0-quiet">· {mode}</span></h2>
    <DetailRow title="Run" value={run ? `${run.runId} · ${run.source}/${run.commitment} · ${run.configuredDurationSeconds} s` : "Not reported"}/>
    <DetailRow title="Phase" value={phase ? `${phaseLabel(c)} since ${fmtClockMs(phase.changedUnixMs)}${phase.stopReason ? ` · stop ${phase.stopReason}` : ""}${phase.endResult ? ` · ${phase.endResult}` : ""}` : DASH}/>
    <DetailRow title="Operational profile" value={s ? `${s.activated ? s.profile : "built-in defaults"} · ${s.parameterCount} parameters` : "Not reported"}/>
    <DetailRow title="Software & program identity" value={run ? `program ${run.programId}${a?.deploymentError ? ` · ${a.deploymentError}` : ""}` : "Not checked"}/>
    <DetailRow title="RPC / WebSocket / slot progression" value={run || t ? `${run?.wsHost ?? DASH} · ${connection} · last slot ${orDash(counterOf(c, "last_observed_slot") ?? t?.lastSlot ?? null)} · frame age ${fmtAgeNs(t?.lastFrameAgeNs)}` : "Not checked"}/>
    <DetailRow title="Transport events" value={t || c?.market ? `${t?.lastState || "none"}${t?.lastCode ? ` ${t.lastCode}` : ""} · HTTP 429 ${n("transport_http_429")} · reconnect attempts ${n("transport_reconnect_attempts")} · recovered ${n("transport_recovered")}` : DASH}/>
    <DetailRow title="Wallet / provider / capacity" value="Not collected"/>
    <DetailRow title="RPC latency · p50 / p95 / p99" value="Not collected"/>
    <DetailRow title="Ingress → decision · p50 / p95 / p99" value="Not collected"/>
    <DetailRow title="Stream age / writer queue" value={c ? `${fmtAgeNs(t?.lastFrameAgeNs)} / ${w ? `${fmtBytes(w.queueBytes)} (lag ${w.lagFrames})` : DASH}` : "— / —"}/>
    <DetailRow title="Writer sample · engine clock / age at publication / frame" value={w ? `${fmtClockMs(w.sampledUnixMs)} / ${sampleAge} / ${frame}` : "Not sampled"}/>
    <DetailRow title="Writer rate · in / out" value={w ? `${fmtRate(w.ingressBytesPerS)} / ${fmtRate(w.writeBytesPerS)}` : "— / —"}/>
    <DetailRow title="Writer budget · peak / free / max" value={w ? `${fmtBytes(w.queuePeakBytes)} / ${fmtBytes(w.freeBudgetBytes)} / ${fmtBytes(w.maxBytes)}` : "— / — / —"}/>
    <DetailRow title="Journal disk · free / reserve" value={w ? `${fmtBytesExact(w.diskFreeBytes)} (${fmtBytes(w.diskFreeBytes)}) / ${fmtBytes(w.diskReserveBytes)}` : "— / —"}/>
    <DetailRow title="Exhaustion forecast · budget / disk" value={w ? `${fmtEtaS(w.budgetExhaustionS)} / ${fmtEtaS(w.diskExhaustionS)}` : "— / —"}/>
    <DetailRow title="Market verdict (live, pre-verification)" value={a ? `${a.verdict}${a.incompleteReasons.length ? ` · ${a.incompleteReasons.join(", ")}` : ""}` : DASH}/>
    <DetailRow title="Transactions · unique / success / failed / unknown" value={c?.market ? `${n("unique_transactions")} / ${n("successful_transactions")} / ${n("failed_transactions")} / ${n("unknown_status_transactions")}` : DASH}/>
    <DetailRow title="Gaps / reconnects / liquidation events" value={c?.market ? `${n("gap_count")} / ${n("reconnect_count")} / ${n("liquidation_events")}` : "— / — / —"}/>
    <DetailRow title="Gross edge · total / p50 / p90" value={a ? `${a.grossEdgeTotalUsd.toFixed(2)} / ${a.grossEdgeP50Usd?.toFixed(2) ?? DASH} / ${a.grossEdgeP90Usd?.toFixed(2) ?? DASH} USD` : DASH}/>
    <DetailRow title="Gateway topics" value={`project0.status ${system.topicStatus} · project0.capture.state ${capture.topicStatus}${capture.reason ? ` · ${capture.reason}` : ""}`}/>
    <Notice>{c
      ? "Read-only status via gateway. The verdict is pre-verification and never MARKET_DATA_VALID during capture. Latency, wallet and capacity are not collected by the engine."
      : "No Project 0 state received. Checks cannot be marked PASS."}</Notice>
    <AllValues/>
  </section>;
}
