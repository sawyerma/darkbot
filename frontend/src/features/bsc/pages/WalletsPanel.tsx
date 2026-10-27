import { TopicId } from "@/core/realtime";
import { useTopic } from "@/core/realtime/hooks";
import { Line } from "@/shared/ui/graphite/Rows";
import { Notice } from "@/shared/ui/graphite/Notice";
import { selectStatus, selectWallets, useBscStore } from "../store/bscStore";
import { EvidenceRows, timestamp, ValueState } from "./EvidenceRows";
import { PlacedSection } from "./PlacedSection";
import { useNativeRecords } from "./useNativeRecords";
import { formatNative } from "../model/format";
import { switchRow, type SwitchRow } from "../model/switches";
import { SwitchActions } from "../SwitchActions";

/** One wallet switch: effective state, what was requested, why, the CAS generation and the setting request. */
function SwitchLines({ row }: { row: SwitchRow }) {
  return <>
    <Line label={`${row.label} switch`} value={<ValueState state={row.effective} />}
      hint={`requested ${row.requested}${row.blockReason ? ` · ${row.blockReason}` : ""} · ${row.hint}; DISABLED initially and after every restart`} />
    <Line label={`${row.label} switch generation`} value={row.available ? row.generation.toString() : row.reason} hint={`Native owner generation (CAS) · last set by ${row.receipt}`} />
  </>;
}

const wei = (v: string | undefined) => v === undefined ? "UNKNOWN" : formatNative(v, "wei");

/** Wallets in the fixed §13 order: launcher → buyer → balances → funding → reserve → nonces → snapshots. */
export function WalletsPanel() {
  const quality = useTopic(TopicId.BSC_WALLETS_STATE);
  useTopic(TopicId.BSC_LAUNCH_STATE); // preflight funding, reserves and nonces
  useTopic(TopicId.BSC_MONITORING_STATE); // attempt-bound spend and nonces
  const { projection: w } = useBscStore(selectWallets);
  const { projection: s } = useBscStore(selectStatus);
  const records = useNativeRecords();
  const launcher = switchRow("BSC_WALLET_LAUNCHER", w?.launcherControl);
  const buyer = switchRow("BSC_WALLET_BUYER", w?.buyerControl);
  return <>
    <Line label="Wallet source quality" value={<ValueState state={quality} />} hint="bsc.wallets.state transport" />
    <PlacedSection panel="wallets" section={1} records={records}>
      <Line label="Expected address" value={w?.expectedAddress || "—"} />
      <Line label="Launcher" value={w?.launcherAddress || "—"} />
      <SwitchLines row={launcher} />
      <SwitchActions rows={[launcher]} testId="bsc-wallet-launcher" action="bsc.wallet.set" />
    </PlacedSection>
    <PlacedSection panel="wallets" section={2} records={records}>
      <Line label="Buyer" value={w?.buyerAddress || "—"} />
      <SwitchLines row={buyer} />
      <SwitchActions rows={[buyer]} testId="bsc-wallet-buyer" action="bsc.wallet.set" />
    </PlacedSection>
    <PlacedSection panel="wallets" section={3} records={records}>
      <Line label="Launcher balance" value={wei(w?.launcherBalanceWei)} hint="Historical snapshot, not a live balance" />
      <Line label="Buyer balance" value={wei(w?.buyerBalanceWei)} hint="Historical snapshot, not a live balance" />
    </PlacedSection>
    <PlacedSection panel="wallets" section={4} records={records}>
      <Line label="Launcher required" value={wei(w?.requiredLauncherWei)} />
      <Line label="Buyer required" value={wei(w?.requiredBuyerWei)} />
      <Line label="Funding verdict" value={<ValueState state={s?.readiness?.fundingVerdict} />} hint="From the native readiness check" />
    </PlacedSection>
    <PlacedSection panel="wallets" section={5} records={records} />
    <PlacedSection panel="wallets" section={6} records={records} />
    <PlacedSection panel="wallets" section={7} records={records}>
      <Line label="Wallet snapshots" value={<ValueState state={w?.source || undefined} />} hint="HISTORICAL_PREFLIGHT is an old observation" />
      <Line label="Wallet state" value={w?.walletStatus || "UNKNOWN"} />
      <Line label="Chain" value={w?.chainId?.toString() || "—"} />
      <Line label="Observed block" value={w?.observedBlock?.toString() || "—"} />
      <Line label="Balance observed" value={timestamp(w?.observedUnixS)} />
      <EvidenceRows evidence={w?.evidence} />
    </PlacedSection>
    <Notice>Balances are historical snapshots, not a live balance reader. No transfers or wallet mutations are available. The two wallet switches are native permissions of the bsc_runtime owner (the same as darkbotctl bsc source request): every Mainnet signing or sending path checks them, also when started in a terminal; ENABLED alone signs and sends nothing.</Notice>
  </>;
}
