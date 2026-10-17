import type { ReactNode } from "react";
import "./graphite.css";

// Fachfreier Rahmen für Overview/BSC im Graphite-Look. Project 0 behält seinen eigenen
// p0-graphite-Rahmen (frozen) und verwendet diese Komponente nicht.

interface Props {
  title: string;
  subtitle?: string;
  right?: ReactNode;
  children: ReactNode;
  footer?: ReactNode;
}

export function GraphiteShell({ title, subtitle, right, children, footer }: Props) {
  return (
    <div className="g-shell">
      <section className="g-panel">
        <header className="g-heading">
          <div>
            <h1>{title}</h1>
            {subtitle ? <p>{subtitle}</p> : null}
          </div>
          {right}
        </header>
        {children}
        {footer ? <div className="g-footer">{footer}</div> : null}
      </section>
    </div>
  );
}
