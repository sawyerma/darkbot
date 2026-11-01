import { Line } from "@/shared/ui/graphite/Rows";
import { Notice } from "@/shared/ui/graphite/Notice";
import { ValueState } from "./EvidenceRows";
import { CAMPAIGN_SCOPE, NO_EVALUATION, T0_UNKNOWN, attemptCells, campaignRows } from "../model/live";
import type { LiveAttemptView, LiveCampaignView } from "../model/types";

const ATTEMPT_HEADER = ["#", "Tx", "Kind · cell", "State", "Verdict", "Components", "15-s window", "vs first external BUY", "Local timing", "Counters", "Adapters", "Reasons"];
const CELL_HEADER = ["Cell", "Enabled", "Attempts", "Passed", "Failed", "Undecided", "Duration", "Verdict"];
const atMost = (v: bigint, min: bigint | undefined, unit = "") => `${v.toString()}${unit}${min === undefined ? "" : ` / min ${min.toString()}${unit}`}`;

/** REV.1.0 R2b: the campaign verdict and its attempts exactly as `bsc_runtime live attempts`
 *  recorded them (LIVE section 4); the evaluator decides, nothing is computed here. */
export function LiveCampaign({ campaign: c, attempts }: { campaign: LiveCampaignView | undefined; attempts: readonly LiveAttemptView[] }) {
  return <>
    {c ? <>
      <Line label="Campaign verdict" value={<ValueState state={c.verdict} />} hint={c.reasons.length ? c.reasons.join(" · ") : "no reason recorded"} />
      <Line label="Release evidence" value={<ValueState state={c.releaseEvidence} />} hint={CAMPAIGN_SCOPE} />
      <div data-testid="bsc-live-campaign-counts">{campaignRows(c).map(r => <Line key={r.label} label={r.label} value={r.value} hint={r.hint} />)}</div>
      <table className="bsc-table" data-testid="bsc-live-campaign-cells">
        <thead><tr>{CELL_HEADER.map(h => <th key={h}>{h}</th>)}</tr></thead>
        <tbody>{c.cells.map(x => <tr key={x.cell}><td>{x.cell}</td><td>{String(x.enabled)}</td><td>{atMost(x.attempts, x.minAttempts)}</td><td>{x.passed.toString()}</td><td>{x.failed.toString()}</td><td>{x.undecided.toString()}</td><td>{atMost(x.durationS, x.minDurationS, " s")}</td><td>{x.verdict}</td></tr>)}</tbody>
      </table>
      <Line label="Manifest · precommit" value={`${c.manifestSha256 || "—"} · ${c.precommitSha256 || "—"}`} hint={`build ${c.buildSha256 || "—"}`} />
      <Line label="Evaluated" value={c.evaluatedUnixMs > 0n ? new Date(Number(c.evaluatedUnixMs)).toISOString() : "UNKNOWN"} hint={`verdict record SHA-256 ${c.recordSha256}`} />
    </> : <Line label="Campaign verdict" value={<span data-testid="bsc-live-campaign-none">{NO_EVALUATION}</span>} hint="bsc_runtime live attempts has written no campaign verdict yet" />}
    <Notice>{T0_UNKNOWN}</Notice>
    {attempts.length ? <table className="bsc-table" data-testid="bsc-live-attempts">
      <thead><tr>{ATTEMPT_HEADER.map(h => <th key={h}>{h}</th>)}</tr></thead>
      <tbody>{attempts.map(a => <tr key={a.tx}>{attemptCells(a).map((v, i) => <td key={ATTEMPT_HEADER[i]}>{v}</td>)}</tr>)}</tbody>
    </table> : <Line label="Attempts" value="—" hint="no attempt record" />}
  </>;
}
