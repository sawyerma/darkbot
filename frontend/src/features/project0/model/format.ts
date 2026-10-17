import type { FinanceSnapshot } from "./contracts";
/** A missing/UNKNOWN monetary value must never become 0 USD. */
export function usdMicroDisplay(value: string | null | undefined): string {
  if (value == null || !/^-?\d+$/.test(value)) return "— USD";
  const n = BigInt(value);
  const sign = n < 0n ? "-" : "";
  const a = n < 0n ? -n : n;
  const whole = a / 1000000n;
  const cents = ((a % 1000000n) / 10000n).toString().padStart(2, "0");
  return sign + whole.toLocaleString("en-US") + "." + cents + " USD";
}
export function financialRows(f: FinanceSnapshot | null) {
  return [
    ["Total capital", usdMicroDisplay(f?.totalCapitalUsdMicro)],
    ["Realized net PnL", usdMicroDisplay(f?.realizedNetPnlUsdMicro)],
    ["On-chain balance", "—"],
    ["Available capital", usdMicroDisplay(f?.availableCapitalUsdMicro)],
    ["Reserved capital", usdMicroDisplay(f?.reservedCapitalUsdMicro)],
    ["Withdrawable profit", usdMicroDisplay(f?.withdrawableProfitUsdMicro)],
    ["Transferred to Treasury", usdMicroDisplay(f?.transferredToTreasuryUsdMicro)],
  ] as const;
}
export function validDecimal(v: string, allowZero = false): boolean {
  if (!/^\d+(?:\.\d{1,6})?$/.test(v)) return false;
  const n = Number(v);
  return Number.isFinite(n) && (allowZero ? n >= 0 : n > 0);
}
