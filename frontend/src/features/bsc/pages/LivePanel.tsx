import { TopicId } from "@/core/realtime";
import { useTopic } from "@/core/realtime/hooks";
import { Line } from "@/shared/ui/graphite/Rows";
import { Notice } from "@/shared/ui/graphite/Notice";
import { selectLive, useBscStore } from "../store/bscStore";
import { ValueState } from "./EvidenceRows";
import { PlacedSection } from "./PlacedSection";
import { useNativeRecords } from "./useNativeRecords";
import { conditionRows, signedBnb } from "../model/live";
import { buyLimitRow, switchRow } from "../model/switches";
import { formatNative } from "../model/format";
import { BuyLimitActions } from "../BuyLimitActions";
import { SwitchActions } from "../SwitchActions";
import { LiveGrantActions, LiveRevokeActions, ScopeLiveActions, TotalLimitActions } from "../LiveActions";

const wei = (v: string | undefined) => v === undefined || v === "" ? "—" : /^[0-9]+$/.test(v) ? formatNative(v, "wei") : v;

/** The BSC LIVE tab (plan 28.10.): the owner's gate with every precondition, the grant, the
 *  activation, attempts, spend, the runtime's LIVE path, positions, receipts and PnL. */
export function LivePanel() {
  const quality = useTopic(TopicId.BSC_LIVE_STATE);
  const { projection: l } = useBscStore(selectLive);
  const records = useNativeRecords();
  const conditions = conditionRows(l);
  const generation = l?.generation ?? 0n;
  const owner = !!l && l.gate !== "UNKNOWN";
  const econLive = switchRow("BSC_ECON_LIVE", l?.economicLive);
  return <>
    <Line label="LIVE source quality" value={<ValueState state={quality} />} hint="bsc.live.state transport" />
    <PlacedSection panel="live" section={1} records={records}>
      <Line label="LIVE gate" value={<ValueState state={l?.gate} />} hint={l?.blockers.length ? `first: ${l.blockers[0]}` : "every condition holds"} />
      <table className="bsc-table" data-testid="bsc-live-conditions"><tbody>
        {conditions.map(c => <tr key={c.id} data-met={c.met}><td>{c.met ? "✓" : "✗"}</td><td>{c.label}</td><td>{c.reason}</td><td className="g-muted">{c.met ? "" : c.where}</td></tr>)}
      </tbody></table>
      <Line label="LIVE qualification" value={<ValueState state={l?.qualification} />} hint={l?.qualificationDetail || "bsc/config/live_qualification.dbrec, verified against its evidence"} />
      {l?.warnings.length ? <Line label="Warnings" value={l.warnings.join(", ")} /> : null}
    </PlacedSection>
    <PlacedSection panel="live" section={2} records={records}>
      {l?.grant ? <>
        <Line label="Grant" value={`${l.grant.strategies} · ${l.grant.venue}${l.grant.exitOnly ? " · EXIT-ONLY" : ""}`} hint={`request ${l.grant.requestId} @ generation ${l.grant.setGeneration.toString()}`} />
        <Line label="Grant expiry" value={new Date(Number(l.grant.expiresUnixMs)).toISOString()} />
        <Line label="Grant scope SHA-256" value={l.grant.scopeSha256} />
        <pre data-testid="bsc-live-grant-scope">{l.grant.scopeText}</pre>
      </> : <Line label="Grant" value="NONE" hint="LIVE needs the operator's grant of one confirmed scope" />}
      <LiveGrantActions generation={generation} available={owner} />
    </PlacedSection>
    <PlacedSection panel="live" section={3} records={records}>
      <Line label="Runtime mode" value={<ValueState state={l?.runtimeMode?.effective} />} hint={`requested ${l?.runtimeMode?.requested ?? "—"}${l?.runtimeMode?.blockReason ? ` · ${l.runtimeMode.blockReason}` : ""}`} />
      <table className="bsc-table" data-testid="bsc-live-scope"><tbody>
        {(l?.scope ?? []).map(s => <tr key={s.id}><td>{s.id}</td><td>requested {s.requested}</td><td>{s.effective}</td><td>{s.blockReason}</td></tr>)}
      </tbody></table>
      <ScopeLiveActions rows={l?.scope ?? []} />
      <Line label="Economic LIVE (start / stop)" value={<ValueState state={econLive.effective} />} hint="DISABLE stops every LIVE intent at its next admission" />
      <SwitchActions rows={[econLive]} testId="bsc-live-econ" action="bsc.economic.set" />
      <LiveRevokeActions generation={generation} granted={!!l?.grant} />
    </PlacedSection>
    <PlacedSection panel="live" section={4} records={records}>
      <Line label="Daily BUY attempts" value={buyLimitRow(l?.daily).summary} />
      <BuyLimitActions row={buyLimitRow(l?.daily)} />
      <Line label="Campaign BUY attempts" value={`${(l?.totalUsed ?? 0n).toString()} of ${(l?.totalLimit ?? 0n).toString()} (at most 10)`} />
      <Line label="This grant's attempts" value={`${(l?.grantAttemptsUsed ?? 0n).toString()} of ${l?.grant?.maxAttempts ?? 0}`} />
      <TotalLimitActions generation={generation} available={owner} limit={l?.totalLimit ?? 0n} />
    </PlacedSection>
    <PlacedSection panel="live" section={5} records={records}>
      <Line label="Spent by this grant" value={wei(l?.spentWei)} hint="The most every admitted intent may cost (input and gas)" />
      <Line label="Grant max input" value={wei(l?.grant?.maxInputWei)} />
      <Line label="Grant max spend" value={wei(l?.grant?.maxSpendWei)} />
    </PlacedSection>
    <PlacedSection panel="live" section={6} records={records}>
      <Line label="LIVE path" value={<ValueState state={l?.pathState} />} hint={l?.pathReason || "the runtime's LIVE executor"} />
      <Line label="Route" value={l?.route || "—"} hint="BUILDERS (48Club/BlockRazor TLS) or LOOPBACK (127.0.0.1, nothing leaves the host)" />
      <Line label="Buyer" value={l?.buyer || "—"} />
      <Line label="Buyer balance" value={wei(l?.buyerBalanceWei)} hint="At the last read-only probe" />
      <Line label="Last refusal" value={l?.lastRefusal || "—"} />
    </PlacedSection>
    <PlacedSection panel="live" section={7} records={records}>
      <table className="bsc-table" data-testid="bsc-live-positions"><tbody>
        {(l?.positions ?? []).map(p => <tr key={p.lot}><td>{p.token}</td><td>{p.component}</td><td>{p.state}</td><td>{p.tokens}</td><td>{wei(p.inputWei)}</td><td>{wei(p.proceedsWei)}</td><td>{p.pnlWei ? signedBnb(p.pnlWei) : "—"}</td></tr>)}
      </tbody></table>
    </PlacedSection>
    <PlacedSection panel="live" section={8} records={records}>
      <Line label="Released" value={`${(l?.released ?? 0n).toString()} (${(l?.unknown ?? 0n).toString()} UNKNOWN)`} hint="Transactions whose bytes may have left; never sent again" />
      <table className="bsc-table" data-testid="bsc-live-transactions"><tbody>
        {(l?.transactions ?? []).map(t => <tr key={t.txHash}><td>{t.intent}</td><td>{t.txHash}</td><td>nonce {t.nonce.toString()}</td><td>{t.state}</td><td>{t.blockNumber?.toString() ?? ""}</td></tr>)}
      </tbody></table>
    </PlacedSection>
    <PlacedSection panel="live" section={9} records={records}>
      <Line label="Realized PnL" value={l ? signedBnb(l.realizedPnlWei) : "—"} hint="Proceeds − input − all gas of closed positions; failed BUYs' gas" />
      <Line label="Gas paid" value={wei(l?.gasWei)} />
    </PlacedSection>
    <Notice>LIVE is the native owner's decision: effective only while every condition above holds, for the confirmed scope until its expiry, in this boot session. Every LIVE intent is admitted again at its dispatch (switches, daily and campaign limits, the grant's budget, the verified qualification); nothing is retried, and an intent whose handoff did not complete stays UNKNOWN until the chain decides.</Notice>
  </>;
}
