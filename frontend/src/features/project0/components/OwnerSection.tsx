import { useState } from "react";
import type { TopicStatus } from "@/core/realtime";
import type { LatencyView, LiquidatorView, OwnerThresholdView } from "../model/owner";
import type { SystemView } from "../model/status";
import { admissionLine, nsToMs, programPinStatus } from "../model/owner";
import { usdMicroDisplay } from "../model/format";
import { orDash, parameterText } from "../model/statusFormat";
import { DetailRow, Notice, Section } from "./PanelPrimitives";
import { analyticsCommand, capabilityCommand, useAnalyticsControl } from "@/core/realtime/analyticsControl";

const ms = (ns: bigint | null) => {
  const v = nsToMs(ns);
  return v == null ? "—" : `${v} ms`;
};
/** DEC-06: a duration below a millisecond in µs (the threshold path's wake-ups). */
const dur = (ns: bigint | null) => ns == null ? "—" : ns < 1_000_000n ? `${ns / 1_000n} µs` : `${ns / 1_000_000n} ms`;
const percentiles = (l: LatencyView) => l.count === 0n ? "no sample" : `${dur(l.p50)} / ${dur(l.p95)} / ${dur(l.p99)} (n ${l.count})`;

/** D01/DEC-05: the owner and liquidator parameters of the engine's registry shown in this tab. */
const OWNER_PARAMETERS = [
  "workers", "sweep_batch_max_events", "sweep_batch_max_age_ns", "ingress_burst_before_detect",
  "blockhash_max_age_ms", "blockhash_refresh_ms", "projection_requeue_max", "lane_weight_l0", "lane_weight_l1", "lane_weight_l2",
  "jupiter_timeout_ms", "fe04_settle_retry_ms", "fe04_refresh_ms", "owner_status_ms", "prepared_candidates",
  "rebalance_observe_ms", "rebalance_budget_usd_micro", "rebalance_quote_rounds",
  "max_total_capital", "max_outstanding_inventory", "max_concurrent_positions", "max_exposure_per_attempt",
  "max_daily_loss", "max_failed_attempts_per_day", "max_tip", "max_slippage_bps", "max_outstanding_attempts",
  "min_profit_usd_micro", "jupiter_max_accounts", "compute_unit_limit", "priority_micro_lamports_per_cu",
  "loaded_accounts_data_size", "max_opportunity_age_ns", "execution_deadline_ns", "integration_probe_limit_per_kind",
  "threshold_bootstrap_batch", "candidate_recheck_slots", "candidate_error_recheck_slots", "candidate_repeat_soft_cap",
  "rpc_candidates_per_drain", "candidate_network_deadline_ms", "mt0_interval_seconds",
  "recovery_root_catchup_seconds", "recovery_root_stall_seconds", "recovery_root_hard_max_seconds",
  "fe04_approval_max_lifetime_seconds", "threshold_observe_budget_ppm", "threshold_crosscheck_accounts",
] as const;

/** The owner's typed settings (DEC-05). */
const OWNER_SETTINGS = ["diagnostics", "threshold_trigger", "c0_census", "source_geyser", "source_geyser_provider"] as const;


/**
 * The running state owner as the Rust owner published it (project0.liquidator.state): state,
 * ingress, trigger, the day's decisions, Safety and every capability with its gate. Nothing is
 * derived here; a topic without a state shows its status.
 */
