import type { TopicStatus } from "@/core/realtime";
import type { WalletsView } from "../model/owner";
import { DetailRow, Notice, Section } from "./PanelPrimitives";

const usd = (micro: bigint | null) => (micro == null ? "—" : `${(Number(micro) / 1e6).toFixed(6)} USD`);

/**
 * DEC-04 (V7): the B hold mode beside the frozen Wallets panel — requested and effective (ACTIVE
 * only with the user's N and the evidence), the journals' evidence, the capital and inventory
 * Safety binds — and the held attempt's A hold (wallet reservation, inventory value), B (exact per
 * asset, or unqualified and why: A applies) against the finalized flows. Read-only: the mode is
 * requested on the Liquidator tab. Nothing is shown without the owner's projection.
 */
export function HoldsSection({ wallets, topicStatus }: { wallets: WalletsView | null; topicStatus: TopicStatus }) {
  const h = wallets?.holdB ?? null;
  if (!h) return null;
  const a = h.attempt;
  const under = a?.flows.filter((f) => f.underestimated).map((f) => f.asset) ?? [];
  return <Section title="Holds · A / B" aside={<span className="p0-badge">{topicStatus}</span>}>
    {topicStatus !== "LIVE" ? <p className="p0-quiet">Last reported holds ({topicStatus}); not current.</p> : null}
    <DetailRow title="B hold mode · requested / effective"
      value={<span data-testid="p0-hold-mode">{`${h.requested} / ${h.effective}`}</span>}/>
    {h.blockReason ? <DetailRow title="Not effective because" value={h.blockReason}/> : null}
    <DetailRow title="N (B-8) · evidence exact / underestimated / unqualified"
      value={`${h.n == null ? "undecided" : h.n.toString()} · ${h.evidenceExact} / ${h.evidenceUnderestimated} / ${h.evidenceUnqualified}`}/>
    <DetailRow title="Bound capital · inventory holds" value={`${usd(h.capitalInUseUsdMicro)} · ${usd(h.inventoryReservedUsdMicro)}`}/>
    {a ? <>
      <DetailRow title={`Attempt · admitted in ${a.mode}`} value={`A inventory ${usd(a.aInventoryUsdMicro)} · B ${a.bClass === "EXACT" ? `${usd(a.bInventoryUsdMicro)} (${a.bKind})` : `UNQUALIFIED: ${a.bReason} (A applies)`}`}/>
      {a.aWallet.map((w) => <DetailRow key={`a-${w.asset}`} title={`A reserve · ${w.asset}`} value={w.units.toString()}/>)}
      {a.flows.map((f) => <DetailRow key={`b-${f.asset}`} title={`B · ${f.asset}`}
        value={`reserve ${f.reserve} · keeps ≤ ${f.residualMax} · finalized ${f.actual == null ? "—" : f.actual.toString()}${f.underestimated ? " · UNDERESTIMATED" : ""}`}/>)}
      {under.length ? <Notice><span data-testid="p0-hold-underestimated">B underestimated {under.join(", ")}: ACTIVE stays blocked</span></Notice> : null}
    </> : <p className="p0-quiet">No held attempt.</p>}
  </Section>;
}
