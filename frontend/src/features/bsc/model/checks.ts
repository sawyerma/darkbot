/** One native readiness row as the reporter projects it: `STATUS · detail`. */
export interface CheckRow { readonly name: string; readonly value: string }

/** The native status of one readiness check, or undefined when the native record has no such row. */
export const checkStatus = (checks: readonly CheckRow[] | undefined, name: string): string | undefined =>
  checks?.find(c => c.name === name)?.value.split(" · ")[0];