export function OwnerSection({ owner, topicStatus, system }: { owner: LiquidatorView | null; topicStatus: TopicStatus; system: SystemView | null }) {
  const params = <>
    {OWNER_PARAMETERS.map((name) => {
      const p = system?.parameters.get(name);
      return <DetailRow key={name} title={name} value={p ? parameterText(p) : "Not reported"}/>;
    })}
    {OWNER_SETTINGS.map((name) => {
      const s = system?.settings.get(name);
      return <DetailRow key={name} title={name} value={s ? `${s.value} · ${s.source} · restart` : "Not reported"}/>;
    })}
  </>;
  if (!owner) {
    return <Section title="Runtime owner" aside={<span className="p0-badge">{topicStatus}</span>}>
      <p className="p0-quiet">No Shadow/econ4h owner state received.</p>{params}</Section>;
  }
  const { engine: e, trigger: t, pipeline: p, safety: s } = owner;
  // A state of a topic that is not LIVE is the last reported one, never shown as running.
  const live = topicStatus === "LIVE";
  return <Section title="Runtime owner" aside={<span className="p0-badge">{live ? `${e?.runState ?? "—"} · ${owner.mode}` : `${topicStatus} · last ${e?.runState ?? "—"}`}</span>}>
    {live ? null : <p className="p0-quiet">Last reported state ({topicStatus}); not current.</p>}
    <DetailRow title="Run" value={`${owner.runId} · ${owner.mode}${owner.endResult ? ` · ended ${owner.endResult}` : ""}`}/>
    <DetailRow title="State" value={e ? `${e.runState}${e.haltReason ? ` · ${e.haltReason}` : ""}${e.programUpgradeLatched ? " · program upgrade" : ""}` : "—"}/>
    {e ? (() => { const pin = programPinStatus(e.programPin, e.programUpgradeLatched); return <DetailRow title={`Program pin · ${pin.status}`} value={pin.detail}/>; })() : null}
    <DetailRow title="Ingress · generation / reconnects" value={e ? `${e.ingressGeneration} / ${e.reconnects}${e.gapOpen ? " · gap open" : ""}${e.ingressHaltRaised ? " · halt raised" : ""}` : "—"}/>
    <DetailRow title="Same-slot updates · unversioned refused / unordered" value={e ? `${orDash(e.counters.get("same_slot_unversioned_refused"))} / ${orDash(e.counters.get("same_slot_no_wv"))}` : "—"}/>
    <DetailRow title="Trigger · lanes L0/L1/L2 · detect" value={t ? `${t.mode} · ${t.lanes.join("/")} · ${t.detectQueue}` : "—"}/>
    <DetailRow title="Repeats suppressed · soft cap" value={t ? `${t.suppressed} · ${t.softcapSuppressed}` : "—"}/>
    {t?.threshold ? <ThresholdRows th={t.threshold}/> : null}
    <DetailRow title="Loss Build · BLOCKHASH_STALE before planning · parked / re-armed / superseded · refresh wake-ups"
      value={t ? `${t.blockhashRefused} · ${t.blockhashParked} / ${t.blockhashRearmed} / ${t.blockhashSuperseded} · ${t.blockhashRefreshRequests}` : "—"}/>
    <DetailRow title="Day · health / liquidatable / plans" value={p ? `${p.healthChecked} / ${p.liquidatable} / ${p.plansBuilt} built, ${p.plansRejected} rejected` : "—"}/>
    <DetailRow title="Day · simulations pass / fail · shadow ready / wins" value={p ? `${p.simulationsPassed} / ${p.simulationsFailed} · ${p.shadowReady} / ${p.shadowWins}` : "—"}/>
    <DetailRow title="Latency p95 · decision (rx→ready) / fee quote / simulation" value={p ? `${ms(p.decisionP95Ns)} / ${ms(p.feeQuoteP95Ns)} / ${ms(p.simulationP95Ns)}` : "—"}/>
    <DetailRow title="Safety · capital in use / limit" value={s ? `${usdMicroDisplay(s.capitalInUseUsdMicro)} / ${usdMicroDisplay(s.maxTotalCapitalUsdMicro)}${s.haltReason ? ` · halted ${s.haltReason}` : ""}` : "—"}/>
    <DetailRow title="Safety · day realized / loss limit" value={s ? `${usdMicroDisplay(s.dayRealizedUsdMicro)} / ${usdMicroDisplay(s.maxDailyLossUsdMicro)}` : "—"}/>
    {owner.rebalance ? <>
      <DetailRow title="Rebalancing · plan" value={owner.rebalance.atUnixMs == null ? `${owner.rebalance.mode} · nothing planned yet`
        : `${owner.rebalance.mode} · slot ${owner.rebalance.slot} · budget ${usdMicroDisplay(owner.rebalance.budgetUsdMicro.toString())}${owner.rebalance.quotesPending > 0n ? ` · ${owner.rebalance.quotesPending} quotes pending (provisional)` : ""}`}/>
      {owner.rebalance.trades.map((t, i) => <DetailRow key={`t${i}`} title={`Rebalance · sell ${t.sell.slice(0, 8)} → ${t.buy.slice(0, 8)}`}
        value={`${t.amountIn} → ≥ ${t.minimumOut} · cost ${usdMicroDisplay(t.costUsdMicro.toString())} · planned, never traded`}/>)}
      {owner.rebalance.blocked.map((b) => <DetailRow key={`b${b.asset}`} title={`Rebalance · ${b.asset.slice(0, 8)}`} value={b.reason}/>)}
    </> : null}
    {owner.approval ? <DetailRow title="FE-04 approval"
      value={owner.approval.state === "NONE" ? "none configured · LIVE not authorized (FE-06)"
        : `${owner.approval.state}${owner.approval.reason ? ` (${owner.approval.reason})` : ""} · ${owner.approval.kind} ${owner.approval.approvalId}`
          + `${owner.approval.kind === "attempt" ? ` · ${owner.approval.fundingClass} · capital ≤ ${usdMicroDisplay(owner.approval.maxCapitalUsdMicro.toString())} · loss ≤ ${usdMicroDisplay(owner.approval.maxLossUsdMicro.toString())} · ${owner.approval.allowedTargets} targets` : ""}`
          + ` · slippage ≤ ${owner.approval.maxSlippageBps} bps · expires ${new Date(Number(owner.approval.expiresUnixS) * 1000).toISOString()} · LIVE not authorized (FE-06)`}/> : null}
    {owner.admission ? <DetailRow title="FE-04 admission" value={admissionLine(owner.admission)}/> : null}
    {owner.capabilities.map((c) => <DetailRow key={c.name} title={c.name}
      value={`${c.effective}${c.requested !== c.effective ? ` (requested ${c.requested})` : ""}${c.blockReason ? ` · ${c.blockReason}` : ""}`}/>)}
    <LiveRequest owner={owner} live={live}/>
    <ModeRequest owner={owner} live={live} capability="HOLD_B" label="B hold mode" action="Request hold mode"
      active="Active (needs N and the evidence)"/>
    <ModeRequest owner={owner} live={live} capability="JITO_BUNDLE" label="Bundle path" action="Request bundle path"
      active="Active (needs a qualified bundle simulator)"/>
    <ModeRequest owner={owner} live={live} capability="THRESHOLD_TRIGGER" label="Threshold trigger" action="Request threshold mode"
      active="Active (from Observe; needs N and the evidence)"/>
    {params}
  </Section>;
}

