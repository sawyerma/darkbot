import { TopicId } from "@/core/realtime";
import { useTopic } from "@/core/realtime/hooks";
import { Line, Section } from "@/shared/ui/graphite/Rows";
import { Notice, StatusBadge } from "@/shared/ui/graphite/Notice";
import { formatBps, formatVersion, formatWei } from "../model/format";
import { selectLaunch, useBscStore } from "../store/bscStore";

export function LaunchBuyPanel() {
  const topicStatus = useTopic(TopicId.BSC_LAUNCH_STATE);
  const { projection } = useBscStore(selectLaunch);
  const l = topicStatus === "LIVE" ? projection : null;

  return (
    <>
      <Section title="Approved plan (read-only)">
        <Line label="Plan version" value={formatVersion(l?.planVersion ?? null)} />
        <Line label="Budget" value={formatWei(l?.budgetWei ?? null)} />
        <Line label="Max fee" value={formatBps(l?.maxFeeBps ?? null)} />
        <Line label="Slippage" value={formatBps(l?.slippageBps ?? null)} />
        <Line label="Gate verdict" value={l?.gateVerdict ?? "—"} />
        <Line label="Transport" value={<StatusBadge state={topicStatus === "LIVE" ? "READY" : topicStatus === "DETACHED" ? "DETACHED" : topicStatus === "UNAVAILABLE" || topicStatus === "ERROR" ? "BLOCKED" : "NOT_CONNECTED"} />} />
      </Section>

      <Section title="Locked actions">
        {l && l.lockedActions.length > 0 ? (
          l.lockedActions.map((a) => <Line key={a} label={a} value="Locked" />)
        ) : (
          <Line label="All value-bearing actions" value="Locked" hint="until G0.7 is executed and authorized" />
        )}
      </Section>

      <Notice>
        No BUY control is rendered here by design. Commands arrive in phase F6 behind the BSC gate; the engine alone decides.
      </Notice>
    </>
  );
}
