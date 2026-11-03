import type { TopicStatus } from "@/core/realtime";
import type { WalletsView } from "../model/owner";
import { usdMicroDisplay } from "../model/format";
import { DetailRow, Section } from "./PanelPrimitives";

const sol = (lamports: bigint | null) => (lamports == null ? "unknown" : `${lamports} lamports`);

/**
 * DEC-17b: every configured signer's wallet beside the frozen Wallets panel — its roles and
 * switch, whether it runs candidates and why it may not run one now, its SOL (reconciled and free
 * after its holds and the policy reserve), its holds, held attempt, Safety and journal, and the
 * candidates of this run it ran; then how many candidates no wallet could run and why the last
 * could not. Each candidate runs with the wallet with the most free SOL (the lowest key on a tie);
 * an error, UNKNOWN or HALT of a wallet blocks that wallet only. B4: a liquidator without its own
 * fees shows the fee payer it runs with; a fee payer the SOL held on it for the liquidators it pays.
 * Read-only: switches are on the Signers section. Nothing is shown without the owner's projection.
 */
export function WalletRowsSection({ wallets, topicStatus }: { wallets: WalletsView | null; topicStatus: TopicStatus }) {
  if (!wallets || wallets.rows.length === 0) return null;
  return <Section title="Wallets · per signer" aside={<span className="p0-badge">{topicStatus}</span>}>
    {topicStatus !== "LIVE" ? <p className="p0-quiet">Last reported wallets ({topicStatus}); not current.</p> : null}
    {wallets.rows.map((r) => <DetailRow key={r.wallet} title={`${r.wallet} · ${r.roles.length ? r.roles.join(" · ") : "no role"}`}
      value={<span data-testid={`p0-wallet-row-${r.wallet}`}>{[
        r.enabled ? "ENABLED" : "DISABLED",
        !r.runsCandidates ? "runs no candidate" : r.blockReason ? `blocked: ${r.blockReason}` : "may run candidates",
        r.runsCandidates ? `fees paid by ${r.feePayer == null ? "no eligible fee payer" : r.feePayer === r.wallet ? "itself" : r.feePayer}` : `holds ${r.solHeldForOthers} lamports for the liquidators it pays`,
        `SOL ${sol(r.solLamports)} · free ${sol(r.freeSolLamports)}`,
        r.holds.length ? `holds ${r.holds.map((h) => `${h.asset} ${h.units}`).join(", ")}` : "no hold",
        `attempt ${r.heldAttempt}`,
        r.haltReason ? `HALT ${r.haltReason}` : "not halted",
        `capital ${usdMicroDisplay(r.capitalInUseUsdMicro.toString())} · ${r.dayAttempts} attempts today`,
        r.journalRead ? `realized ${r.realizedNetUsdMicro == null ? "unknown" : usdMicroDisplay(r.realizedNetUsdMicro)}` : "journal not read",
        `ran ${r.selected} candidates`,
      ].join(" · ")}</span>}/>)}
    <DetailRow title="Candidates · run / no wallet could run them"
      value={`${wallets.selections} / ${wallets.selectionsNoneEligible}${wallets.lastNoneEligible ? ` · last: ${wallets.lastNoneEligible}` : ""}`}/>
  </Section>;
}
