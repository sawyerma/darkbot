// The decoded BSC projections; nothing here decides or permits. Amounts remain exact decimal wei; absent means unknown.
export interface PublicField { name:string; value:string; hint?:string }
/** One public native leaf: DBREC name, kind (NativeKind), canonical value and native unit. */
export interface NativeFieldView { name:string; kind:number; value:string; unit:string }
export interface NativeRecordView { schema:string; evidence?:Evidence; provenance:string; fields:NativeFieldView[] }
export interface Evidence { path:string; sha256:string; observedUnixS:bigint }
export interface Guard { guardState:string; frozenApprovedSha256:string; contractPresent:boolean; contractPath:string; contractSha256:string; approved?:boolean; executionReady?:boolean; attemptId:string; maxAttempts?:bigint }
export interface BscStatusView {
 sourceCommit:string; evaluatedUnixS:bigint; dataAsOfUnixS:bigint;
 guard?:Guard; gates:{gateId:string;state:string;evidence?:Evidence;reasonCodes:string[]}[];
 readiness?:{verdict:string; mode:string; candidateBinding:string; evidence?:Evidence; fundingVerdict:string; checks:PublicField[]; portalVersionBound:string;portalVersionObserved:string;portalImplBound:string;portalImplObserved:string;codeHashChanged?:boolean;headBlock?:bigint;reasons:string[]};
 builder?:{refreshObservedUnixS:bigint;refreshExpiresUnixS:bigint;floors:PublicField[];probe?:Evidence;price:string;noMerge:string;positionFirst:string;inclusion:string};
 witness?:{sourceReady?:boolean;t0ToBuyState:string;evidence?:Evidence};
 dry?:{evidence?:Evidence;dryId:string;state:string;parameters:PublicField[];validForCurrentPortal?:boolean};
 jobs:{job:string;state:string;launchId:string;resultCode:string;startedUnixS:bigint;endedUnixS:bigint;controllerAvailable?:boolean}[]; warnings:string[];
}
/** One native wallet or economic switch (plan 27.10.): a permission, never an action. */
export interface ControlSwitchView {id:string;requested:string;effective:string;blockReason:string;generation:bigint;requestId:string;setGeneration?:bigint}
/** The persisted daily BUY attempt limit and its use on the current UTC day (plan 27.10.). */
export interface BuyAttemptLimitView {perDay:bigint;dayUtc:string;used:bigint;remaining:bigint;generation:bigint;requestId:string;setGeneration?:bigint}
export interface BscLaunchView {contractPresent:boolean;evidence?:Evidence;contractFields:PublicField[];runtimeFields:PublicField[];reason:string;records:NativeRecordView[];economicControls:ControlSwitchView[];buyAttemptLimit?:BuyAttemptLimitView;runtimeMode?:ControlSwitchView}
export interface BscMonitoringView {state:string;evidence?:Evidence;results:PublicField[];records:NativeRecordView[]}
/** Plan 28.10.: the LIVE tab, as the native owner, runtime and ledger hold it; never a permission. */
export interface LivePreconditionView {id:string;met:boolean;reason:string}
export interface LiveGrantView {venue:string;strategies:string;sources:string[];adapters:string[];maxInputWei:string;maxSpendWei:string;maxAttempts:number;expiresUnixMs:bigint;scopeSha256:string;scopeText:string;requestId:string;setGeneration:bigint;exitOnly:boolean;risk?:LiveGrantRiskView;limits?:LiveGrantLimitsView}
/** BSC3-069: the execution risk a grant names; absent: the grant admits nothing. */
/** 31.10. order: the positions and losses a grant names; absent: the grant arms nothing. */
export interface LiveGrantLimitsView {maxOpenPositions:number;maxRealizedLossWei:string;maxDailyRealizedLossWei:string;minBuyIntervalMs:bigint}
export interface LiveGrantRiskView {maxBuySlippageBps:number;maxExitSlippageBps:number;maxGasPriceWei:bigint;maxBuyGasLimit:bigint;maxApproveGasLimit:bigint;maxSellGasLimit:bigint}
export interface LivePositionView {lot:string;token:string;component:string;state:string;tokens:string;inputWei:string;proceedsWei:string;gasWei:string;pnlWei?:string}
export interface LiveTransactionView {txHash:string;intent:string;token:string;nonce:bigint;state:string;blockNumber?:bigint}
/** REV.1.0 R2b: the LIVE campaign verdict and its attempts, as their records hold them (evidence only). */
export interface LiveCampaignCellView {cell:string;enabled:boolean;minAttempts?:bigint;attempts:bigint;passed:bigint;failed:bigint;undecided:bigint;minDurationS?:bigint;durationS:bigint;verdict:string}
export interface LiveCampaignView {verdict:string;releaseEvidence:string;reasons:string[];planned:bigint;observed:bigint;selected:bigint;admitted:bigint;skipped:bigint;written:bigint;acquired:bigint;reverted:bigint;dropped:bigint;late:bigint;unresolved:bigint;passed:bigint;failed:bigint;undecided:bigint;consecutiveMisses:bigint;beforePrecommit:bigint;unverifiable:bigint;coveredS:bigint;uncoveredS:bigint;coveredPpm:bigint;skipReasons:string[];cells:LiveCampaignCellView[];manifestSha256:string;precommitSha256:string;buildSha256:string;evaluatedUnixMs:bigint;recordSha256:string}
export interface LiveAttemptAdapterView {builder:string;outcome:string;answer:string}
export interface LiveAttemptView {ordinal:bigint;tx:string;kind:string;cell:string;state:string;verdict:string;acquisition:string;canonical:string;timing:string;placement:string;capture:string;local:string;economics:string;reasons:string[];windowComplete:boolean;windowWhy:string;trades:bigint;buys:bigint;sells:bigint;firstExternalPrice?:string;priceVsFirstExternalPpm?:bigint;ingressToSendNs?:bigint;selectToSendNs?:bigint;sendToAckNs?:bigint;attempts:bigint;acquired:bigint;passed:bigint;failed:bigint;undecided:bigint;adapters:LiveAttemptAdapterView[]}
export interface BscLiveView {gate:string;blockers:string[];preconditions:LivePreconditionView[];grant?:LiveGrantView;generation:bigint;runtimeMode?:ControlSwitchView;economicLive?:ControlSwitchView;daily?:BuyAttemptLimitView;totalLimit:bigint;totalUsed:bigint;grantAttemptsUsed:bigint;spentWei:string;qualification:string;qualificationDetail:string;pathState:string;pathReason:string;route:string;buyer:string;buyerBalanceWei?:string;released:bigint;unknown:bigint;lastRefusal:string;positions:LivePositionView[];transactions:LiveTransactionView[];realizedPnlWei:string;gasWei:string;warnings:string[];scope:ControlSwitchView[];grantableSources:string[];campaign?:LiveCampaignView;attempts:LiveAttemptView[]}
export interface BscWalletsView {expectedAddress:string;walletStatus:string;launcherAddress:string;buyerAddress:string;chainId?:bigint;launcherBalanceWei?:string;buyerBalanceWei?:string;observedBlock?:bigint;observedUnixS:bigint;source:string;requiredLauncherWei?:string;requiredBuyerWei?:string;evidence?:Evidence;launcherControl?:ControlSwitchView;buyerControl?:ControlSwitchView}
