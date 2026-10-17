// View-Modell der BSC-Oberfläche. Alle Fachfelder sind nullable: null = UNKNOWN ("—").
// Feldbindung an die Rust-Owner (LiveGuardContext, ContractSnapshot, V4-Guard, G0.7/G0.8,
// DBREC) erfolgt in F0-VAR über SOURCE_VARIABLE_MATRIX.md; bis dahin sind das SOLL-Felder.
// Kein Feld hier ist eine Berechtigung. Geldwerte sind Decimal-Strings (Wei), nie number.

export type GateVerdict = "NOT_EXECUTED" | "NOT_QUALIFIED" | "PASS" | "FAIL" | "BLOCKED" | "UNKNOWN";
export type GuardState = "UNARMED" | "ARMED" | "UNKNOWN";

export interface CommandRow {
  commandId: string;
  requestId: string;
  action: string;
  phase: string; // PENDING | COMPLETED | FAILED | UNKNOWN | PENDING_RECONCILE
  detail: string;
}

export interface BscStatusView {
  configVersion: bigint | null;
  guardState: GuardState;
  gates: { g06a: GateVerdict; g07: GateVerdict; g08: GateVerdict };
  portal: { version: string | null; frozenVersion: string | null; drift: boolean | null };
  builder: { probe: "READY" | "NOT_READY" | "UNKNOWN"; version: string | null };
  preflight: GateVerdict;
  sourceWitness: string | null;
  liveBlocked: boolean | null;
  pendingCommands: CommandRow[];
  recentCommandResults: CommandRow[];
}

export interface BscLaunchView {
  planVersion: bigint | null;
  budgetWei: string | null;
  maxFeeBps: number | null;
  slippageBps: number | null;
  gateVerdict: GateVerdict;
  lockedActions: string[];
}

export interface BscMonitoringView {
  witness: string | null;
  chainReconcile: GateVerdict;
  receiptsCount: number | null;
  canonicalOrdering: string | null;
  feesObservedWei: string | null;
  lastInclusionBlock: bigint | null;
}

export interface BscTokenBalance {
  symbol: string;
  balanceAtomic: string;
  decimals: number;
}

export interface BscWalletsView {
  bnbBalanceWei: string | null;
  reservedWei: string | null;
  availableWei: string | null;
  tokens: BscTokenBalance[];
  gates: { chain: GateVerdict; allowlist: GateVerdict; finality: GateVerdict };
}
