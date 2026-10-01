import { useRealtimeStatus } from "@/core/realtime/useRealtimeStatus";

const projects = [
  ["BSC", "Birth Entry / Gate0"],
  ["Project 0", "Solana liquidator"],
  ["Solana", "Pump.fun / shared Solana"],
] as const;

export default function OverviewPage() {
  const realtime = useRealtimeStatus();
  return (
    <section className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold">DarkBot Overview</h1>
        <p className="text-sm text-muted-foreground">One frontend, three isolated backend domains.</p>
      </div>
      <div className="grid gap-4 md:grid-cols-3">
        {projects.map(([name, detail]) => (
          <div key={name} className="rounded-lg border bg-card p-4">
            <div className="font-medium">{name}</div><div className="text-sm text-muted-foreground">{detail}</div>
          </div>
        ))}
      </div>
      <div className="rounded-lg border bg-card p-4 text-sm">Realtime control-plane: <strong>{realtime}</strong></div>
    </section>
  );
}