type Mode = "OFF" | "OBSERVE" | "ACTIVE";

/**
 * DEC-06: the threshold mode on the Liquidator tab — requested/effective, generation and block
 * reason; OBSERVE's comparison with the sweep on the same ticks (hits found by both, misses, extra
 * wake-ups, detection p50/p95/p99 of both paths), the effort of both paths and the owner time of
 * the threshold work beside the sweep against its budget; ACTIVE's cross-check and fallbacks.
 */
function ThresholdRows({ th }: { th: OwnerThresholdView }) {
  return <>
    <DetailRow title="Threshold mode · requested / effective · generation"
      value={`${th.requested} (${th.requestedSource}) / ${th.effective}${th.blockReason ? ` · ${th.blockReason}` : ""} · gen ${th.generation}`}/>
    <DetailRow title="Threshold · comparable ticks / since last miss / N · not comparable"
      value={`${th.ticks} / ${th.streak} / ${th.minTicks ?? "undecided (B-8)"} · ${th.uncomparableTicks}`}/>
    <DetailRow title="Threshold · hits (sweep and threshold) / misses / extra wake-ups" value={`${th.hits} / ${th.misses} / ${th.extra}`}/>
    <DetailRow title="Detection p50 / p95 / p99 · sweep" value={percentiles(th.sweepDetect)}/>
    <DetailRow title="Detection p50 / p95 / p99 · threshold" value={percentiles(th.thresholdDetect)}/>
    <DetailRow title="Owner time beside the sweep · p50 / p95 / p99 · total · budget · sampled"
      value={`${percentiles(th.observeOwner)} · ${dur(th.observeOwnerNsTotal)} · ${th.observeBudgetPpm} ppm · ${th.sampledOut}`}/>
    <DetailRow title="Effort · sweep health evaluations / owner time · threshold index evaluations"
      value={`${th.sweepHealthEvaluations} / ${dur(th.sweepOwnerNsTotal)} · ${th.indexEvaluations}`}/>
    <DetailRow title="Threshold index · exact / unknown / pending" value={`${th.indexTracked} / ${th.indexFallback} / ${th.indexBootstrapPending}`}/>
    <DetailRow title="Cross-check (ACTIVE) · samples / skipped / stale / misses · fallbacks"
      value={`${th.crosscheckSamples} / ${th.crosscheckSkipped} / ${th.crosscheckStale} / ${th.crosscheckMisses} · ${th.fallbacks}`}/>
  </>;
}

