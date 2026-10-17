import { Line, Section } from "@/shared/ui/graphite/Rows";
import { Notice } from "@/shared/ui/graphite/Notice";

// Analytics zeigt nur echte, versiegelte BSC-Ergebnisse. Reports werden in F7 paginiert
// per expliziter Nutzeraktion geladen (historyClient, TanStack Query), nie gepollt.
export function AnalyticsPanel() {
  return (
    <>
      <Section title="Qualified results">
        <Line label="Qualified runs" value="—" />
        <Line label="Last sealed report" value="—" />
        <Line label="Offline replay" value="—" hint="shown as OFFLINE/REPLAY, never counted as profit" />
      </Section>
      <Notice>No qualified BSC results are available. Nothing is simulated here.</Notice>
    </>
  );
}
