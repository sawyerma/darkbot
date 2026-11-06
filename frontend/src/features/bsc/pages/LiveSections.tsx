import { TopicId } from "@/core/realtime";
import { useTopic } from "@/core/realtime/hooks";
import { Line } from "@/shared/ui/graphite/Rows";
import { Notice } from "@/shared/ui/graphite/Notice";
import { selectLive, useBscStore } from "../store/bscStore";
import { ValueState } from "./EvidenceRows";
import { PlacedSection } from "./PlacedSection";
import { useNativeRecords } from "./useNativeRecords";
import { builderPolicyText, conditionRows, divergenceText, exitDeciderRows, heldPositions, operatorDecisions, outcomeRows, pathRows, presenceText, qualificationAgeText, recoveryRows, signedBnb, grantChoices } from "../model/live";
import { buyLimitRow, switchRow } from "../model/switches";
import { formatNative } from "../model/format";
import { BuyLimitActions } from "../BuyLimitActions";
import { SwitchActions } from "../SwitchActions";
import { LIVE_BURN, LIVE_LOT, LIVE_QUARANTINE, LiveBurnAction, LiveGrantActions, LiveLotActions, LiveQuarantineAction, LiveRevokeActions, Receipt, ScopeLiveActions, TotalLimitActions } from "../LiveActions";
import { LiveCampaign } from "./LiveCampaign";

const wei = (v: string | undefined) => v === undefined || v === "" ? "—" : /^[0-9]+$/.test(v) ? formatNative(v, "wei") : v;

/** The pre-V8 LIVE sections (plan 28.10., §13 order, placement panel "live"), kept whole and shown
 *  by the V8 Live area in its drawers: `only` names the sections a drawer shows (1 gate, 2 grant,
 *  3 activation, 4 attempts, 5 spend, 6 path, 7 positions, 8 receipts, 9 PnL); the positions table
 *  itself is the V8 trade list, so `positionsTable` is off there. Section 10 (Strategy C) is the V8
 *  strategy card. */
