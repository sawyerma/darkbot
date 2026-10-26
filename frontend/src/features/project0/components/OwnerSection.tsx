import type { TopicStatus } from "@/core/realtime";
import type { LiquidatorView } from "../model/owner";
import type { SystemView } from "../model/status";
import { nsToMs, programPinStatus } from "../model/owner";
import { usdMicroDisplay } from "../model/format";
import { orDash, parameterText } from "../model/statusFormat";
import { DetailRow, Section } from "./PanelPrimitives";

const ms = (ns: bigint | null) => {
  const v = nsToMs(ns);
  return v == null ? "—" : `${v} ms`;
};

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
    {owner.capabilities.map((c) => <DetailRow key={c.name} title={c.name}
      value={`${c.effective}${c.requested !== c.effective ? ` (requested ${c.requested})` : ""}${c.blockReason ? ` · ${c.blockReason}` : ""}`}/>)}
    {params}
  </Section>;
}
