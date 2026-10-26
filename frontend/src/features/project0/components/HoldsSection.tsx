import type { TopicStatus } from "@/core/realtime";
import type { HoldFlowView, WalletsView } from "../model/owner";
import { usdMicroDisplay } from "../model/format";
import { DetailRow, Notice, Section } from "./PanelPrimitives";

/** A's unknown-price hold is the u64 maximum: never a value. */
const U64_MAX = 18446744073709551615n;
const usd = (micro: bigint | null) =>
  micro == null ? "— USD" : micro === U64_MAX ? "unknown (no price)" : usdMicroDisplay(micro.toString());

function flowText(f: HoldFlowView, settled: boolean): string {
  const keeps = f.residualMax == null ? "unbounded (value-bounded)" : f.residualMax.toString();
  const actual = f.actual != null ? f.actual.toString() : settled ? "beyond int64" : "—";
  return `reserve ${f.reserve} · keeps ≤ ${keeps} · finalized ${actual}${f.underestimated ? " · UNDERESTIMATED" : ""}`;
}

/**
 * DEC-04 (V7): the B hold mode beside the frozen Wallets panel — requested and effective (ACTIVE
 * only on a recorded request with the user's N and the evidence), the journals' evidence, the
 * capital and inventory Safety binds — and the held attempt's holds: A (wallet reservation,
 * inventory value) and B (exact per asset, or unqualified and why: A applies), the value Safety
 * reserved, each prediction against the finalized flows and the finalized inventory mark.
 * Read-only: the mode is requested on the Liquidator tab. Nothing is shown without the owner's
 * projection.
 */
export function HoldsSection({ wallets, topicStatus }: { wallets: WalletsView | null; topicStatus: TopicStatus }) {
  const h = wallets?.holdB ?? null;
  if (!h) return null;
  const a = h.attempt;
  const under = [...(a?.flows ?? []), ...(a?.aFlows ?? [])].filter((f) => f.underestimated).map((f) => f.asset);
  return <Section title="Holds · A / B" aside={<span className="p0-badge">{topicStatus}</span>}>
    {topicStatus !== "LIVE" ? <p className="p0-quiet">Last reported holds ({topicStatus}); not current.</p> : null}
    <DetailRow title="B hold mode · requested / effective"
      value={<span data-testid="p0-hold-mode">{`${h.requested} / ${h.effective}`}</span>}/>
    {h.blockReason ? <DetailRow title="Not effective because" value={h.blockReason}/> : null}
    <DetailRow title="N (B-8) · evidence exact / underestimated / unqualified"
      value={`${h.n == null ? "undecided" : h.n.toString()} · ${h.evidenceExact} / ${h.evidenceUnderestimated} / ${h.evidenceUnqualified}`}/>
    <DetailRow title="Bound capital · inventory holds" value={`${usd(h.capitalInUseUsdMicro)} · ${usd(h.inventoryReservedUsdMicro)}`}/>
    {a ? <>
      <DetailRow title={`Attempt · admitted in ${a.mode}`} value={`A inventory ${usd(a.aInventoryUsdMicro)} · B ${a.bClass === "EXACT" ? `${usd(a.bInventoryUsdMicro)} (${a.bKind})` : `UNQUALIFIED: ${a.bReason} (A applies)`} · reserved ${usd(a.reservedInventoryUsdMicro)}`}/>
      {a.aFlows.map((f) => <DetailRow key={`a-${f.asset}`} title={`A · ${f.asset}`} value={flowText(f, a.settled)}/>)}
      {a.flows.map((f) => <DetailRow key={`b-${f.asset}`} title={`B · ${f.asset}`} value={flowText(f, a.settled)}/>)}
      {a.settled ? <DetailRow title="Finalized inventory mark" value={usd(a.actualInventoryUsdMicro)}/> : null}
      {under.length ? <Notice><span data-testid="p0-hold-underestimated">Underestimated {[...new Set(under)].join(", ")}: ACTIVE stays blocked</span></Notice> : null}
    </> : <p className="p0-quiet">No held attempt.</p>}
  </Section>;
}