export function LiveSections({ only, positionsTable = true }: { only: readonly number[]; positionsTable?: boolean }) {
  const quality = useTopic(TopicId.BSC_LIVE_STATE);
  useTopic(TopicId.BSC_MONITORING_STATE); // the runtime status (path counters) and the placed records
  const { projection: l } = useBscStore(selectLive);
  const records = useNativeRecords();
  const conditions = conditionRows(l);
  const generation = l?.generation ?? 0n;
  const owner = !!l && l.gate !== "UNKNOWN";
  const econLive = switchRow("BSC_ECON_LIVE", l?.economicLive);
  const recovery = recoveryRows(l);
  const decisions = operatorDecisions(l);
  const builder = builderPolicyText(l);
  const age = qualificationAgeText(l);
  const divergence = divergenceText(l);
  const presence = presenceText(l);
  return <>
    {only.includes(1) ? <Line label="LIVE source quality" value={<ValueState state={quality} />} hint="bsc.live.state transport" /> : null}
    {only.includes(1) ? <PlacedSection panel="live" section={1} records={records}>
      <Line label="LIVE gate" value={<ValueState state={l?.gate} />} hint={l?.blockers.length ? `first: ${l.blockers[0]}` : "every condition holds"} />
      <table className="bsc-table" data-testid="bsc-live-conditions"><tbody>
        {conditions.map(c => <tr key={c.id} data-met={c.met}><td>{c.met ? "✓" : "✗"}</td><td>{c.label}</td><td>{c.reason}</td><td className="g-muted">{c.met ? "" : c.where}</td></tr>)}
      </tbody></table>
      <Line label="LIVE qualification" value={<ValueState state={l?.qualification} />} hint={l?.qualificationDetail || "bsc/config/live_qualification.dbrec, verified against its evidence"} />
      {age !== undefined ? <Line label="Qualification age" value={age} hint="BSC3-049: older than the max age (BSC_EXEC_LIVE_QUALIFICATION_MAX_AGE_MS), or for another risk scope, it admits no BUY (exits go on)" /> : null}
      {divergence !== undefined ? <Line label="Second source" value={divergence} hint="BSC3-056: BSC_EXEC_LIVE_DIVERGENCE_QUARANTINE" /> : null}
      {l?.warnings.length ? <Line label="Warnings" value={l.warnings.join(", ")} /> : null}
      <LiveQuarantineAction seq={l?.firstUnreadable} generation={generation} available={owner} />
      <Receipt testId={LIVE_QUARANTINE} />
    </PlacedSection> : null}
    {only.includes(2) ? <PlacedSection panel="live" section={2} records={records}>
      {l?.grant ? <>
        <Line label="Grant" value={`${l.grant.strategies} · ${l.grant.venue}${l.grant.exitOnly ? " · EXIT-ONLY" : ""}`} hint={`request ${l.grant.requestId} @ generation ${l.grant.setGeneration.toString()}`} />
        <Line label="Granted risk" value={l.grant.risk ? `slippage BUY ≤ ${l.grant.risk.maxBuySlippageBps} bps · exit ≤ ${l.grant.risk.maxExitSlippageBps} bps · gas price ≤ ${l.grant.risk.maxGasPriceWei.toString()} wei${l.grant.risk.outlierAck ? " · confirmed as an outlier" : ""}` : "none (the grant admits nothing)"}
          hint={l.grant.risk ? `gas limits BUY ≤ ${l.grant.risk.maxBuyGasLimit.toString()} · approval ≤ ${l.grant.risk.maxApproveGasLimit.toString()} · SELL ≤ ${l.grant.risk.maxSellGasLimit.toString()}` : "BSC3-069"} />
        <Line label="Granted limits" value={l.grant.limits ? `open positions ≤ ${l.grant.limits.maxOpenPositions} · realized loss ≤ ${signedBnb(l.grant.limits.maxRealizedLossWei)} (grant) · ${signedBnb(l.grant.limits.maxDailyRealizedLossWei)} (UTC day)` : "none (the grant arms nothing)"}
          hint={l.grant.limits ? `BUY interval ≥ ${l.grant.limits.minBuyIntervalMs.toString()} ms` : "31.10. order"} />
        <Line label="Grant expiry" value={new Date(Number(l.grant.expiresUnixMs)).toISOString()} />
      </> : <Line label="Grant" value="NONE" hint="LIVE needs the operator's grant of one confirmed scope" />}
      {l?.riskSoftGasPriceWei !== undefined ? <Line label="Risk soft bounds" value={`slippage BUY ≤ ${l.riskSoftBuySlippageBps} bps · exit ≤ ${l.riskSoftExitSlippageBps} bps · gas price ≤ ${l.riskSoftGasPriceWei.toString()} wei · outlier grants ${l.riskOutlier ? "allowed when confirmed" : "locked"}`} hint="BSC3-069: a grant above them is an outlier; it admits BUYs only confirmed as one and with BSC_EXEC_LIVE_RISK_OUTLIER ON (exits go on)" /> : null}
      {l?.c3Rearm !== undefined ? <Line label="C3 re-arm" value={l.c3Rearm ? "ON · a new arm after each C3 BUY whose nonce the chain took" : "OFF · one C3 BUY per arm"} hint="BSC3-042: BSC_EXEC_LIVE_C3_REARM" /> : null}
      {l?.exitRepriceBps !== undefined ? <Line label="Exit re-price" value={l.exitRepriceBps > 0 ? `+${l.exitRepriceBps} bps per lost exit step, at most the grant's max gas price` : "OFF · a lost exit step waits for the operator"} hint="BSC3-030: BSC_EXEC_LIVE_EXIT_REPRICE_BPS" /> : null}
      {l?.dexExit !== undefined ? <Line label="DEX exit" value={l.dexExit ? "ON · a migrated lot exits at the Portal's quote" : "OFF · a migrated lot stays stranded for the operator"} hint="BSC3-030: BSC_EXEC_LIVE_DEX_EXIT" /> : null}
      {l?.publicExit !== undefined ? <Line label="Public exit route" value={l.publicExit ? "ON · exit steps also to the public mempool (never a BUY)" : "OFF · exit steps to the builders only"} hint="BSC3-030: BSC_EXEC_LIVE_PUBLIC_EXIT" /> : null}
      {builder !== undefined ? <Line label="Builder policy" value={builder} hint="R4: BSC_EXEC_LIVE_BUILDER_POLICY, checked every pass; while it does not hold, BUYs close and exits go on" /> : null}
      {presence !== undefined ? <Line label="Presence anchor" value={presence} hint="BSC3-001: BSC_EXEC_LIVE_PRESENCE_ANCHOR; the grant's presence digest (bsc_runtime live scope --expect-generation N) signed by its key (live grant --presence-signature); the GUI grants unsigned" /> : null}
      {l?.grant ? <>
        <Line label="Grant scope SHA-256" value={l.grant.scopeSha256} />
        <Line label="Grant risk scope SHA-256" value={l.grant.riskScopeSha256 || "—"} hint="BSC3-049: the scope a qualification's risk decision names" />
        <pre data-testid="bsc-live-grant-scope">{l.grant.scopeText}</pre>
      </> : null}
      <LiveGrantActions generation={generation} available={owner} grantable={l?.grantableSources ?? []} maxAttempts={l?.buyAttemptsTotalMax} choices={grantChoices(l)} />
    </PlacedSection> : null}
    {only.includes(3) ? <PlacedSection panel="live" section={3} records={records}>
      <Line label="Runtime mode" value={<ValueState state={l?.runtimeMode?.effective} />} hint={`requested ${l?.runtimeMode?.requested ?? "—"}${l?.runtimeMode?.blockReason ? ` · ${l.runtimeMode.blockReason}` : ""}`} />
      <table className="bsc-table" data-testid="bsc-live-scope"><tbody>
        {(l?.scope ?? []).map(s => <tr key={s.id}><td>{s.id}</td><td>requested {s.requested}</td><td>{s.effective}</td><td>{s.blockReason}</td></tr>)}
      </tbody></table>
      <ScopeLiveActions rows={l?.scope ?? []} />
      <Line label="Economic LIVE (start / stop)" value={<ValueState state={econLive.effective} />} hint="DISABLE stops every LIVE intent at its next admission" />
      <SwitchActions rows={[econLive]} testId="bsc-live-econ" action="bsc.economic.set" />
      <LiveRevokeActions generation={generation} granted={!!l?.grant} held={heldPositions(l)} />
    </PlacedSection> : null}
    {only.includes(4) ? <PlacedSection panel="live" section={4} records={records}>
      <Line label="Daily BUY attempts" value={buyLimitRow(l?.daily).summary} />
      <BuyLimitActions row={buyLimitRow(l?.daily)} />
      <Line label="Campaign BUY attempts" value={`${(l?.totalUsed ?? 0n).toString()} of ${(l?.totalLimit ?? 0n).toString()} (at most ${l?.buyAttemptsTotalMax ?? "?"})`} />
      <Line label="This grant's attempts" value={`${(l?.grantAttemptsUsed ?? 0n).toString()} of ${l?.grant?.maxAttempts ?? 0}`} />
      <Line label="Next C3 attempt" value={l?.leaseAttempt || "—"} hint="BSC3-078: the attempt the next C3 BUY takes (its lease holds it, or it was kept from a returned one)" />
      <TotalLimitActions generation={generation} available={owner} limit={l?.totalLimit ?? 0n} max={l?.buyAttemptsTotalMax} />
      <LiveCampaign campaign={l?.campaign} attempts={l?.attempts ?? []} warnings={l?.warnings ?? []} />
    </PlacedSection> : null}
    {only.includes(5) ? <PlacedSection panel="live" section={5} records={records}>
      <Line label="Spent by this grant" value={wei(l?.spentWei)} hint="The most every admitted intent may cost (input and gas)" />
      <Line label="Grant max input" value={wei(l?.grant?.maxInputWei)} />
      <Line label="Grant max spend" value={wei(l?.grant?.maxSpendWei)} />
      <Line label="Campaign spent" value={`${wei(l?.campaignSpentWei)} of ${wei(l?.campaignSpendMaxWei)}`} hint="Every grant's admissions together; each BUY with its exits stays within BSC_EXEC_LIVE_SPEND_TOTAL_WEI (0: no BUY), never raised automatically" />
    </PlacedSection> : null}
    {only.includes(6) ? <PlacedSection panel="live" section={6} records={records}>
      <Line label="LIVE path" value={<ValueState state={l?.pathState} />} hint={l?.pathReason || "the runtime's LIVE executor"} />
      <Line label="Route" value={l?.route || "—"} hint="BUILDERS (48Club/BlockRazor TLS) or LOOPBACK (127.0.0.1, nothing leaves the host)" />
      <Line label="Buyer" value={l?.buyer || "—"} />
      <Line label="Buyer balance" value={wei(l?.buyerBalanceWei)} hint="At the last read-only probe" />
      <Line label="Last refusal" value={l?.lastRefusal || "—"} />
      <div data-testid="bsc-live-path-counters">{pathRows(l).map(r => <Line key={r.label} label={r.label} value={r.value} hint={r.hint} />)}</div>
    </PlacedSection> : null}
    {only.includes(7) ? <PlacedSection panel="live" section={7} records={records}>
      {positionsTable ? <table className="bsc-table" data-testid="bsc-live-positions"><tbody>
        {(l?.positions ?? []).map(p => <tr key={p.lot}><td>{p.token}</td><td>{p.component}</td><td>{p.state}</td><td>{p.tokens}</td><td>{wei(p.inputWei)}</td><td>{p.proceedsUnknown ? "unknown (no Portal trade event)" : wei(p.proceedsWei)}</td><td>{p.pnlWei ? `${p.proceedsUnknown ? "≥ " : ""}${signedBnb(p.pnlWei)}` : "—"}</td>
          <td>{p.decisions.length ? <LiveLotActions lot={p.lot} decisions={p.decisions} version={decisions.version} exitNow={decisions.exitNow} /> : null}</td></tr>)}
      </tbody></table> : null}
      <Line label={decisions.row.label} value={decisions.row.value} hint={decisions.row.hint} />
      <Receipt testId={LIVE_LOT} />
      <div data-testid="bsc-live-exit-deciders">{exitDeciderRows(l).map(r => <Line key={r.label} label={r.label} value={r.value} hint={r.hint} />)}</div>
    </PlacedSection> : null}
    {only.includes(8) ? <PlacedSection panel="live" section={8} records={records}>
      <Line label="Released" value={`${(l?.released ?? 0n).toString()} (${(l?.unknown ?? 0n).toString()} UNKNOWN)`} hint="Transactions whose bytes may have left; never sent again" />
      <table className="bsc-table" data-testid="bsc-live-transactions"><tbody>
        {(l?.transactions ?? []).map(t => <tr key={t.txHash}><td>{t.intent}</td><td>{t.txHash}</td><td>nonce {t.nonce.toString()}</td><td>{t.state}</td><td>{t.blockNumber?.toString() ?? ""}</td><td>{t.source}</td>
          <td>{t.burnable || t.burnRequested ? <LiveBurnAction tx={t.txHash} nonce={t.nonce} version={decisions.version} burn={decisions.burn} requested={t.burnRequested} /> : null}</td></tr>)}
      </tbody></table>
      <Receipt testId={LIVE_BURN} />
      <div data-testid="bsc-live-outcomes">{outcomeRows(l).map(r => <Line key={r.label} label={r.label} value={r.value} hint={r.hint} />)}</div>
      <Line label={recovery.row.label} value={recovery.row.value} hint={recovery.row.hint} />
      {recovery.pending.length ? <table className="bsc-table" data-testid="bsc-live-recovery-pending"><tbody>
        {recovery.pending.map(([tx, why]) => <tr key={tx}><td>{tx}</td><td>{why}</td></tr>)}
      </tbody></table> : null}
    </PlacedSection> : null}
    {only.includes(9) ? <PlacedSection panel="live" section={9} records={records}>
      <Line label="Realized PnL" value={l ? signedBnb(l.realizedPnlWei) : "—"} hint="Proceeds − input − all gas of closed positions; failed BUYs' gas" />
      <Line label="Gas paid" value={wei(l?.gasWei)} />
      {l?.ledgerError ? <Line label="Runtime ledger read" value={l.ledgerError} hint="The runtime could not read the LIVE ledger: its counters above do not count" /> : null}
    </PlacedSection> : null}
    <Notice>LIVE is the native owner's decision: effective only while every condition above holds, for the confirmed scope until its expiry, in this boot session. Every LIVE intent is admitted again at its dispatch (switches, daily and campaign limits, the grant's budget, the verified qualification); nothing is retried, and an intent whose handoff did not complete stays UNKNOWN until the chain decides.</Notice>
  </>;
}
