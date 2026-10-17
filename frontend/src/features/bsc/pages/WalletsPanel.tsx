import { TopicId } from "@/core/realtime";
import { useTopic } from "@/core/realtime/hooks";
import { Line, Section } from "@/shared/ui/graphite/Rows";
import { Notice } from "@/shared/ui/graphite/Notice";
import { formatWei } from "../model/format";
import { selectWallets, useBscStore } from "../store/bscStore";

// BSC-eigene Wallets. Keine P0-Treasury, kein Send, kein Auto-Sweep in dieser Phase.
export function WalletsPanel() {
  const feedStatus = useTopic(TopicId.BSC_WALLETS_STATE);
  const { projection } = useBscStore(selectWallets);
  const w = feedStatus === "LIVE" ? projection : null;

  return (
    <>
      <Section title="Balances">
        <Line label="BNB balance" value={formatWei(w?.bnbBalanceWei ?? null)} />
        <Line label="Reserved" value={formatWei(w?.reservedWei ?? null)} />
        <Line label="Available" value={formatWei(w?.availableWei ?? null)} />
        {w && w.tokens.length > 0 ? (
          w.tokens.map((t) => (
            <Line key={t.symbol} label={t.symbol} value={formatWei(t.balanceAtomic, t.decimals, 6, t.symbol)} />
          ))
        ) : (
          <Line label="Tokens" value="—" />
        )}
      </Section>

      <Section title="Gates">
        <Line label="Source quality" value={feedStatus} />
        <Line label="Chain" value={w?.gates.chain ?? "—"} />
        <Line label="Allowlist" value={w?.gates.allowlist ?? "—"} />
        <Line label="Finality" value={w?.gates.finality ?? "—"} />
      </Section>

      <Notice>
        BSC wallets are separate from the Project 0 treasury. Balances are displayed only with chain, price and finality provenance from the engine.
      </Notice>
    </>
  );
}
