import "@/shared/ui/graphite/control-dialog.css";
import type { ReactNode } from "react";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { useWalletControl } from "@/core/realtime/bscControl";
import type { TransferRow } from "../model/wallets";
import { CHAIN_LABEL } from "../model/wallets";
import { ChainTxStatus } from "../api/bscV8Adapter";
import { Badge, DetailLine, Icon, Wei, bnb } from "./V8";

export async function copyText(text: string): Promise<boolean> {
  try { await navigator.clipboard.writeText(text); return true; } catch { return false; }
}
export const shortAddress = (a: string) => a ? `${a.slice(0, 8)}…${a.slice(-6)}` : "No address";

/** Why an action is not offered, visible next to it (never a silent grey button). */
export function Locked({ reason, testId }: { reason: string; testId?: string }) {
  return <div className="notice mt" data-testid={testId}><Icon name="lock" size="s" /><div>{reason}</div></div>;
}

/** The decided receipt of the last wallet request of this origin (gateway → native owner). */
export function CommandReceipt({ origin }: { origin: string }) {
  const { receipt } = useWalletControl();
  return receipt && receipt.origin === origin ? <p className="small mt" role="status" data-testid={`${origin}-receipt`}>{receipt.phase} · {receipt.detail}<br />Command {receipt.id}</p> : null;
}

/** A transfer as the native owner holds it: status, amount, fee, nonce, transaction, block, reason. */
export function TransferLines({ row, empty, testId }: { row: TransferRow | undefined; empty: string; testId: string }) {
  if (!row) return <DetailLine label="Last transfer" value={empty} testId={testId} />;
  const t = row.t;
  return <div data-testid={testId} data-status={row.label}>
    <DetailLine label="Last transfer" value={<span className="start"><Badge label={row.label} tone={row.tone} />{row.duplicateTrigger ? <Badge label="Two requests on one trigger" tone="bad" /> : null}</span>} />
    <DetailLine label="Amount" value={<Wei wei={t.amountWei} places={6} />} />
    <DetailLine label="Fee" value={t.receipt?.feeWei !== undefined ? `${bnb(t.receipt.feeWei, 6)} BNB` : t.feeEstimateWei !== undefined ? `≤ ${bnb(t.feeEstimateWei, 6)} BNB (estimate)` : "— (not reported)"} />
    <DetailLine label="Nonce · block" value={`${t.nonce?.toString() ?? "—"} · ${t.receipt?.blockNumber?.toString() ?? "not included"}`} />
    <DetailLine label="Transaction" value={<span className="wallet-address">{t.txHash || "not signed"}</span>} />
    {t.triggerId ? <DetailLine label="Trigger" value={<span className="wallet-address">{t.triggerId}{t.policyVersion !== undefined ? ` · policy v${t.policyVersion}` : ""}</span>} /> : null}
    {t.failureReason ? <DetailLine label="Reason" value={t.failureReason} /> : null}
    <DetailLine label="Intent · request" value={<span className="wallet-address">{t.commandId || "—"} · {t.requestId || "—"}</span>} />
  </div>;
}

export const chainLabel = (s: ChainTxStatus) => CHAIN_LABEL[s] ?? CHAIN_LABEL[ChainTxStatus.UNSPECIFIED];

/** The confirmation of one money request: everything the owner will be asked, then one request. */
const SEND_NOTE = "One request to the native BSC treasury owner. It checks the version, the balance, the reserve and the fee again, signs and sends on its own, and reports the chain's receipt. An unknown handoff is never sent twice.";
export function WalletConfirm({ open, title, lines, action, note = SEND_NOTE, onCancel, onConfirm, testId }: { open: boolean; title: string; lines: ReadonlyArray<[string, ReactNode]>; action: string; note?: string; onCancel: () => void; onConfirm: () => void; testId: string }) {
  return <AlertDialog.Root open={open} onOpenChange={o => { if (!o) onCancel(); }}>
    <AlertDialog.Portal><AlertDialog.Overlay className="ov-log-overlay" /><AlertDialog.Content className="ov-confirm-view" data-testid={testId}>
      <AlertDialog.Title>{title}</AlertDialog.Title>
      <AlertDialog.Description asChild><div>
        {lines.map(([k, v]) => <p key={k}><strong>{k}:</strong> {v}</p>)}
        <p>{note}</p>
      </div></AlertDialog.Description>
      <div className="ov-confirm-actions"><AlertDialog.Cancel asChild><button className="ov-btn">Cancel</button></AlertDialog.Cancel>
        <AlertDialog.Action asChild><button className="ov-btn" data-testid={`${testId}-confirm`} onClick={onConfirm}>{action}</button></AlertDialog.Action></div>
    </AlertDialog.Content></AlertDialog.Portal>
  </AlertDialog.Root>;
}
