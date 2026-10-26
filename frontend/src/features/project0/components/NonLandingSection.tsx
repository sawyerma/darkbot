import type { TopicStatus } from "@/core/realtime";
import { NON_LANDING_CLASSES, unusableByReason, type WalletsView } from "../model/owner";
import { DetailRow, Notice, Section } from "./PanelPrimitives";

const dash = (v: bigint | null) => (v == null ? "—" : v.toString());

/**
 * DEC-03 (N-2): the non-landing status of the held FE-04 attempt or its latest exit, beside the
 * frozen Wallets panel — exactly one of the five result classes, the unusable B sources per
 * reason, C's result, the hold duration and the latest read's calls and bytes. Read-only: no
 * switch and no manual resolution (N-2 §5). Without a held FE-04 journal there is no subject and
 * nothing is shown.
 */
export function NonLandingSection({ wallets, topicStatus }: { wallets: WalletsView | null; topicStatus: TopicStatus }) {
  const n = wallets?.nonLanding ?? null;
  if (!n) return null;
  const live = topicStatus === "LIVE";
  const unusable = unusableByReason(n.sources);
  const sightings = n.sources.filter(s => s.class === "SIGHTING").map(s => s.operator);
  return <Section title="Non-landing · N-2" aside={<span className="p0-badge">{topicStatus}</span>}>
    {!live ? <p className="p0-quiet">Last reported status ({topicStatus}); not current.</p> : null}
    {n.alarm ? <Notice><span data-testid="p0-non-landing-alarm">Alarm: contradiction open — holds stay, read again</span></Notice> : null}
    <DetailRow title={`Result · ${n.subject === "EXIT" ? "exit" : "attempt"}`}
      value={<span data-testid="p0-non-landing-class">{NON_LANDING_CLASSES[n.resultClass] ?? n.resultClass}</span>}/>
    <DetailRow title="C · canonical walk" value={n.cResult || "—"}/>
    <DetailRow title="B · usable / sources" value={`${n.sources.filter(s => s.class === "USABLE").length} / ${n.sources.length}`}/>
    {sightings.length ? <DetailRow title="B · sighting" value={sightings.join(", ")}/> : null}
    {unusable.map(u => <DetailRow key={u.reason} title={`B unusable · ${u.reason}`} value={String(u.count)}/>)}
    <DetailRow title="Hold duration" value={n.holdMs == null ? "—" : `${(Number(n.holdMs) / 1000).toFixed(1)} s`}/>
    <DetailRow title="Calls · bytes (latest read)" value={`${dash(n.calls)} · ${dash(n.bytes)}`}/>
    <DetailRow title="Reads this run" value={n.reads.toString()}/>
  </Section>;
}
