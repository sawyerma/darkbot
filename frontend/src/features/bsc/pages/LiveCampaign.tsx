import { Line } from "@/shared/ui/graphite/Rows";
import { Notice } from "@/shared/ui/graphite/Notice";
import { ValueState } from "./EvidenceRows";
import { ATTEMPT_COLUMNS, CAMPAIGN_SCOPE, COUNTERS_MEANING, LOCAL_NOT_T0, NO_EVALUATION, T0_UNKNOWN, VERDICT_INVALID, attemptCells, campaignBeyondState, campaignRows, evaluationOf, unixMs } from "../model/live";
import type { LiveAttemptView, LiveCampaignView } from "../model/types";

const CELL_HEADER = ["Cell", "Enabled", "Attempts", "Passed", "Failed", "Undecided", "Duration", "Verdict"];
const atMost = (v: bigint, min: bigint | undefined, unit = "") => `${v.toString()}${unit}${min === undefined ? "" : ` / min ${min.toString()}${unit}`}`;

/** REV.1.0 R2b: the campaign verdict and its attempts exactly as `bsc_runtime live attempts`
 *  recorded them (LIVE section 4); the evaluator decides, nothing is computed here. A verdict
 *  record the reporter could not read is INVALID, never "keine Bewertung", and so is a section
 *  beyond what the state carries (its records counted, none shown). */
export function LiveCampaign({ campaign: c, attempts, warnings }: { campaign: LiveCampaignView | undefined; attempts: readonly LiveAttemptView[]; warnings: readonly string[] }) {
  // T0 stays unknown unless every attempt's record says it is the producer's exact time.
  const t0Exact = attempts.length > 0 && attempts.every(a => a.t0Exact);
  const evaluation = evaluationOf(c, attempts);
  const beyond = campaignBeyondState(warnings);
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
      <Line label="Evaluated" value={c.evaluatedUnixMs > 0n ? unixMs(c.evaluatedUnixMs) : "UNKNOWN"} hint={`verdict record SHA-256 ${c.recordSha256}`} />
    </> : beyond !== undefined
      ? <Line label="Campaign verdict" value={<span data-testid="bsc-live-campaign-invalid"><ValueState state="INVALID" /></span>} hint={`${beyond}: the campaign section is beyond what the rest of the LIVE tab leaves of the state bound; not shown`} />
    : warnings.includes(VERDICT_INVALID)
      ? <Line label="Campaign verdict" value={<span data-testid="bsc-live-campaign-invalid"><ValueState state="INVALID" /></span>} hint={`${VERDICT_INVALID}: the verdict record is unreadable, a link, torn, of another schema or beyond its share of the state; no value`} />
      : <Line label="Campaign verdict" value={<span data-testid="bsc-live-campaign-none">{NO_EVALUATION}</span>} hint="bsc_runtime live attempts has written no campaign verdict yet" />}
    <Notice>{t0Exact ? LOCAL_NOT_T0 : `${T0_UNKNOWN}. ${LOCAL_NOT_T0}`}</Notice>
    {attempts.length ? <>
      <table className="bsc-table" data-testid="bsc-live-attempts">
        <thead><tr>{ATTEMPT_COLUMNS.map(h => <th key={h}>{h}</th>)}</tr></thead>
        <tbody>{attempts.map(a => <tr key={a.tx}>{attemptCells(a, evaluation).map((v, i) => <td key={ATTEMPT_COLUMNS[i]}>{v}</td>)}</tr>)}</tbody>
      </table>
      <Line label="Counters" value="counts up to each attempt" hint={COUNTERS_MEANING} />
    </> : beyond !== undefined
      ? <Line label="Attempts" value={<span data-testid="bsc-live-attempts-beyond"><ValueState state="INVALID" /></span>} hint={`${beyond}: not shown`} />
      : <Line label="Attempts" value="—" hint="no attempt record" />}
  </>;
}
