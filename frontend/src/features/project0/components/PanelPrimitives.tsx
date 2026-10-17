import type { ReactNode } from "react";

export function Section({ title, aside, children }: {
  title?: string; aside?: ReactNode; children: ReactNode;
}) {
  return <section className="p0-section">
    {title ? <div className="p0-row p0-section-title"><h2>{title}</h2>{aside}</div> : null}
    {children}
  </section>;
}
export function DetailRow({ title, value }: { title: string; value: ReactNode }) {
  return <div className="p0-line"><span>{title}</span><span className="p0-right">{value}</span></div>;
}
export function TabStrip<T extends string>({ options, value, onChange, label, minor = false }: {
  options: ReadonlyArray<{ value: T; label: string }>;
  value: T; onChange: (value: T) => void; label: string; minor?: boolean;
}) {
  return <div className={minor ? "p0-subtabs" : "p0-tabs"} role="tablist" aria-label={label}>
    {options.map(option => <button key={option.value} type="button" role="tab"
      aria-selected={value === option.value} className={value === option.value ? "p0-selected" : ""}
      onClick={() => onChange(option.value)}>{option.label}</button>)}
  </div>;
}
export function UnknownMetric({ label, unit }: { label: string; unit: string }) {
  return <div className="p0-metric"><span>{label}</span><b>— <small>{unit}</small></b></div>;
}
export function Notice({ children }: { children: ReactNode }) {
  return <p className="p0-note" role="status">{children}</p>;
}
