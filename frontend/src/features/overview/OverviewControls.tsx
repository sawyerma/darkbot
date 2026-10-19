import { useId } from "react";
import type { DisabledControl, ProcessIndicator } from "./model/lifecycle";

/** A real, inert button: disabled by type, with its reason as tooltip and for screen readers. */
export function DisabledAction({ label, control, testId }: { label: string; control: DisabledControl; testId: string }) {
  const reasonId = useId();
  return (
    <>
      <span className="ov-disabled-action" title={control.reason}>
        <button type="button" className="ov-btn" disabled
          aria-describedby={reasonId} data-testid={testId}>{label}</button>
      </span>
      <span id={reasonId} className="ov-sr-only">{control.reason}</span>
    </>
  );
}

export function ProcessLine({ indicator, testId }: { indicator: ProcessIndicator; testId: string }) {
  return (
    <div className="ov-process" data-state={indicator.state} title={indicator.reason} data-testid={testId}>
      <span className="ov-dot" aria-hidden="true" />
      <span>{indicator.label}</span>
    </div>
  );
}
