import { Line, Metrics, Section, orDash } from "@/shared/ui/graphite/Rows";
import { Notice } from "@/shared/ui/graphite/Notice";
import { formatVersion, yesNo } from "../model/format";
import { selectStatus, useBscStore } from "../store/bscStore";
import type { BscTab } from "../BscPage";

export function OverviewPanel({ goto }: { goto: (t: BscTab) => void }) {
  const { projection: s, meta, status } = useBscStore(selectStatus);
  const g = s?.gates;

  return (
    <>
      <Section>
        <Metrics
          items={[
            { label: "G0.6a", value: g?.g06a ?? "—" },
            { label: "G0.7", value: g?.g07 ?? "—" },
            { label: "G0.8", value: g?.g08 ?? "—" },
            { label: "Portal", value: orDash(s?.portal.version) },
          ]}
        />
      </Section>

      <Section title="Readiness">
        <Line label="Guard" value={s?.guardState ?? "—"} hint={s?.liveBlocked == null ? undefined : `LIVE blocked: ${yesNo(s.liveBlocked)}`} />
        <Line label="Source quality" value={status} hint={status === "LIVE" ? "Status feed only; not execution authorization" : "Cached values may be stale"} />
        <Line label="Preflight" value={s?.preflight ?? "—"} />
        <Line label="Builder probe" value={s?.builder.probe ?? "—"} hint={orDash(s?.builder.version)} />
        <Line label="Portal drift" value={yesNo(s?.portal.drift ?? null)} hint={s?.portal.frozenVersion ? `frozen ${s.portal.frozenVersion}` : undefined} />
        <Line label="Source witness" value={orDash(s?.sourceWitness)} />
        <Line label="Config version" value={formatVersion(s?.configVersion ?? null)} hint={meta ? `gen ${meta.generation} · seq ${meta.seq}` : undefined} />
      </Section>

      <Section title="Commands">
        <Line label="Pending" value={s ? String(s.pendingCommands.length) : "—"} />
        <Line label="Recent results" value={s ? String(s.recentCommandResults.length) : "—"} />
        <div className="g-actions">
          <button type="button" className="g-secondary" onClick={() => goto("launch")}>Launch / BUY</button>
          <button type="button" className="g-secondary" onClick={() => goto("monitoring")}>Monitoring</button>
        </div>
      </Section>

      <Notice>
        Read-only. No backend approval is assumed. A connected status feed never authorizes trading; no BUY command exists in this UI.
      </Notice>
    </>
  );
}
