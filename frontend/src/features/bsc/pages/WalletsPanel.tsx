import { TopicId } from "@/core/realtime";
import { useTopic } from "@/core/realtime/hooks";
import { Line,Section } from "@/shared/ui/graphite/Rows";
import { Notice } from "@/shared/ui/graphite/Notice";
import { selectWallets,useBscStore } from "../store/bscStore";
import { EvidenceRows,timestamp } from "./EvidenceRows";
import { formatWei } from "../model/format";
export function WalletsPanel(){const quality=useTopic(TopicId.BSC_WALLETS_STATE);const {projection:w}=useBscStore(selectWallets);return <><Section title="Wallet snapshots"><Line label="Source quality" value={quality}/><Line label="Source" value={w?.source||"UNKNOWN"}/><Line label="Wallet state" value={w?.walletStatus||"UNKNOWN"}/><Line label="Expected address" value={w?.expectedAddress||"—"}/><Line label="Launcher" value={w?.launcherAddress||"—"}/><Line label="Launcher balance" value={formatWei(w?.launcherBalanceWei??null)}/><Line label="Launcher required" value={formatWei(w?.requiredLauncherWei??null)}/><Line label="Buyer" value={w?.buyerAddress||"—"}/><Line label="Buyer balance" value={formatWei(w?.buyerBalanceWei??null)}/><Line label="Buyer required" value={formatWei(w?.requiredBuyerWei??null)}/><Line label="Chain" value={w?.chainId?.toString()||"—"}/><Line label="Observed block" value={w?.observedBlock?.toString()||"—"}/><Line label="Balance observed" value={timestamp(w?.observedUnixS)}/><EvidenceRows evidence={w?.evidence}/></Section><Notice>Balances are historical snapshots, not a live balance reader. No transfers or wallet mutations are available.</Notice></>;}
