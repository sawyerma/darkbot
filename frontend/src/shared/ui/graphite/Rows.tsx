import type { ReactNode } from "react";

// Darstellungsbausteine ohne Fachlogik. Unbekannt ist "—", nie 0.
export const DASH = "—";

export function orDash(value: string | number | bigint | null | undefined): string {
  if (value === null || value === undefined || value === "") return DASH;
  return String(value);
}

export function Section({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <section className="g-section">
      {title ? <h2 className="g-section-title">{title}</h2> : null}
      {children}
    </section>
  );
}

export function Line({ label, value, hint }: { label: ReactNode; value: ReactNode; hint?: ReactNode }) {
  return (
    <div className="g-line">
      <span>{label}</span>
      <span className="g-right">
        {value}
        {hint ? <small className="g-quiet"> {hint}</small> : null}
      </span>
    </div>
  );
}

export interface MetricItem {
  label: string;
  value: string;
  unit?: string;
}

export function Metrics({ items }: { items: readonly MetricItem[] }) {
  return (
    <div className="g-metrics">
      {items.map((m) => (
        <div className="g-metric" key={m.label}>
          <span>{m.label}</span>
          <b>{m.value}</b>
          {m.unit ? <small> {m.unit}</small> : null}
        </div>
      ))}
    </div>
  );
}