/**
 * DEC-04/DEC-11 (N-5): a capability mode through the same controller (`p0ctl capability set
 * HOLD_B|JITO_BUNDLE …`, CAS on the capability configuration version). ACTIVE is only requested
 * here: the owner decides what is effective — the B hold with the user's N (B-8) and N finalized
 * attempts without an underestimation, the bundle path never without a qualified bundle
 * simulator — and the capability row shows the reason.
 */
function ModeRequest({ owner, live, capability, label, action, active }: {
  owner: LiquidatorView; live: boolean; capability: "HOLD_B" | "JITO_BUNDLE" | "THRESHOLD_TRIGGER"; label: string; action: string; active: string;
}) {
  const { busy } = useAnalyticsControl();
  const row = owner.capabilities.find((c) => c.name === capability) ?? null;
  // The operator's current request until another value is chosen (a click never changes it unseen).
  const [chosen, setMode] = useState<Mode | null>(null);
  const requested = row && ["OFF", "OBSERVE", "ACTIVE"].includes(row.requested) ? row.requested as Mode : "OFF";
  const mode = chosen ?? requested;
  const version = owner.capabilityConfigVersion;
  const blocked = busy ? "Command in progress" : !live ? "Runtime owner not current"
    : version == null ? "Capability configuration version unknown (no capability control in this runtime)" : "";
  if (!row) return null;
  return <>
    <div className="p0-formgrid">
      <label>{label}<select value={mode} onChange={(e) => setMode(e.target.value as Mode)}>
        <option value="OFF">Off</option><option value="OBSERVE">Observe</option><option value="ACTIVE">{active}</option></select></label>
    </div>
    <div className="p0-actions">
      <button className="p0-secondary" disabled={blocked !== ""} title={blocked || undefined}
        onClick={() => void analyticsCommand("project0.capability.set", { params: capabilityCommand(capability, mode).params, configVersion: version ?? undefined })}>
        {action}
      </button>
    </div>
  </>;
}

/**
 * DEC-13 (N-5): the LIVE request through the one controller (`p0ctl capability set LIVE …`, CAS
 * on the capability configuration version this view showed). It records what the operator asks
 * for; LIVE stays effective BLOCKED until a separate FE-06 authorization — no request makes the
 * owner send anything.
 */
function LiveRequest({ owner, live }: { owner: LiquidatorView; live: boolean }) {
  const { busy, receipt } = useAnalyticsControl();
  const requested = owner.capabilities.find((c) => c.name === "LIVE")?.requested ?? null;
  const version = owner.capabilityConfigVersion;
  const blocked = busy ? "Command in progress" : !live ? "Runtime owner not current"
    : version == null ? "Capability configuration version unknown (no capability control in this runtime)" : "";
  const next = requested === "LIVE" ? "OFF" : "LIVE";
  const d = owner.lastCapabilityDecision;
  return <>
    {d ? <DetailRow title={`Capability decision · #${d.seq}`} value={`${d.applied ? "APPLIED" : "REFUSED"} · ${d.request} · ${d.detail}`}/> : null}
    <div className="p0-actions">
      <button className="p0-secondary" disabled={blocked !== ""} title={blocked || undefined}
        onClick={() => void analyticsCommand("project0.capability.set", { params: capabilityCommand("LIVE", next).params, configVersion: version ?? undefined })}>
        {next === "LIVE" ? "Request LIVE (stays BLOCKED until FE-06)" : "Withdraw LIVE request"}
      </button>
    </div>
    {receipt?.action === "project0.capability.set" ? <Notice><span data-testid="p0-capability-command-receipt">Command {receipt.phase}{receipt.detail ? ` · ${receipt.detail}` : ""}</span></Notice> : null}
  </>;
}
