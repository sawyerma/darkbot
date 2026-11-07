# Protocol Documentation
<a name="top"></a>

## Table of Contents

- [darkbot/bsc/v1/status.proto](#darkbot_bsc_v1_status-proto)
    - [BscLaunchState](#darkbot-bsc-v1-BscLaunchState)
    - [BscLiveGrantCommand](#darkbot-bsc-v1-BscLiveGrantCommand)
    - [BscLiveState](#darkbot-bsc-v1-BscLiveState)
    - [BscMonitoringState](#darkbot-bsc-v1-BscMonitoringState)
    - [BscStatus](#darkbot-bsc-v1-BscStatus)
    - [BscWalletsState](#darkbot-bsc-v1-BscWalletsState)
    - [BuilderStatus](#darkbot-bsc-v1-BuilderStatus)
    - [BuyAttemptLimit](#darkbot-bsc-v1-BuyAttemptLimit)
    - [ControlSwitch](#darkbot-bsc-v1-ControlSwitch)
    - [DryStatus](#darkbot-bsc-v1-DryStatus)
    - [Evidence](#darkbot-bsc-v1-Evidence)
    - [GateStatus](#darkbot-bsc-v1-GateStatus)
    - [GuardStatus](#darkbot-bsc-v1-GuardStatus)
    - [JobStatus](#darkbot-bsc-v1-JobStatus)
    - [LiveAttempt](#darkbot-bsc-v1-LiveAttempt)
    - [LiveAttemptAdapter](#darkbot-bsc-v1-LiveAttemptAdapter)
    - [LiveCampaign](#darkbot-bsc-v1-LiveCampaign)
    - [LiveCampaignCell](#darkbot-bsc-v1-LiveCampaignCell)
    - [LiveGrant](#darkbot-bsc-v1-LiveGrant)
    - [LiveGrantLimits](#darkbot-bsc-v1-LiveGrantLimits)
    - [LiveGrantRisk](#darkbot-bsc-v1-LiveGrantRisk)
    - [LivePosition](#darkbot-bsc-v1-LivePosition)
    - [LivePrecondition](#darkbot-bsc-v1-LivePrecondition)
    - [LiveTransaction](#darkbot-bsc-v1-LiveTransaction)
    - [NativeField](#darkbot-bsc-v1-NativeField)
    - [NativeRecord](#darkbot-bsc-v1-NativeRecord)
    - [PublicField](#darkbot-bsc-v1-PublicField)
    - [ReadinessStatus](#darkbot-bsc-v1-ReadinessStatus)
    - [WitnessStatus](#darkbot-bsc-v1-WitnessStatus)

    - [NativeKind](#darkbot-bsc-v1-NativeKind)

- [darkbot/bsc/v1/common.proto](#darkbot_bsc_v1_common-proto)
    - [OwnerProvenance](#darkbot-bsc-v1-OwnerProvenance)
    - [PerformancePoint](#darkbot-bsc-v1-PerformancePoint)
    - [PerformanceSeries](#darkbot-bsc-v1-PerformanceSeries)
    - [PerformanceSeriesQuery](#darkbot-bsc-v1-PerformanceSeriesQuery)
    - [SourcedTime](#darkbot-bsc-v1-SourcedTime)

    - [Availability](#darkbot-bsc-v1-Availability)
    - [DataQuality](#darkbot-bsc-v1-DataQuality)
    - [PerformanceMetric](#darkbot-bsc-v1-PerformanceMetric)
    - [PerformanceScope](#darkbot-bsc-v1-PerformanceScope)
    - [TimeSource](#darkbot-bsc-v1-TimeSource)

- [darkbot/bsc/v1/training.proto](#darkbot_bsc_v1_training-proto)
    - [LiveStrategySelectCommand](#darkbot-bsc-v1-LiveStrategySelectCommand)
    - [ReplaySource](#darkbot-bsc-v1-ReplaySource)
    - [ReplaySourcePage](#darkbot-bsc-v1-ReplaySourcePage)
    - [ReplaySourceQuery](#darkbot-bsc-v1-ReplaySourceQuery)
    - [StrategyCSettableValues](#darkbot-bsc-v1-StrategyCSettableValues)
    - [StrategyCSettings](#darkbot-bsc-v1-StrategyCSettings)
    - [StrategyCSettingsQuery](#darkbot-bsc-v1-StrategyCSettingsQuery)
    - [StrategyCSettingsSetAllCommand](#darkbot-bsc-v1-StrategyCSettingsSetAllCommand)
    - [StrategyCSettingsState](#darkbot-bsc-v1-StrategyCSettingsState)
    - [StrategyFeatureSpec](#darkbot-bsc-v1-StrategyFeatureSpec)
    - [StrategyFilterRule](#darkbot-bsc-v1-StrategyFilterRule)
    - [StrategySettingSpec](#darkbot-bsc-v1-StrategySettingSpec)
    - [TrainingProfile](#darkbot-bsc-v1-TrainingProfile)
    - [TrainingProfileCloneCommand](#darkbot-bsc-v1-TrainingProfileCloneCommand)
    - [TrainingProfileCreateCommand](#darkbot-bsc-v1-TrainingProfileCreateCommand)
    - [TrainingProfileDeleteCommand](#darkbot-bsc-v1-TrainingProfileDeleteCommand)
    - [TrainingProfileUpdateCommand](#darkbot-bsc-v1-TrainingProfileUpdateCommand)
    - [TrainingProfiles](#darkbot-bsc-v1-TrainingProfiles)
    - [TrainingProfilesQuery](#darkbot-bsc-v1-TrainingProfilesQuery)
    - [TrainingResult](#darkbot-bsc-v1-TrainingResult)
    - [TrainingRun](#darkbot-bsc-v1-TrainingRun)
    - [TrainingRunPage](#darkbot-bsc-v1-TrainingRunPage)
    - [TrainingRunPlan](#darkbot-bsc-v1-TrainingRunPlan)
    - [TrainingRunQuery](#darkbot-bsc-v1-TrainingRunQuery)
    - [TrainingRunStartCommand](#darkbot-bsc-v1-TrainingRunStartCommand)
    - [TrainingRunStopCommand](#darkbot-bsc-v1-TrainingRunStopCommand)

    - [CreatorSellExit](#darkbot-bsc-v1-CreatorSellExit)
    - [CreatorSoldBeforeFill](#darkbot-bsc-v1-CreatorSoldBeforeFill)
    - [ReplayVerdict](#darkbot-bsc-v1-ReplayVerdict)
    - [SettingsApplyStatus](#darkbot-bsc-v1-SettingsApplyStatus)
    - [TrainingProfileStatus](#darkbot-bsc-v1-TrainingProfileStatus)
    - [TrainingRunMode](#darkbot-bsc-v1-TrainingRunMode)
    - [TrainingRunStatus](#darkbot-bsc-v1-TrainingRunStatus)
    - [TrainingRunStopKind](#darkbot-bsc-v1-TrainingRunStopKind)

- [darkbot/bsc/v1/history.proto](#darkbot_bsc_v1_history-proto)
    - [TradeCounts](#darkbot-bsc-v1-TradeCounts)
    - [TradeDetail](#darkbot-bsc-v1-TradeDetail)
    - [TradeDetailQuery](#darkbot-bsc-v1-TradeDetailQuery)
    - [TradeExitTerms](#darkbot-bsc-v1-TradeExitTerms)
    - [TradeHistoryPage](#darkbot-bsc-v1-TradeHistoryPage)
    - [TradeHistoryQuery](#darkbot-bsc-v1-TradeHistoryQuery)
    - [TradeRow](#darkbot-bsc-v1-TradeRow)
    - [TradeSighting](#darkbot-bsc-v1-TradeSighting)
    - [TradeTransaction](#darkbot-bsc-v1-TradeTransaction)

    - [ExitReason](#darkbot-bsc-v1-ExitReason)
    - [ExitReasonSource](#darkbot-bsc-v1-ExitReasonSource)
    - [TradePlacement](#darkbot-bsc-v1-TradePlacement)
    - [TradeSort](#darkbot-bsc-v1-TradeSort)
    - [TradeStatus](#darkbot-bsc-v1-TradeStatus)

- [darkbot/bsc/v1/wallet.proto](#darkbot_bsc_v1_wallet-proto)
    - [FiatQuote](#darkbot-bsc-v1-FiatQuote)
    - [MainPayoutPolicy](#darkbot-bsc-v1-MainPayoutPolicy)
    - [MainPayoutPolicySetCommand](#darkbot-bsc-v1-MainPayoutPolicySetCommand)
    - [PolicyAmount](#darkbot-bsc-v1-PolicyAmount)
    - [ProfitTransferPolicy](#darkbot-bsc-v1-ProfitTransferPolicy)
    - [ProfitTransferPolicySetCommand](#darkbot-bsc-v1-ProfitTransferPolicySetCommand)
    - [WalletAccount](#darkbot-bsc-v1-WalletAccount)
    - [WalletAuthorization](#darkbot-bsc-v1-WalletAuthorization)
    - [WalletAuthorizationBounds](#darkbot-bsc-v1-WalletAuthorizationBounds)
    - [WalletAuthorizationScope](#darkbot-bsc-v1-WalletAuthorizationScope)
    - [WalletBalanceSnapshot](#darkbot-bsc-v1-WalletBalanceSnapshot)
    - [WalletDestination](#darkbot-bsc-v1-WalletDestination)
    - [WalletDestinationConfirmCommand](#darkbot-bsc-v1-WalletDestinationConfirmCommand)
    - [WalletDestinationSetCommand](#darkbot-bsc-v1-WalletDestinationSetCommand)
    - [WalletFundTradingCommand](#darkbot-bsc-v1-WalletFundTradingCommand)
    - [WalletHistoryPage](#darkbot-bsc-v1-WalletHistoryPage)
    - [WalletHistoryQuery](#darkbot-bsc-v1-WalletHistoryQuery)
    - [WalletMovement](#darkbot-bsc-v1-WalletMovement)
    - [WalletPayoutCommand](#darkbot-bsc-v1-WalletPayoutCommand)
    - [WalletTransferAuthorizeCommand](#darkbot-bsc-v1-WalletTransferAuthorizeCommand)
    - [WalletTransferPrepareCommand](#darkbot-bsc-v1-WalletTransferPrepareCommand)
    - [WalletTransferReceipt](#darkbot-bsc-v1-WalletTransferReceipt)
    - [WalletTransferState](#darkbot-bsc-v1-WalletTransferState)
    - [WalletTreasuryQuery](#darkbot-bsc-v1-WalletTreasuryQuery)
    - [WalletTreasuryState](#darkbot-bsc-v1-WalletTreasuryState)

    - [ChainTxStatus](#darkbot-bsc-v1-ChainTxStatus)
    - [PolicyUnit](#darkbot-bsc-v1-PolicyUnit)
    - [TransferStatus](#darkbot-bsc-v1-TransferStatus)
    - [WalletMovementKind](#darkbot-bsc-v1-WalletMovementKind)
    - [WalletRole](#darkbot-bsc-v1-WalletRole)

- [darkbot/bsc/v1/watch.proto](#darkbot_bsc_v1_watch-proto)
    - [BscWatchQuery](#darkbot-bsc-v1-BscWatchQuery)
    - [BscWatchState](#darkbot-bsc-v1-BscWatchState)
    - [WatchEvent](#darkbot-bsc-v1-WatchEvent)
    - [WatchSubjectState](#darkbot-bsc-v1-WatchSubjectState)

    - [RequalificationState](#darkbot-bsc-v1-RequalificationState)
    - [WatchSubjectKind](#darkbot-bsc-v1-WatchSubjectKind)
    - [WatchVerdict](#darkbot-bsc-v1-WatchVerdict)

- [darkbot/bsc/v1/query.proto](#darkbot_bsc_v1_query-proto)
    - [BscQueryRequest](#darkbot-bsc-v1-BscQueryRequest)
    - [BscQueryResponse](#darkbot-bsc-v1-BscQueryResponse)

    - [QueryRefusal](#darkbot-bsc-v1-QueryRefusal)

- [darkbot/control/v1/topics.proto](#darkbot_control_v1_topics-proto)
    - [TopicDescriptor](#darkbot-control-v1-TopicDescriptor)

    - [ProjectId](#darkbot-control-v1-ProjectId)
    - [TopicClass](#darkbot-control-v1-TopicClass)
    - [TopicId](#darkbot-control-v1-TopicId)

- [darkbot/control/v1/commands.proto](#darkbot_control_v1_commands-proto)
    - [CommandLookupResponse](#darkbot-control-v1-CommandLookupResponse)
    - [CommandRequest](#darkbot-control-v1-CommandRequest)
    - [CommandResponse](#darkbot-control-v1-CommandResponse)
    - [ExpectedVersion](#darkbot-control-v1-ExpectedVersion)

    - [CommandPhase](#darkbot-control-v1-CommandPhase)
    - [CommandRejectReason](#darkbot-control-v1-CommandRejectReason)
    - [CommandStatus](#darkbot-control-v1-CommandStatus)

- [darkbot/control/v1/stream.proto](#darkbot_control_v1_stream-proto)
    - [CatchupComplete](#darkbot-control-v1-CatchupComplete)
    - [ClientCapabilities](#darkbot-control-v1-ClientCapabilities)
    - [ClientMessage](#darkbot-control-v1-ClientMessage)
    - [ControlAck](#darkbot-control-v1-ControlAck)
    - [Error](#darkbot-control-v1-Error)
    - [Event](#darkbot-control-v1-Event)
    - [EventCursor](#darkbot-control-v1-EventCursor)
    - [Heartbeat](#darkbot-control-v1-Heartbeat)
    - [Hello](#darkbot-control-v1-Hello)
    - [Pong](#darkbot-control-v1-Pong)
    - [ServerLimits](#darkbot-control-v1-ServerLimits)
    - [ServerMessage](#darkbot-control-v1-ServerMessage)
    - [StateFull](#darkbot-control-v1-StateFull)
    - [StateWatermark](#darkbot-control-v1-StateWatermark)
    - [Subscribe](#darkbot-control-v1-Subscribe)
    - [Unsubscribe](#darkbot-control-v1-Unsubscribe)
    - [Welcome](#darkbot-control-v1-Welcome)

    - [AckReason](#darkbot-control-v1-AckReason)
    - [AckStatus](#darkbot-control-v1-AckStatus)
    - [CatchupQuality](#darkbot-control-v1-CatchupQuality)
    - [CatchupReason](#darkbot-control-v1-CatchupReason)
    - [ErrorCode](#darkbot-control-v1-ErrorCode)
    - [EvidenceScope](#darkbot-control-v1-EvidenceScope)
    - [StartPolicy](#darkbot-control-v1-StartPolicy)
    - [StateQuality](#darkbot-control-v1-StateQuality)
    - [WelcomeStatus](#darkbot-control-v1-WelcomeStatus)

- [darkbot/control/v1/engine_feed.proto](#darkbot_control_v1_engine_feed-proto)
    - [EngineFrame](#darkbot-control-v1-EngineFrame)
    - [EngineHello](#darkbot-control-v1-EngineHello)
    - [StateTooLarge](#darkbot-control-v1-StateTooLarge)

- [darkbot/control/v1/numeric.proto](#darkbot_control_v1_numeric-proto)
    - [SignedDecimal](#darkbot-control-v1-SignedDecimal)
    - [UnsignedDecimal](#darkbot-control-v1-UnsignedDecimal)

- [darkbot/control/v1/overview.proto](#darkbot_control_v1_overview-proto)
    - [FinanceSummary](#darkbot-control-v1-FinanceSummary)
    - [GlobalFinance](#darkbot-control-v1-GlobalFinance)
    - [LatencySample](#darkbot-control-v1-LatencySample)
    - [OverviewAggregates](#darkbot-control-v1-OverviewAggregates)
    - [ProjectJob](#darkbot-control-v1-ProjectJob)
    - [ProjectSummary](#darkbot-control-v1-ProjectSummary)

    - [FinanceQuality](#darkbot-control-v1-FinanceQuality)
    - [Readiness](#darkbot-control-v1-Readiness)
    - [SourceState](#darkbot-control-v1-SourceState)

- [darkbot/project0/v1/analytics.proto](#darkbot_project0_v1_analytics-proto)
    - [AnalyticsJob](#darkbot-project0-v1-AnalyticsJob)
    - [AnalyticsJobs](#darkbot-project0-v1-AnalyticsJobs)
    - [AnalyticsRun](#darkbot-project0-v1-AnalyticsRun)
    - [AutostartGrant](#darkbot-project0-v1-AutostartGrant)
    - [AutostartGrantCommand](#darkbot-project0-v1-AutostartGrantCommand)
    - [AutostartPreviewCommand](#darkbot-project0-v1-AutostartPreviewCommand)
    - [BacktestJobCommand](#darkbot-project0-v1-BacktestJobCommand)
    - [BacktestResult](#darkbot-project0-v1-BacktestResult)
    - [BootFlow](#darkbot-project0-v1-BootFlow)
    - [BootNetworkCommand](#darkbot-project0-v1-BootNetworkCommand)
    - [BootOfflineCommand](#darkbot-project0-v1-BootOfflineCommand)
    - [BootStartCommand](#darkbot-project0-v1-BootStartCommand)
    - [BootStep](#darkbot-project0-v1-BootStep)
    - [CapitalScenario](#darkbot-project0-v1-CapitalScenario)
    - [FundingSet](#darkbot-project0-v1-FundingSet)
    - [H0JobCommand](#darkbot-project0-v1-H0JobCommand)
    - [H0Result](#darkbot-project0-v1-H0Result)
    - [H0Spec](#darkbot-project0-v1-H0Spec)
    - [JobGrid](#darkbot-project0-v1-JobGrid)
    - [JobParams](#darkbot-project0-v1-JobParams)
    - [NamedCount](#darkbot-project0-v1-NamedCount)
    - [OptimizeJobCommand](#darkbot-project0-v1-OptimizeJobCommand)
    - [OptimizeResult](#darkbot-project0-v1-OptimizeResult)
    - [OptimizeScore](#darkbot-project0-v1-OptimizeScore)
    - [PostCaptureJobCommand](#darkbot-project0-v1-PostCaptureJobCommand)
    - [PostCaptureResult](#darkbot-project0-v1-PostCaptureResult)
    - [PostCaptureSpec](#darkbot-project0-v1-PostCaptureSpec)
    - [ProviderCredentialCommand](#darkbot-project0-v1-ProviderCredentialCommand)
    - [ProviderDescriptorCommand](#darkbot-project0-v1-ProviderDescriptorCommand)
    - [ProviderPriorityCommand](#darkbot-project0-v1-ProviderPriorityCommand)
    - [ProviderQualifyCommand](#darkbot-project0-v1-ProviderQualifyCommand)
    - [ProviderRegistryView](#darkbot-project0-v1-ProviderRegistryView)
    - [ProviderSelectCommand](#darkbot-project0-v1-ProviderSelectCommand)
    - [RegistryCredential](#darkbot-project0-v1-RegistryCredential)
    - [RegistryProvider](#darkbot-project0-v1-RegistryProvider)
    - [RunQualification](#darkbot-project0-v1-RunQualification)
    - [ShadowCounterfactual](#darkbot-project0-v1-ShadowCounterfactual)
    - [UntilStopped](#darkbot-project0-v1-UntilStopped)
    - [UntilStoppedQualification](#darkbot-project0-v1-UntilStoppedQualification)
    - [UntilStoppedRun](#darkbot-project0-v1-UntilStoppedRun)
    - [VerificationMethod](#darkbot-project0-v1-VerificationMethod)

- [darkbot/project0/v1/capture.proto](#darkbot_project0_v1_capture-proto)
    - [CaptureState](#darkbot-project0-v1-CaptureState)
    - [Counter](#darkbot-project0-v1-Counter)
    - [MarketAssessment](#darkbot-project0-v1-MarketAssessment)
    - [MarketCounters](#darkbot-project0-v1-MarketCounters)
    - [PhaseStatus](#darkbot-project0-v1-PhaseStatus)
    - [ProviderBudget](#darkbot-project0-v1-ProviderBudget)
    - [ReasonCount](#darkbot-project0-v1-ReasonCount)
    - [ReceiverTotal](#darkbot-project0-v1-ReceiverTotal)
    - [RunInfo](#darkbot-project0-v1-RunInfo)
    - [TransportStatus](#darkbot-project0-v1-TransportStatus)
    - [WriterLane](#darkbot-project0-v1-WriterLane)

    - [CapturePhase](#darkbot-project0-v1-CapturePhase)

- [darkbot/project0/v1/owner.proto](#darkbot_project0_v1_owner-proto)
    - [AssetAmount](#darkbot-project0-v1-AssetAmount)
    - [Capability](#darkbot-project0-v1-Capability)
    - [CapabilityControlCommand](#darkbot-project0-v1-CapabilityControlCommand)
    - [DataSource](#darkbot-project0-v1-DataSource)
    - [Fe04Admission](#darkbot-project0-v1-Fe04Admission)
    - [Fe04Approval](#darkbot-project0-v1-Fe04Approval)
    - [Fe06Preview](#darkbot-project0-v1-Fe06Preview)
    - [Fe06State](#darkbot-project0-v1-Fe06State)
    - [FinanceSummary](#darkbot-project0-v1-FinanceSummary)
    - [HoldAttempt](#darkbot-project0-v1-HoldAttempt)
    - [HoldB](#darkbot-project0-v1-HoldB)
    - [HoldFlow](#darkbot-project0-v1-HoldFlow)
    - [LatencyNs](#darkbot-project0-v1-LatencyNs)
    - [LiquidatorState](#darkbot-project0-v1-LiquidatorState)
    - [LossCount](#darkbot-project0-v1-LossCount)
    - [NonLanding](#darkbot-project0-v1-NonLanding)
    - [NonLandingSource](#darkbot-project0-v1-NonLandingSource)
    - [OwnerEngine](#darkbot-project0-v1-OwnerEngine)
    - [OwnerPipeline](#darkbot-project0-v1-OwnerPipeline)
    - [OwnerRun](#darkbot-project0-v1-OwnerRun)
    - [OwnerSafety](#darkbot-project0-v1-OwnerSafety)
    - [OwnerThreshold](#darkbot-project0-v1-OwnerThreshold)
    - [OwnerTrigger](#darkbot-project0-v1-OwnerTrigger)
    - [ProgramPin](#darkbot-project0-v1-ProgramPin)
    - [RebalanceBlocked](#darkbot-project0-v1-RebalanceBlocked)
    - [RebalancePlan](#darkbot-project0-v1-RebalancePlan)
    - [RebalanceTrade](#darkbot-project0-v1-RebalanceTrade)
    - [RebalanceTransfer](#darkbot-project0-v1-RebalanceTransfer)
    - [SourceControlCommand](#darkbot-project0-v1-SourceControlCommand)
    - [SourceDecision](#darkbot-project0-v1-SourceDecision)
    - [SourceStats](#darkbot-project0-v1-SourceStats)
    - [StateCounters](#darkbot-project0-v1-StateCounters)
    - [WalletRow](#darkbot-project0-v1-WalletRow)
    - [WalletsState](#darkbot-project0-v1-WalletsState)

- [darkbot/project0/v1/status.proto](#darkbot_project0_v1_status-proto)
    - [StatusProjection](#darkbot-project0-v1-StatusProjection)

- [darkbot/project0/v1/system.proto](#darkbot_project0_v1_system-proto)
    - [EffectiveRecord](#darkbot-project0-v1-EffectiveRecord)
    - [EffectiveRecord.SourcesEntry](#darkbot-project0-v1-EffectiveRecord-SourcesEntry)
    - [OperationalConfig](#darkbot-project0-v1-OperationalConfig)
    - [OperationalParameter](#darkbot-project0-v1-OperationalParameter)
    - [RunProvider](#darkbot-project0-v1-RunProvider)
    - [SystemStatus](#darkbot-project0-v1-SystemStatus)

- [Scalar Value Types](#scalar-value-types)



<a name="darkbot_bsc_v1_status-proto"></a>
<p align="right"><a href="#top">Top</a></p>

## darkbot/bsc/v1/status.proto



<a name="darkbot-bsc-v1-BscLaunchState"></a>

### BscLaunchState
Bound launch plan, without signatures, raw transactions or execution authority.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| contract_present | [bool](#bool) |  | True only if the current V5 contract is present and valid. |
| evidence | [Evidence](#darkbot-bsc-v1-Evidence) |  | Contract identity. |
| contract_fields | [PublicField](#darkbot-bsc-v1-PublicField) | repeated | Public contract values, exact integer caps and binding hashes. |
| runtime_fields | [PublicField](#darkbot-bsc-v1-PublicField) | repeated | Observed runtime context; absent until such evidence actually exists. |
| reason | [string](#string) |  | Missing/invalid contract explanation. |
| records | [NativeRecord](#darkbot-bsc-v1-NativeRecord) | repeated | Latest public qualification records: DRY, preflight, quote, economics, simulation, launch path, R0 portal scan, requalification, PREPARE handoff and the V5 contract. |
| economic_controls | [ControlSwitch](#darkbot-bsc-v1-ControlSwitch) | repeated | The economic switches SIGN, ARM, SEND, ATTEMPT and LIVE of the native owner; absent before its state exists. |
| buy_attempt_limit | [BuyAttemptLimit](#darkbot-bsc-v1-BuyAttemptLimit) |  | The daily BUY attempt limit and today&#39;s use; absent before the owner state exists. |
| runtime_mode | [ControlSwitch](#darkbot-bsc-v1-ControlSwitch) |  | The global runtime mode BSC_RUNTIME_MODE (OFF, SHADOW, PAPER or LIVE; a requested LIVE is effective only while the owner&#39;s LIVE gate holds, BscLiveState) in the ControlSwitch shape; absent before the owner state exists. |






<a name="darkbot-bsc-v1-BscLiveGrantCommand"></a>

### BscLiveGrantCommand
The parameters of `bsc.live.grant`: the scope the operator confirmed and its digest.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| venue | [string](#string) |  | The venue, e.g. BSC_VENUE_FLAP. |
| strategies | [string](#string) |  | C3, STRATEGY_C or C3&#43;STRATEGY_C. |
| sources | [string](#string) | repeated | Source IDs. |
| adapters | [string](#string) | repeated | Adapter names. |
| max_input_wei | [string](#string) |  | Largest BUY input, decimal wei. |
| max_spend_wei | [string](#string) |  | Whole spend limit, decimal wei. |
| max_attempts | [uint32](#uint32) |  | BUY attempts the grant may take. |
| expires_unix_ms | [uint64](#uint64) |  | Expiry, unix milliseconds. |
| scope_sha256 | [string](#string) |  | SHA-256 of the canonical scope text the operator was shown, lowercase hex. |
| exit_only | [bool](#bool) |  | A separate authorization of the exits of open positions only, no BUY (BSC3-058). |
| risk | [LiveGrantRisk](#darkbot-bsc-v1-LiveGrantRisk) |  | The execution risk the operator grants (BSC3-069), every bound; required. |
| limits | [LiveGrantLimits](#darkbot-bsc-v1-LiveGrantLimits) |  | The positions and losses the operator grants (31.10. order), every bound; required. |






<a name="darkbot-bsc-v1-BscLiveState"></a>

### BscLiveState
The BSC LIVE tab (plan 28.10.): the owner&#39;s LIVE gate with every precondition and its
reason, the grant, attempts and spend, the LIVE path of the runtime, positions,
transactions and the net result. A view of the native owner and ledger, never a permission.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| gate | [string](#string) |  | CLEAR when the gate holds now, BLOCKED otherwise, UNKNOWN without a readable owner state. |
| blockers | [string](#string) | repeated | The gate&#39;s block reasons in order; the first decides. |
| preconditions | [LivePrecondition](#darkbot-bsc-v1-LivePrecondition) | repeated | Every precondition with whether it holds. |
| grant | [LiveGrant](#darkbot-bsc-v1-LiveGrant) |  | The grant on file; absent without one. |
| generation | [uint64](#uint64) |  | The owner generation: the compare-and-swap of a grant, revoke or limit request. |
| runtime_mode | [ControlSwitch](#darkbot-bsc-v1-ControlSwitch) |  | The runtime mode BSC_RUNTIME_MODE as it holds now. |
| economic_live | [ControlSwitch](#darkbot-bsc-v1-ControlSwitch) |  | The economic LIVE switch BSC_ECON_LIVE. |
| daily | [BuyAttemptLimit](#darkbot-bsc-v1-BuyAttemptLimit) |  | The daily BUY attempt limit and today&#39;s use. |
| total_limit | [uint64](#uint64) |  | The campaign&#39;s total BUY attempt limit (at most buy_attempts_total_max) and its use. |
| total_used | [uint64](#uint64) |  | Total attempts taken, every day and restart counted. |
| grant_attempts_used | [uint64](#uint64) |  | Attempts the current grant took. |
| spent_wei | [string](#string) |  | What the current grant has spent at most (admitted intents), decimal wei. |
| qualification | [string](#string) |  | VERIFIED, NONE or INVALID: the LIVE qualification on file. |
| qualification_detail | [string](#string) |  | Why it is not VERIFIED, or its SHA-256. |
| path_state | [string](#string) |  | The runtime&#39;s LIVE path: OFF, BLOCKED, ARMED, ENDED and the arming states. |
| path_reason | [string](#string) |  | Why the path is in that state. |
| route | [string](#string) |  | BUILDERS or LOOPBACK. |
| buyer | [string](#string) |  | The buyer wallet, 0x hex. |
| buyer_balance_wei | [string](#string) | optional | The buyer&#39;s balance at the last probe, decimal wei. |
| released | [uint64](#uint64) |  | Transactions the path released, and of them UNKNOWN. |
| unknown | [uint64](#uint64) |  | Released transactions whose handoff did not complete. |
| last_refusal | [string](#string) |  | The owner&#39;s or the release&#39;s last reason to refuse an intent. |
| positions | [LivePosition](#darkbot-bsc-v1-LivePosition) | repeated | The ledger&#39;s positions. |
| transactions | [LiveTransaction](#darkbot-bsc-v1-LiveTransaction) | repeated | The ledger&#39;s transactions, newest last. |
| realized_pnl_wei | [string](#string) |  | Realized PnL of closed and failed positions, signed decimal wei. |
| gas_wei | [string](#string) |  | All gas the ledger&#39;s transactions paid, decimal wei. |
| warnings | [string](#string) | repeated | Unreadable inputs, made explicit. |
| scope | [ControlSwitch](#darkbot-bsc-v1-ControlSwitch) | repeated | Every entity the grant names (runtime, components, venue, sources) as requested and as it holds now under the gate: LIVE only while the gate is clear and the scope names it. |
| grantable_sources | [string](#string) | repeated | The ingress sources a grant can name: those the runtime configuration gives an adapter endpoint (BSC_INGRESS_&lt;ID&gt;_WSS); a source without one never sights a launch and can never be qualified. In source order; empty without a readable runtime configuration. |
| campaign | [LiveCampaign](#darkbot-bsc-v1-LiveCampaign) |  | REV.1.0 R2b: the LIVE campaign verdict as its record (bsc_runtime live attempts) holds it; absent before any evaluation. Campaign evidence only, never a release or a permission. |
| attempts | [LiveAttempt](#darkbot-bsc-v1-LiveAttempt) | repeated | The evaluated LIVE BUY attempts in their order, as their records hold them; at most the campaign&#39;s total BUY attempt limit (MAX_BUY_ATTEMPTS_TOTAL), more is a warning. Beyond it, the shown ones are the first by ordinal of the limit plus one first by transaction. |
| exit_undecided | [string](#string) | repeated | REV.1.0 R1/R3b: the open LIVE lots no strategy holds yet (their curve being read), as the runtime&#39;s status names them, 0x token. |
| stranded | [string](#string) | repeated | The open LIVE lots no strategy exit can take, &#34;0x&lt;token&gt; &lt;REASON&gt;&#34;. |
| lot_read_faults | [uint64](#uint64) |  | The curve reads of open lots that failed in the runtime&#39;s process. |
| lot_read_fault | [string](#string) |  | The last of those failures; empty without one. |
| recovery_pending | [string](#string) | repeated | REV.1.0 R3b (BSC3-024/041): the ledger&#39;s unresolved transactions the runtime&#39;s receipt recovery asked about and has not resolved, &#34;0x&lt;tx&gt; &lt;REASON&gt;&#34;. |
| recovered | [uint64](#uint64) |  | The transactions the receipt recovery resolved in the runtime&#39;s process. |
| recovery_faults | [uint64](#uint64) |  | The receipt recovery&#39;s failed reads in the runtime&#39;s process. |
| recovery_fault | [string](#string) |  | The last of those failures; empty without one. |
| ledger_version | [uint64](#uint64) | optional | REV.1.0 R3b: the LIVE ledger&#39;s version, its next entry&#39;s number (never reused): the compare-and-swap (config_version) of an operator&#39;s lot decision or nonce burn, read before the positions and transactions shown; absent while the ledger is unreadable. |
| operator_exit | [bool](#bool) | optional | BSC_EXEC_LIVE_OPERATOR_EXIT of the runtime configuration: a lot&#39;s EXIT_NOW is locked while false; absent without a readable configuration with the executor. |
| nonce_burn | [bool](#bool) | optional | BSC_EXEC_LIVE_NONCE_BURN of the runtime configuration: a nonce burn is locked while false; absent without a readable configuration with the executor. |
| first_unreadable | [uint64](#uint64) | optional | The lowest sequence number of the ledger&#39;s entries that do not decode: the one the operator may quarantine next (bsc.live.ledger.quarantine); the warning LIVE_LEDGER_INVALID counts them all. Absent while every entry decodes. |
| campaign_spent_wei | [string](#string) | optional | REV.1.0 R4: what the campaign has spent at most, every grant&#39;s admissions (decimal wei); absent while the spend is unreadable. |
| campaign_spend_max_wei | [string](#string) | optional | The campaign&#39;s cumulative budget every LIVE BUY with its exits is admitted within (BSC_EXEC_LIVE_SPEND_TOTAL_WEI, decimal wei; 0: no BUY); absent without a readable runtime configuration with an executor. |
| risk_soft_buy_slippage_bps | [uint32](#uint32) | optional | REV.1.0 R4 (BSC3-069): the runtime&#39;s soft bound of a grant&#39;s BUY slippage (bps, BSC_EXEC_LIVE_RISK_SOFT_BUY_SLIPPAGE_BPS): above it a grant is an outlier; absent without a readable runtime configuration with an executor. |
| risk_soft_exit_slippage_bps | [uint32](#uint32) | optional | The soft bound of a grant&#39;s exit slippage (bps, BSC_EXEC_LIVE_RISK_SOFT_EXIT_SLIPPAGE_BPS). |
| risk_soft_gas_price_wei | [uint64](#uint64) | optional | The soft bound of a grant&#39;s gas price (wei, BSC_EXEC_LIVE_RISK_SOFT_GAS_PRICE_WEI). |
| risk_outlier | [bool](#bool) | optional | BSC_EXEC_LIVE_RISK_OUTLIER: a grant confirmed as an outlier admits BUYs only while true. |
| c3_rearm | [bool](#bool) | optional | REV.1.0 R4 (BSC3-042, BSC_EXEC_LIVE_C3_REARM): true renews the arm after a C3 BUY whose nonce the chain took; false: one C3 BUY per arm. |
| builder_policy_fresh_until_unix_ms | [uint64](#uint64) | optional | REV.1.0 R4: until when the bound builder policy (BSC_EXEC_LIVE_BUILDER_POLICY) is fresh (unix ms); absent for an import, an unreadable one or without a readable runtime configuration. |
| builder_policy_refusal | [string](#string) | optional | Why it admits no LIVE BUY at the report&#39;s clock (stale, unverified, the signed gas price below its floor); absent while it holds or without a readable runtime configuration or clock. |
| exit_reprice_bps | [uint32](#uint32) | optional | REV.1.0 R5 (BSC3-030, BSC_EXEC_LIVE_EXIT_REPRICE_BPS): a lost exit step goes again at its nonce at its price raised by these bps, at most the grant&#39;s max gas price; 0: never re-priced. |
| public_exit | [bool](#bool) | optional | REV.1.0 R5 (BSC3-030, BSC_EXEC_LIVE_PUBLIC_EXIT): true sends every exit step, once handed to the builders, also to the configured public JSON-RPC endpoint (the public mempool); never a BUY. |
| dex_exit | [bool](#bool) | optional | REV.1.0 R5 (BSC3-030, BSC_EXEC_LIVE_DEX_EXIT): true exits a lot whose token migrated to its DEX at the Portal&#39;s quote (routed through its DEX router); false: it stays stranded for the operator. |
| next_nonce | [uint64](#uint64) | optional | REV.1.0 R10 (Audit D): the LIVE buyer&#39;s next nonce at the LIVE path&#39;s last probe; absent unless a current runtime status (the owner&#39;s freshness rule) reports it. |
| armed_generation | [uint64](#uint64) | optional | The owner grant generation the LIVE path is armed under; absent unless a current status reports it ARMED. |
| refusals | [string](#string) | repeated | The LIVE path&#39;s refusals by stage, `NAME=n`. |
| responses | [string](#string) | repeated | The providers&#39; answers to the LIVE releases, `OUTCOME=n` (BSC3-051). |
| included | [uint64](#uint64) |  | C2&#39;s evidence on the LIVE path&#39;s intents: included. |
| reverted | [uint64](#uint64) |  | C2&#39;s evidence on the LIVE path&#39;s intents: reverted. |
| replaced | [uint64](#uint64) |  | C2&#39;s evidence on the LIVE path&#39;s intents: replaced (another transaction took the nonce). |
| finalized | [uint64](#uint64) |  | C2&#39;s evidence on the LIVE path&#39;s intents: final. |
| orphaned | [uint64](#uint64) |  | C2&#39;s evidence on the LIVE path&#39;s intents: orphaned (the block left the canonical chain). |
| probes | [uint64](#uint64) |  | The LIVE path&#39;s read-only probes. |
| ledger_error | [string](#string) |  | Why the runtime could not read the LIVE ledger (then its counters do not count); empty: read. |
| latency_samples | [uint64](#uint64) |  | T_ingress→T_handoff of the LIVE releases (BSC3-043): the kept samples. |
| latency_p50_ns | [uint64](#uint64) | optional | T_ingress→T_handoff p50 (ns); absent without a sample. |
| latency_p99_ns | [uint64](#uint64) | optional | T_ingress→T_handoff p99 (ns); absent without a sample. |
| latency_max_ns | [uint64](#uint64) | optional | T_ingress→T_handoff maximum (ns); absent without a sample. |
| lease_attempt | [string](#string) |  | The attempt the next C3 BUY takes (BSC3-078), `0x` hex; empty: none, or no current status reports the path ARMED. |
| divergence_quarantine | [bool](#bool) | optional | REV.1.0 R7 (BSC3-056, BSC_EXEC_LIVE_DIVERGENCE_QUARANTINE): true refuses every send, exits too, while the second source DISAGREES. |
| confirm_state | [string](#string) |  | The second source&#39;s verdict now (AGREED, STALE, BEHIND, DISAGREES, NO_COMMON_HEIGHT); empty: none configured or no runtime status. |
| presence_anchor | [string](#string) | optional | REV.1.0 R9 (BSC3-001, BSC_EXEC_LIVE_PRESENCE_ANCHOR): the anchor address; empty: OFF. |
| presence_signer | [string](#string) |  | The key that signed the grant&#39;s presence digest for its generation; empty: unsigned. |
| qualification_risk_scope_sha256 | [string](#string) |  | REV.1.0 R8 (BSC3-049): the grant risk scope the qualification&#39;s risk decision is made for. |
| qualification_observed_unix_ms | [uint64](#uint64) | optional | The qualification&#39;s age: the oldest of its record, evidence and risk decision (unix ms). |
| qualification_max_age_ms | [uint64](#uint64) | optional | BSC_EXEC_LIVE_QUALIFICATION_MAX_AGE_MS (0: no bound): an older qualification admits no BUY. |
| buy_attempts_total_max | [uint32](#uint32) | optional | Audit E 3.1 (BSC_BUY_ATTEMPTS_TOTAL_MAX): the most BUY attempts a UTC day or the whole campaign may take, and the most a grant may name; absent: no readable runtime configuration. |
| grant_venues | [string](#string) | repeated | Audit E 4.9: the venues a grant can name, as the native owner takes them, in its order. The LIVE tab offers exactly these; never a frontend list. |
| grant_strategies | [string](#string) | repeated | Audit E 4.9: the strategy paths a grant can name, as the native owner takes them. |
| grant_adapters | [string](#string) | repeated | Audit E 4.9: the inclusion adapters a grant can name, as the native owner takes them. |






<a name="darkbot-bsc-v1-BscMonitoringState"></a>

### BscMonitoringState
Read-only G0.8 results; submission acknowledgement is not acquisition.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| state | [string](#string) |  | G0.8 verdict unchanged: NOT_EXECUTED, PASS, ADAPT, STOP, UNRESOLVED or UNKNOWN. |
| evidence | [Evidence](#darkbot-bsc-v1-Evidence) |  | G0.8 result identity. |
| results | [PublicField](#darkbot-bsc-v1-PublicField) | repeated | Explicit public receipt, ordering, acquisition and cost fields. |
| records | [NativeRecord](#darkbot-bsc-v1-NativeRecord) | repeated | Latest public builder, witness, G0.7 attempt, G0.8 and chain-session records. |






<a name="darkbot-bsc-v1-BscStatus"></a>

### BscStatus
The BSC evidence projection: guard, gates, readiness, builder, witness, DRY and jobs. LIVE is
not projected here: bsc.live.state (BscLiveState) is its one source.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| source_commit | [string](#string) |  | Source revision of the deployed snapshot (its COMMIT); UNKNOWN without a readable one. |
| evaluated_unix_s | [uint64](#uint64) |  | Projection evaluation time, not qualification freshness. |
| guard | [GuardStatus](#darkbot-bsc-v1-GuardStatus) |  | Compiled guard and current contract observation. |
| gates | [GateStatus](#darkbot-bsc-v1-GateStatus) | repeated | Separate qualification gates. |
| readiness | [ReadinessStatus](#darkbot-bsc-v1-ReadinessStatus) |  | Latest native readiness evidence. |
| builder | [BuilderStatus](#darkbot-bsc-v1-BuilderStatus) |  | Builder observations. |
| witness | [WitnessStatus](#darkbot-bsc-v1-WitnessStatus) |  | Producer witness status. |
| dry | [DryStatus](#darkbot-bsc-v1-DryStatus) |  | Historical DRY status and exact values. |
| jobs | [JobStatus](#darkbot-bsc-v1-JobStatus) | repeated | Read-only jobs observed by launchd. |
| warnings | [string](#string) | repeated | Missing or invalid data is made explicit. |
| data_as_of_unix_s | [uint64](#uint64) |  | Time of the explicit source snapshot sync (its SYNCED_AT), never evidence freshness; 0 is UNKNOWN (no readable sync time, warning SNAPSHOT_SYNC_TIME_UNKNOWN). |






<a name="darkbot-bsc-v1-BscWalletsState"></a>

### BscWalletsState
Public wallet identity and timestamped evidence balances, never keys.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| expected_address | [string](#string) |  | The configured LIVE buyer (BSC_LIVE_BUYER of the runtime configuration), 0x hex; empty without one. |
| wallet_status | [string](#string) |  | WATCHED: the runtime configuration names that LIVE buyer and C2 watches it; UNKNOWN without a readable LIVE configuration. |
| launcher_address | [string](#string) |  | Launcher address from bound evidence. |
| buyer_address | [string](#string) |  | Buyer address from bound evidence. |
| chain_id | [uint64](#uint64) | optional | Chain identity when recorded. |
| launcher_balance_wei | [string](#string) | optional | Exact balance snapshot; absent if unknown. |
| buyer_balance_wei | [string](#string) | optional | Exact balance snapshot; absent if unknown. |
| observed_block | [uint64](#uint64) | optional | Block of the balance snapshot. |
| observed_unix_s | [uint64](#uint64) |  | Time of the balance snapshot, not reporter time. |
| source | [string](#string) |  | READINESS_SNAPSHOT or HISTORICAL_PREFLIGHT; no implicit live RPC. |
| required_launcher_wei | [string](#string) | optional | Exact launcher funding requirement when recorded. |
| required_buyer_wei | [string](#string) | optional | Exact buyer funding requirement when recorded. |
| evidence | [Evidence](#darkbot-bsc-v1-Evidence) |  | Evidence identity makes snapshot age and provenance reviewable. |
| launcher_control | [ControlSwitch](#darkbot-bsc-v1-ControlSwitch) |  | The launcher wallet&#39;s native ENABLE/DISABLE switch; absent before the owner state exists. |
| buyer_control | [ControlSwitch](#darkbot-bsc-v1-ControlSwitch) |  | The buyer wallet&#39;s native ENABLE/DISABLE switch; absent before the owner state exists. |
| live_balance_wei | [string](#string) | optional | REV.1.0 R10 (Audit D): the LIVE buyer&#39;s balance (decimal wei) at the LIVE path&#39;s last probe; absent unless a current runtime status (the owner&#39;s freshness rule) reports it. |
| live_next_nonce | [uint64](#uint64) | optional | The LIVE buyer&#39;s next nonce at the LIVE path&#39;s last probe; absent unless a current runtime status reports it. |






<a name="darkbot-bsc-v1-BuilderStatus"></a>

### BuilderStatus
Builder documentation and empirical qualification are separate observations.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| refresh_observed_unix_s | [uint64](#uint64) |  | Actual fetch time recorded in evidence. |
| refresh_expires_unix_s | [uint64](#uint64) |  | Refresh expires after 24 hours; zero means unknown. |
| floors | [PublicField](#darkbot-bsc-v1-PublicField) | repeated | Explicit documented floors in exact decimal wei. |
| probe | [Evidence](#darkbot-bsc-v1-Evidence) |  | Probe evidence identity. |
| price | [string](#string) |  | QUALIFIED or UNQUALIFIED; a response alone does not qualify price. |
| no_merge | [string](#string) |  | Empirical qualification of noMerge effects. |
| position_first | [string](#string) |  | Empirical qualification of positionFirst effects. |
| inclusion | [string](#string) |  | Empirical qualification of actual inclusion. |






<a name="darkbot-bsc-v1-BuyAttemptLimit"></a>

### BuyAttemptLimit
The persisted daily limit of economic BUY attempts (plan 27.10.) at the native owner and
its use on the current UTC day; every attempt that may send takes a slot before its
authority is consumed. A limit is never a permission to execute.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| per_day | [uint64](#uint64) |  | Attempts allowed per UTC calendar day; 0 (initially) allows none. |
| day_utc | [string](#string) |  | The UTC calendar day counted, YYYY-MM-DD. |
| used | [uint64](#uint64) |  | Slots taken on that day, sent or unclear attempts included. |
| remaining | [uint64](#uint64) |  | per_day minus used, never below zero. |
| generation | [uint64](#uint64) |  | The owner generation; the compare-and-swap of the next request. |
| request_id | [string](#string) |  | The request that last set the limit; empty while never set. |
| set_generation | [uint64](#uint64) | optional | The generation that request wrote. |






<a name="darkbot-bsc-v1-ControlSwitch"></a>

### ControlSwitch
One native wallet or economic switch (plan 27.10.) of the bsc_runtime owner: a permission
a Mainnet signing or sending path checks besides its own guard, V5, nonce and budget checks,
never an action. DISABLED initially, in a missing or unreadable state and after every restart.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| id | [string](#string) |  | Native ID: BSC_WALLET_LAUNCHER, BSC_WALLET_BUYER, BSC_WALLET_MAIN, BSC_ECON_SIGN, BSC_ECON_ARM, BSC_ECON_SEND, BSC_ECON_ATTEMPT or BSC_ECON_LIVE. |
| requested | [string](#string) |  | DISABLED or ENABLED as requested; UNKNOWN when the owner state is unreadable. |
| effective | [string](#string) |  | DISABLED, ENABLED, or BLOCKED as the owner holds it in this boot session. |
| block_reason | [string](#string) |  | Why the effective state is not the requested one (RESET_BY_RESTART, CONTROL_STATE_INVALID). |
| generation | [uint64](#uint64) |  | The owner generation; the compare-and-swap of the next request. |
| request_id | [string](#string) |  | The request (or RESTART_RESET) that last set this switch; empty while never set. |
| set_generation | [uint64](#uint64) | optional | The generation that request wrote. |






<a name="darkbot-bsc-v1-DryStatus"></a>

### DryStatus
Historical DRY parameters; reporting never runs signing or simulation.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| evidence | [Evidence](#darkbot-bsc-v1-Evidence) |  | Native DRY evidence identity. |
| dry_id | [string](#string) |  | Evidence run identity. |
| state | [string](#string) |  | PASS, FAIL or UNKNOWN. |
| parameters | [PublicField](#darkbot-bsc-v1-PublicField) | repeated | Exact public parameters and results from the record. |
| valid_for_current_portal | [bool](#bool) | optional | Absent without a qualified current deployment comparison. |






<a name="darkbot-bsc-v1-Evidence"></a>

### Evidence
Source identity of immutable evidence. Missing evidence is not a pass.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| path | [string](#string) |  | Path relative to the staged BSC source root. |
| sha256 | [string](#string) |  | SHA-256 of the exact original bytes. |
| observed_unix_s | [uint64](#uint64) |  | Time recorded by the producer, not file mtime. |






<a name="darkbot-bsc-v1-GateStatus"></a>

### GateStatus
Qualification status derived from identified evidence.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| gate_id | [string](#string) |  | G0.6a, G0.7, G0.8, C1.9 or C1.10. |
| state | [string](#string) |  | Native vocabulary, transported unchanged. G0.6a: PASS, FAIL or UNKNOWN. G0.7: NOT_EXECUTED, UNKNOWN or the attempt classification (PROVISIONAL_RELEASED__*, SUBMITTED_OR_SUBMISSION_UNKNOWN__*, STOP_CAPTURE_INCOMPLETE__*), none of which is a success. G0.8: NOT_EXECUTED, UNKNOWN, PASS, ADAPT, STOP or UNRESOLVED. C1.x: UNKNOWN. |
| evidence | [Evidence](#darkbot-bsc-v1-Evidence) |  | Original evidence identity. |
| reason_codes | [string](#string) | repeated | Reasons explaining this observation. |






<a name="darkbot-bsc-v1-GuardStatus"></a>

### GuardStatus
Public guard observation. This message cannot authorize execution.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| frozen_approved_sha256 | [string](#string) |  | Compiled authority hash or UNARMED. |
| guard_state | [string](#string) |  | UNARMED, ARMED, ARMED_MISMATCH or UNKNOWN. |
| contract_path | [string](#string) |  | Only the current V5 contract path is authoritative. |
| contract_present | [bool](#bool) |  | Whether the current contract exists. |
| contract_sha256 | [string](#string) |  | Hash of the current contract bytes, if present. |
| approved | [bool](#bool) | optional | Absent if no valid contract can be inspected. |
| execution_ready | [bool](#bool) | optional | Absent if no valid contract can be inspected. |
| attempt_id | [string](#string) |  | Bound attempt identity. |
| max_attempts | [uint64](#uint64) | optional | Bound attempt maximum. |






<a name="darkbot-bsc-v1-JobStatus"></a>

### JobStatus
Supervisor-owned read-only job lifecycle.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| job | [string](#string) |  | Only readiness is currently controllable. |
| state | [string](#string) |  | Process lifecycle only: RUNNING, STOPPING, STOPPED (ended; see result_code), FAILED (runner or process failure) or UNKNOWN. Never a readiness verdict. |
| launch_id | [string](#string) |  | Unique launch identity; never reused across runs. |
| started_unix_s | [uint64](#uint64) |  | Start time recorded by the controller. |
| ended_unix_s | [uint64](#uint64) |  | Completion time recorded by the controller. |
| result_code | [string](#string) |  | Result of this launch, not an earlier one: the native verdict PASS_READ_ONLY, BLOCKED or STOP, STOPPED when stopped on request, FAILED on a technical failure. |
| controller_available | [bool](#bool) | optional | Whether the supervisor can control the job (deployed readiness binary and snapshot). |
| pid | [uint32](#uint32) | optional | Audit E 4.13: the job&#39;s process while launchd runs it. |






<a name="darkbot-bsc-v1-LiveAttempt"></a>

### LiveAttempt
One evaluated LIVE BUY attempt (REV.1.0 R2), unchanged from its record. T0 is never the
producer&#39;s unless the record says t0_exact: without its witness the timing verdict is UNKNOWN.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| ordinal | [uint64](#uint64) |  | Its place among the evaluated BUYs, from 1. |
| tx | [string](#string) |  | The BUY&#39;s transaction hash, 0x hex. |
| kind | [string](#string) |  | C3_BUY or STRATEGY_BUY. |
| cell | [string](#string) |  | EXTERNAL_FULL_LAUNCH, SELF_LAUNCH or STRATEGY_ENTRY. |
| state | [string](#string) |  | RELEASED, UNKNOWN, INCLUDED, REVERTED, REPLACED or DROPPED. |
| verdict | [string](#string) |  | The attempt&#39;s verdict: PASS, FAIL, UNKNOWN or NOT_APPLICABLE. |
| acquisition | [string](#string) |  | The acquisition&#39;s verdict. |
| canonical | [string](#string) |  | The canonical inclusion&#39;s verdict. |
| timing | [string](#string) |  | The producer&#39;s T0 to T_buy: UNKNOWN without its witness. |
| placement | [string](#string) |  | The placement&#39;s verdict. |
| capture | [string](#string) |  | The capture&#39;s verdict. |
| local | [string](#string) |  | The local start deadline and a qualified send time. |
| economics | [string](#string) |  | The economics&#39; verdict. |
| reasons | [string](#string) | repeated | Why, in the evaluator&#39;s order; at most the attempt&#39;s share of the state (LIVE_ATTEMPT_REASONS_BEYOND_LIMIT counts the rest). |
| window_why | [string](#string) |  | Why the window is not complete; empty when complete. |
| trades | [uint64](#uint64) |  | Trades of the token in the window. |
| buys | [uint64](#uint64) |  | Of them BUYs. |
| sells | [uint64](#uint64) |  | Of them SELLs. |
| first_external_price | [string](#string) | optional | Native wei per 10^18 token units of the first other trader&#39;s BUY, decimal; absent without one. |
| price_vs_first_external_ppm | [uint64](#uint64) | optional | Its own price per token against that BUY&#39;s, ppm; absent without both. |
| ingress_to_send_ns | [uint64](#uint64) | optional | Local timing on the runtime&#39;s clock, ingress (or decision) to the first qualified send, ns. |
| select_to_send_ns | [uint64](#uint64) | optional | Selection to the first qualified send, ns. |
| send_to_ack_ns | [uint64](#uint64) | optional | The first qualified send to the first acknowledgement, ns. |
| attempts | [uint64](#uint64) |  | Counters of every attempt up to this one: the LIVE BUYs (C3 and Strategy C, before the commitment too), the acquisition rate&#39;s denominator. |
| acquired | [uint64](#uint64) |  | Of them acquired. |
| passed | [uint64](#uint64) |  | C3 BUYs among them whose verdict is PASS (a count, not a rate: a Strategy C BUY has no verdict, NOT_APPLICABLE). |
| failed | [uint64](#uint64) |  | C3 BUYs among them whose verdict is FAIL. |
| undecided | [uint64](#uint64) |  | C3 BUYs among them whose verdict is UNKNOWN. |
| adapters | [LiveAttemptAdapter](#darkbot-bsc-v1-LiveAttemptAdapter) | repeated | The adapters of its dispatch. |
| window | [string](#string) |  | The 15-s market window from the launch&#39;s block as written: COMPLETE or INCOMPLETE. |
| attempt | [string](#string) |  | The attempt&#39;s identity (its lease attempt, or its hash), 0x hex. |
| run | [uint64](#uint64) | optional | The run whose journal holds its release; absent without a release record. |
| token | [string](#string) |  | The token, 0x hex. |
| launch_hash | [string](#string) |  | The launch&#39;s transaction hash, 0x hex; empty without a launch (Strategy C). |
| trigger | [string](#string) |  | The source whose sighting triggered it (a C3 BUY); empty without one. |
| block_number | [uint64](#uint64) | optional | Its block&#39;s number; absent while not included. |
| block_hash | [string](#string) |  | Its block&#39;s hash, 0x hex; empty while not included. |
| block_ms | [uint64](#uint64) | optional | Its block&#39;s header time, unix ms; absent while not included. |
| tokens | [string](#string) | optional | Tokens its own Portal BUY acquired, decimal token units; absent when unknown. |
| eth_wei | [string](#string) | optional | The native amount its included BUY spent, decimal wei; absent when unknown. |
| gas_wei | [string](#string) | optional | Gas its inclusion paid, decimal wei; absent when unknown. |
| own_price | [string](#string) | optional | Its own price, native wei per 10^18 token units, decimal; absent without its own trade. |
| placed | [string](#string) |  | The ledger&#39;s PLACED fact: FIRST, LATE, OTHER_BLOCK or NO_TOKENS; empty without one. |
| launch_index | [uint64](#uint64) | optional | The launch&#39;s transaction index in its block; absent when the launch is not in it. |
| own_index | [uint64](#uint64) | optional | Its own transaction index in its block; absent without a PLACED fact. |
| buys_before | [uint64](#uint64) | optional | Other traders&#39; BUYs of the token between the launch and it; absent without a PLACED fact. |
| clock_id | [uint64](#uint64) | optional | The runtime clock the local times are on; absent without a release record. |
| t_ingress_ns | [uint64](#uint64) | optional | Ingress (C3) or decision (Strategy C) on that clock, ns; absent without a release record. |
| t_select_ns | [uint64](#uint64) | optional | Selection on that clock, ns; absent without a release record. |
| t_send_ns | [uint64](#uint64) | optional | The first qualified send among its adapters on that clock, ns; absent without one. |
| t_ack_ns | [uint64](#uint64) | optional | The first acknowledgement on that clock, ns; absent without one. |
| deadline_misses | [uint64](#uint64) | optional | Local start deadlines it missed; absent without a release record. |
| window_ms | [uint64](#uint64) |  | The market window&#39;s length, ms. |
| launch_block | [uint64](#uint64) | optional | The launch&#39;s block; absent without one. |
| launch_ms | [uint64](#uint64) | optional | The launch block&#39;s header time, unix ms (the window&#39;s start); absent without one. |
| window_end_block | [uint64](#uint64) | optional | The last block inside the window; absent while the window is not over. |
| t0_exact | [bool](#bool) |  | Whether T0 is the producer&#39;s exact time (never so without its witness). |
| first_external_buy_tx | [string](#string) |  | The first other trader&#39;s BUY in the window, 0x hex; empty without one. |
| first_external_buy_block | [uint64](#uint64) | optional | Its block; absent without one. |
| first_external_buy_index | [uint64](#uint64) | optional | Its transaction index in that block; absent without one. |
| buy_wei | [string](#string) |  | The native volume of the token&#39;s BUYs in the window (its own too), decimal wei. |
| sell_wei | [string](#string) |  | The native volume of the token&#39;s SELLs in the window, decimal wei. |
| evaluated_unix_ms | [uint64](#uint64) |  | When its evaluation wrote the record, unix ms: the campaign verdict&#39;s evaluated_unix_ms when both are of one evaluation. |






<a name="darkbot-bsc-v1-LiveAttemptAdapter"></a>

### LiveAttemptAdapter
One adapter of a LIVE attempt&#39;s dispatch and its answer.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| builder | [string](#string) |  | The granted builder, e.g. CLUB48_BUILDER_V1; empty when the record names none. |
| outcome | [string](#string) |  | The dispatch outcome at this adapter. |
| answer | [string](#string) |  | The builder&#39;s answer; empty without one. |
| adapter | [uint64](#uint64) |  | The adapter&#39;s index in the runtime&#39;s dispatch. |
| boundary | [string](#string) |  | The boundary its send time is taken at. |
| qualified | [bool](#bool) |  | Whether its record and its transport qualify that boundary as the send time. |
| t_write_check_ns | [uint64](#uint64) | optional | Its write check on the runtime&#39;s clock, ns; absent without one. |
| t_boundary_ns | [uint64](#uint64) | optional | Its boundary on the runtime&#39;s clock, ns; absent without one. |
| t_queue_ns | [uint64](#uint64) | optional | Its queueing on the runtime&#39;s clock, ns; absent without one. |
| t_answer_ns | [uint64](#uint64) | optional | Its answer on the runtime&#39;s clock, ns; absent without one. |






<a name="darkbot-bsc-v1-LiveCampaign"></a>

### LiveCampaign
The LIVE campaign verdict (REV.1.0 R2, master plan §13 J), unchanged from its record: the
evaluator decides, the reporter only reads. Every count&#39;s denominator is another count here.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| verdict | [string](#string) |  | CAMPAIGN_NOT_READY, CAMPAIGN_OPEN, CAMPAIGN_FAIL or CAMPAIGN_PASS. |
| release_evidence | [string](#string) |  | RELEASE_EVIDENCE_NOT_READY or RELEASE_EVIDENCE_PASS: the campaign evidence only; a release also needs the implementation matrix, runtime enforcement and LIVE authorization. |
| reasons | [string](#string) | repeated | Why the verdict is not a pass, in the evaluator&#39;s order; with skip_reasons at most the verdict&#39;s share of the state (LIVE_CAMPAIGN_REASONS_BEYOND_LIMIT counts the rest). |
| planned | [uint64](#uint64) |  | The enabled cells&#39; minimum attempts, summed. |
| observed | [uint64](#uint64) |  | The canonical C1-scope Flap launches since the commitment. |
| selected | [uint64](#uint64) |  | Of them seen in full before their block (actionable). |
| admitted | [uint64](#uint64) |  | C3 BUYs the owner admitted (written and withdrawn). |
| skipped | [uint64](#uint64) |  | Observed launches without an attempt. |
| written | [uint64](#uint64) |  | C3 BUYs written after the commitment: the campaign&#39;s attempts. |
| acquired | [uint64](#uint64) |  | Of them acquired. |
| reverted | [uint64](#uint64) |  | Of them reverted. |
| dropped | [uint64](#uint64) |  | Of them dropped. |
| late | [uint64](#uint64) |  | Of them late or in another block. |
| unresolved | [uint64](#uint64) |  | Of them not resolved by the chain. |
| passed | [uint64](#uint64) |  | Of them passed. |
| failed | [uint64](#uint64) |  | Of them failed. |
| undecided | [uint64](#uint64) |  | Of them undecided. |
| consecutive_misses | [uint64](#uint64) |  | The longest run of observed launches without an attempt. |
| before_precommit | [uint64](#uint64) |  | C3 BUYs written before the commitment (not the campaign&#39;s). |
| unverifiable | [uint64](#uint64) |  | Launches whose transaction could not be verified (coverage unknown). |
| covered_s | [uint64](#uint64) |  | The time the campaign&#39;s sealed runs cover, seconds. |
| uncovered_s | [uint64](#uint64) |  | The rest of its span, seconds. |
| covered_ppm | [uint64](#uint64) |  | The covered share of its span, ppm. |
| skip_reasons | [string](#string) | repeated | The skipped launches by how they were seen, CLASS=count. |
| cells | [LiveCampaignCell](#darkbot-bsc-v1-LiveCampaignCell) | repeated | The cells of the manifest. |
| manifest_sha256 | [string](#string) |  | SHA-256 of the rated manifest, lowercase hex; empty without one. |
| precommit_sha256 | [string](#string) |  | SHA-256 of its precommitment record, lowercase hex; empty without one. |
| build_sha256 | [string](#string) |  | SHA-256 of the evaluating bsc_runtime build, lowercase hex (zero when unknown). |
| evaluated_unix_ms | [uint64](#uint64) |  | When the evaluation wrote the record, unix milliseconds. |
| record_sha256 | [string](#string) |  | SHA-256 of the verdict record&#39;s bytes, lowercase hex. |






<a name="darkbot-bsc-v1-LiveCampaignCell"></a>

### LiveCampaignCell
One cell of the LIVE campaign (REV.1.0 R2), as the verdict record holds it.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| cell | [string](#string) |  | EXTERNAL_FULL_LAUNCH or SELF_LAUNCH. |
| enabled | [bool](#bool) |  | Whether the manifest enables it. |
| min_attempts | [uint64](#uint64) | optional | The manifest&#39;s minimum attempts; absent when it names none. |
| attempts | [uint64](#uint64) |  | Its attempts. |
| passed | [uint64](#uint64) |  | Of them passed. |
| failed | [uint64](#uint64) |  | Of them failed. |
| undecided | [uint64](#uint64) |  | Of them undecided. |
| min_duration_s | [uint64](#uint64) | optional | The manifest&#39;s minimum duration, seconds; absent when it names none. |
| duration_s | [uint64](#uint64) |  | How long it ran, seconds. |
| verdict | [string](#string) |  | Its verdict. |






<a name="darkbot-bsc-v1-LiveGrant"></a>

### LiveGrant
The operator&#39;s LIVE grant of one scope as the native owner holds it.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| venue | [string](#string) |  | The venue, e.g. BSC_VENUE_FLAP. |
| strategies | [string](#string) |  | C3, STRATEGY_C or C3&#43;STRATEGY_C. |
| sources | [string](#string) | repeated | Source IDs whose verified full sightings may trigger a BUY. |
| adapters | [string](#string) | repeated | Inclusion adapters, e.g. CLUB48_BUILDER_V1. |
| max_input_wei | [string](#string) |  | Largest BUY input, decimal wei. |
| max_spend_wei | [string](#string) |  | The grant&#39;s whole spend limit (inputs and gas), decimal wei. |
| max_attempts | [uint32](#uint32) |  | BUY attempts the grant may take. |
| expires_unix_ms | [uint64](#uint64) |  | Expiry, unix milliseconds. |
| scope_sha256 | [string](#string) |  | SHA-256 of the canonical scope text the operator confirmed, lowercase hex. |
| scope_text | [string](#string) |  | The canonical scope text itself. |
| request_id | [string](#string) |  | The request that wrote the grant. |
| set_generation | [uint64](#uint64) |  | The owner generation that request wrote. |
| exit_only | [bool](#bool) |  | The grant admits the exits of open positions only, no BUY (BSC3-058). |
| risk | [LiveGrantRisk](#darkbot-bsc-v1-LiveGrantRisk) |  | The execution risk the operator granted (BSC3-069); absent in a grant written before it, which admits nothing. |
| limits | [LiveGrantLimits](#darkbot-bsc-v1-LiveGrantLimits) |  | The positions and losses the operator granted (31.10. order); absent in a grant written before them, which arms nothing. |
| risk_scope_sha256 | [string](#string) |  | REV.1.0 R10 (BSC3-049): SHA-256 of the grant&#39;s risk scope text, the scope a qualification&#39;s risk decision must name. |






<a name="darkbot-bsc-v1-LiveGrantLimits"></a>

### LiveGrantLimits
The positions and losses of a LIVE grant: a BUY beyond any of them is refused, exits go on.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| max_open_positions | [uint32](#uint32) |  | Most positions open at once (a BUY pending, open or exiting counts). |
| max_realized_loss_wei | [string](#string) |  | Most realized net loss of the grant&#39;s positions, decimal wei. |
| max_daily_realized_loss_wei | [string](#string) |  | Most realized net loss of the positions closed on one UTC day, decimal wei. |
| min_buy_interval_ms | [uint64](#uint64) |  | Least time between two BUYs, milliseconds. |






<a name="darkbot-bsc-v1-LiveGrantRisk"></a>

### LiveGrantRisk
The execution risk of a LIVE grant (BSC3-069): the executor arms only within it.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| max_buy_slippage_bps | [uint32](#uint32) |  | Most slippage a BUY may accept, basis points (below 10000). |
| max_exit_slippage_bps | [uint32](#uint32) |  | Most slippage an exit (SELL) may accept, basis points (below 10000). |
| max_gas_price_wei | [uint64](#uint64) |  | Most gas price a transaction may be signed with, wei. |
| max_buy_gas_limit | [uint64](#uint64) |  | Most gas limit of a BUY. |
| max_approve_gas_limit | [uint64](#uint64) |  | Most gas limit of an approval. |
| max_sell_gas_limit | [uint64](#uint64) |  | Most gas limit of a SELL. |
| outlier_ack | [bool](#bool) |  | REV.1.0 R4 (BSC3-069): the operator confirmed these bounds as an outlier (above the runtime&#39;s soft bounds); without it a grant above them never arms. |






<a name="darkbot-bsc-v1-LivePosition"></a>

### LivePosition
One LIVE position of the ledger: its BUY, exit and native result.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| token | [string](#string) |  | Token address, 0x hex. |
| component | [string](#string) |  | BSC_EXEC or BSC_STRATEGY_C. |
| state | [string](#string) |  | PENDING, OPEN, EXITING, CLOSED or FAILED. |
| tokens | [string](#string) |  | Tokens the BUY received, decimal. |
| input_wei | [string](#string) |  | The BUY&#39;s input, decimal wei. |
| proceeds_wei | [string](#string) |  | The SELL&#39;s proceeds, decimal wei. |
| gas_wei | [string](#string) |  | Gas of all its included or reverted transactions, decimal wei. |
| pnl_wei | [string](#string) | optional | Proceeds minus input minus gas once closed (failed: minus gas), signed decimal wei. |
| lot | [string](#string) |  | The lot: its BUY&#39;s transaction hash, 0x hex (a token bought again is another lot). |
| decisions | [string](#string) | repeated | REV.1.0 R3b: the operator&#39;s decisions the ledger takes of this lot now (RESUME, WRITE_OFF, EXIT_NOW; exit-now also needs BSC_EXEC_LIVE_OPERATOR_EXIT ON), by the ledger&#39;s own rule. |
| proceeds_unknown | [bool](#bool) |  | REV.1.0 R5: its SELL is included without the Portal&#39;s trade event (a DEX-routed SELL): the proceeds are unknown, counted as 0 (proceeds_wei 0; pnl_wei a lower bound, the worst case). |






<a name="darkbot-bsc-v1-LivePrecondition"></a>

### LivePrecondition
One condition of the native owner&#39;s LIVE gate (plan 28.10.), in the order an operator meets
them; LIVE is effective only while every one holds.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| id | [string](#string) |  | Stable condition ID, e.g. ECONOMIC_LIVE, LIVE_GRANT, WALLET_BUYER, ECON_SEND, RUNTIME_LIVE, BUY_LIMIT_DAILY, BUY_LIMIT_TOTAL, GRANT_ATTEMPTS, GRANT_SPEND, GRANT_BUYS, GRANT_RISK, VENUE_QUALIFIED, SOURCES_QUALIFIED, ADAPTERS_QUALIFIED, ACQUISITION_PROVEN or RISK_DECISION. |
| met | [bool](#bool) |  | Whether the condition holds now. |
| reason | [string](#string) |  | The owner&#39;s reason when it does not (the gate&#39;s own block reason). |






<a name="darkbot-bsc-v1-LiveTransaction"></a>

### LiveTransaction
One LIVE transaction whose bytes may have left, and what the chain made of it.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| tx_hash | [string](#string) |  | Transaction hash, 0x hex. |
| intent | [string](#string) |  | BUY, APPROVE, SELL or BURN (REV.1.0 R3b: a nonce burn, the buyer&#39;s transfer of nothing to itself). |
| token | [string](#string) |  | Token address, 0x hex. |
| nonce | [uint64](#uint64) |  | The buyer&#39;s nonce it used. |
| state | [string](#string) |  | RELEASED, UNKNOWN, WITHDRAWN (admitted, no byte left: never on chain), INCLUDED, REVERTED or REPLACED. |
| block_number | [uint64](#uint64) | optional | The block that included it. |
| source | [string](#string) |  | REV.1.0 R3b: RECEIPT when its chain fact comes from a block the receipt recovery found (final, the second source&#39;s; its receipt&#39;s, or where its sender&#39;s nonce passes it), not from the follower&#39;s block; empty otherwise. |
| burn_requested | [bool](#bool) |  | REV.1.0 R3b: the operator requested its nonce burnt (BURN_REQUESTED); the armed runtime burns it while BSC_EXEC_LIVE_NONCE_BURN is ON. |
| burnable | [bool](#bool) |  | REV.1.0 R3b: the ledger takes a nonce burn of it now (its signed bytes may still land, it is the configured LIVE buyer&#39;s), by the ledger&#39;s own rule; the switch aside. |






<a name="darkbot-bsc-v1-NativeField"></a>

### NativeField
One public leaf of a native record. Secrets, salts, raw bytes and private records never
appear; the census sensitivity of every leaf decides (bsc-native-fields.json).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| name | [string](#string) |  | DBREC field name; nested rows as parent[index].name, repeated leaves as name[index]. |
| kind | [NativeKind](#darkbot-bsc-v1-NativeKind) |  | DBREC kind of the leaf. |
| value | [string](#string) |  | Canonical text of the value for its kind. |
| unit | [string](#string) |  | Native unit such as wei, unix_milliseconds, nanoseconds or basis_points. |






<a name="darkbot-bsc-v1-NativeRecord"></a>

### NativeRecord
The public leaves of one identified native record. The frontend places every field by
frontend/contracts/bsc-fields.json (panel, section, order); nothing is computed from it.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| schema | [string](#string) |  | DBREC schema name. |
| evidence | [Evidence](#darkbot-bsc-v1-Evidence) |  | Record identity: path, SHA-256 and native observation time. |
| provenance | [string](#string) |  | NATIVE_CREATED, LEGACY_IMPORTED_AUDITED or UNKNOWN. |
| fields | [NativeField](#darkbot-bsc-v1-NativeField) | repeated | Public leaves in native schema order. |






<a name="darkbot-bsc-v1-PublicField"></a>

### PublicField
An explicitly selected public bound value, never a signed transaction or secret.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| name | [string](#string) |  | Stable name from the native evidence schema. |
| value | [string](#string) |  | Exact text; monetary integers use decimal wei, never floating point. |






<a name="darkbot-bsc-v1-ReadinessStatus"></a>

### ReadinessStatus
Native read-only readiness result and its original timestamp.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| verdict | [string](#string) |  | PASS_READ_ONLY, BLOCKED (a required condition is missing), STOP (a contradiction), NOT_RUN or UNKNOWN, unchanged from the native record; never LIVE authority. |
| evidence | [Evidence](#darkbot-bsc-v1-Evidence) |  | Source evidence, possibly historical. |
| funding_verdict | [string](#string) |  | Funding result, if reported. |
| checks | [PublicField](#darkbot-bsc-v1-PublicField) | repeated | Individual native checks and their public details. |
| portal_version_bound | [string](#string) |  | Bound portal version. |
| portal_version_observed | [string](#string) |  | Observed portal version. |
| portal_impl_bound | [string](#string) |  | Bound implementation address. |
| portal_impl_observed | [string](#string) |  | Observed implementation address. |
| code_hash_changed | [bool](#bool) | optional | Absent when no comparison was performed. |
| head_block | [uint64](#uint64) | optional | Block for the readiness observation, when recorded. |
| reasons | [string](#string) | repeated | Reported reasons for blocking. |
| mode | [string](#string) |  | READ_ONLY_RPC or OFFLINE_ZERO_EGRESS as recorded natively; empty without a record. |
| candidate_binding | [string](#string) |  | Binding to the current offline candidate selection: BOUND (observed after the switch, with a passing CANDIDATE_SELECTION check for it), STALE (older or another selection), NO_SELECTION, or UNKNOWN (no record or an invalid candidate mirror). |






<a name="darkbot-bsc-v1-WitnessStatus"></a>

### WitnessStatus
Original producer witness availability, not inferred timing.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| source_ready | [bool](#bool) | optional | Absent when the witness contract cannot be read. |
| t0_to_buy_state | [string](#string) |  | UNKNOWN until original execution witnesses prove timing. |
| evidence | [Evidence](#darkbot-bsc-v1-Evidence) |  | Witness contract identity. |








<a name="darkbot-bsc-v1-NativeKind"></a>

### NativeKind
Kind of a native leaf; the value is its canonical text form.

| Name | Number | Description |
| ---- | ------ | ----------- |
| NATIVE_KIND_UNSPECIFIED | 0 | Not set; never displayed as a value. |
| NATIVE_KIND_BOOL | 1 | true or false. |
| NATIVE_KIND_U64 | 2 | Unsigned decimal. |
| NATIVE_KIND_I64 | 3 | Signed decimal. |
| NATIVE_KIND_U256 | 4 | Exact unsigned decimal (wei, token units); never a float. |
| NATIVE_KIND_ADDRESS | 5 | 0x followed by 40 hex digits. |
| NATIVE_KIND_B256 | 6 | 0x followed by 64 hex digits. |
| NATIVE_KIND_TEXT | 7 | Redacted public text. |










<a name="darkbot_bsc_v1_common-proto"></a>
<p align="right"><a href="#top">Top</a></p>

## darkbot/bsc/v1/common.proto



<a name="darkbot-bsc-v1-OwnerProvenance"></a>

### OwnerProvenance
Who produced a V8 projection, from what, and how far it is proven.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| owner | [string](#string) |  | The native owner, e.g. &#34;bsc_runtime live ledger&#34;, &#34;bsc_runtime strategy settings&#34;. |
| owner_version | [uint64](#uint64) |  | The owner&#39;s version the values were read at (ledger version, settings generation, policy version); the snapshot a cursor and a compare-and-swap are bound to. |
| observed_unix_ms | [uint64](#uint64) |  | When the owner produced this projection, unix milliseconds. |
| evidence | [Evidence](#darkbot-bsc-v1-Evidence) |  | The native record it was read from; absent when it is the owner&#39;s in-memory state. |
| quality | [DataQuality](#darkbot-bsc-v1-DataQuality) |  | How far the values are proven. |
| quality_reasons | [string](#string) | repeated | Why the quality is not VERIFIED, in the owner&#39;s order. |
| provenance | [string](#string) |  | NATIVE_CREATED, LEGACY_IMPORTED_AUDITED or UNKNOWN, as NativeRecord.provenance. |






<a name="darkbot-bsc-v1-PerformancePoint"></a>

### PerformancePoint
One point of a performance series, from verified snapshots and trade facts only.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| at | [SourcedTime](#darkbot-bsc-v1-SourcedTime) |  | When the value holds. |
| value_wei | [string](#string) |  | The value at that time, exact (signed) decimal wei. |
| fact_id | [string](#string) |  | The fact that produced the point (trade lot, transfer request, balance snapshot); empty for a period boundary. |
| quality | [DataQuality](#darkbot-bsc-v1-DataQuality) |  | How far this point is proven. |






<a name="darkbot-bsc-v1-PerformanceSeries"></a>

### PerformanceSeries
A time series of one metric, reconstructible from the owner&#39;s facts; never a demo curve.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| availability | [Availability](#darkbot-bsc-v1-Availability) |  | Whether the native owner answered. |
| unavailable_reason | [string](#string) |  | Why it is not AVAILABLE. |
| provenance | [OwnerProvenance](#darkbot-bsc-v1-OwnerProvenance) |  | The owner and snapshot the series was read at. |
| metric | [PerformanceMetric](#darkbot-bsc-v1-PerformanceMetric) |  | The quantity. |
| scope | [PerformanceScope](#darkbot-bsc-v1-PerformanceScope) |  | What it is about. |
| scope_id | [string](#string) |  | The run, profile or wallet role the scope names; empty for LIVE and ALL_WALLETS. |
| points | [PerformancePoint](#darkbot-bsc-v1-PerformancePoint) | repeated | The points in time order. |
| opening_wei | [string](#string) | optional | The value before the first point (the period&#39;s opening balance or PnL), decimal wei; absent when the basis is unknown. |
| truncated | [bool](#bool) |  | The owner thinned the points to its cap (max_points or its own): the shape, not every fact. |






<a name="darkbot-bsc-v1-PerformanceSeriesQuery"></a>

### PerformanceSeriesQuery
A bounded read of one performance series.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| metric | [PerformanceMetric](#darkbot-bsc-v1-PerformanceMetric) |  | The quantity. |
| scope | [PerformanceScope](#darkbot-bsc-v1-PerformanceScope) |  | What it is about. |
| scope_id | [string](#string) |  | The run, profile or wallet role the scope names. |
| from_unix_ms | [uint64](#uint64) | optional | Start of the period, unix ms; absent: the owner&#39;s first fact. |
| to_unix_ms | [uint64](#uint64) | optional | End of the period, unix ms; absent: now. |
| max_points | [uint32](#uint32) |  | At most this many points; 0: the owner&#39;s default; the owner thins to its cap and says so (PerformanceSeries.truncated). |






<a name="darkbot-bsc-v1-SourcedTime"></a>

### SourcedTime
A point in time with the clock it was taken on.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| unix_ms | [uint64](#uint64) | optional | Unix milliseconds; absent while unknown. |
| source | [TimeSource](#darkbot-bsc-v1-TimeSource) |  | The clock of unix_ms. |
| block_number | [uint64](#uint64) | optional | The block it refers to, when it is a block time. |








<a name="darkbot-bsc-v1-Availability"></a>

### Availability
Whether the native owner answers for a V8 projection or action.

| Name | Number | Description |
| ---- | ------ | ----------- |
| AVAILABILITY_UNSPECIFIED | 0 | Not set by any producer; never displayed as a value. |
| AVAILABILITY_AVAILABLE | 1 | The native owner answered; the values are its own. |
| AVAILABILITY_UNAVAILABLE | 2 | The owner exists but cannot answer now (reason given): its state unreadable, stale or detached. |
| AVAILABILITY_NOT_IMPLEMENTED | 3 | No native owner implements it yet (the BSC-V8 checklist names the missing one). |



<a name="darkbot-bsc-v1-DataQuality"></a>

### DataQuality
How far the values of a row or projection are proven by native evidence.

| Name | Number | Description |
| ---- | ------ | ----------- |
| DATA_QUALITY_UNSPECIFIED | 0 | Not set by any producer; never displayed as a value. |
| DATA_QUALITY_VERIFIED | 1 | Every value comes from identified native evidence. |
| DATA_QUALITY_INCOMPLETE | 2 | Some values are missing; each missing field stays absent, never zero. |
| DATA_QUALITY_UNRESOLVED | 3 | The chain has not decided yet (UNKNOWN handoff, receipt window, reorg depth). |
| DATA_QUALITY_STALE | 4 | Older than the owner&#39;s freshness rule. |
| DATA_QUALITY_CONFLICT | 5 | Independent sources disagree (e.g. the second chain source DISAGREES). |



<a name="darkbot-bsc-v1-PerformanceMetric"></a>

### PerformanceMetric
The quantity of a performance series.

| Name | Number | Description |
| ---- | ------ | ----------- |
| PERFORMANCE_METRIC_UNSPECIFIED | 0 | Not set by any producer; never displayed as a value. |
| PERFORMANCE_METRIC_REALIZED_NET_PNL | 1 | Cumulative realized net PnL of closed trades, signed decimal wei (no transfers). |
| PERFORMANCE_METRIC_WALLET_BALANCE | 2 | A wallet&#39;s balance including transfers, decimal wei. |
| PERFORMANCE_METRIC_WALLET_TRADING_PNL | 3 | A wallet&#39;s trading PnL excluding transfers, signed decimal wei. |
| PERFORMANCE_METRIC_TRAINING_NET_RESULT | 4 | A training run&#39;s cumulative PAPER net result, signed decimal wei (no real funds). |



<a name="darkbot-bsc-v1-PerformanceScope"></a>

### PerformanceScope
What a performance series is about.

| Name | Number | Description |
| ---- | ------ | ----------- |
| PERFORMANCE_SCOPE_UNSPECIFIED | 0 | Not set by any producer; never displayed as a value. |
| PERFORMANCE_SCOPE_LIVE | 1 | The LIVE ledger&#39;s trades. |
| PERFORMANCE_SCOPE_WALLET | 2 | One BSC wallet (scope_id = its WalletRole name, e.g. WALLET_ROLE_MAIN). |
| PERFORMANCE_SCOPE_ALL_WALLETS | 3 | Every BSC wallet together. |
| PERFORMANCE_SCOPE_TRAINING_RUN | 4 | One training run (scope_id = its run_id). |
| PERFORMANCE_SCOPE_TRAINING_PROFILE | 5 | One training profile across its runs (scope_id = its profile_id). |



<a name="darkbot-bsc-v1-TimeSource"></a>

### TimeSource
The clock a time was taken on. Times of different clocks are never subtracted.

| Name | Number | Description |
| ---- | ------ | ----------- |
| TIME_SOURCE_UNSPECIFIED | 0 | Not set by any producer; never displayed as a value. |
| TIME_SOURCE_BLOCK_HEADER | 1 | The block header&#39;s own timestamp (BSC block time, ms); not the producer&#39;s T0. |
| TIME_SOURCE_LOCAL_WALL | 2 | The host wall clock at a local event (sighting, send, answer). |
| TIME_SOURCE_RUNTIME_MONOTONIC | 3 | The runtime&#39;s monotonic clock (clock_id of the release record); only differences on one clock are meaningful. |
| TIME_SOURCE_PRODUCER_WITNESS | 4 | An exact producer T0 from an original execution witness (t0_exact). |










<a name="darkbot_bsc_v1_training-proto"></a>
<p align="right"><a href="#top">Top</a></p>

## darkbot/bsc/v1/training.proto



<a name="darkbot-bsc-v1-LiveStrategySelectCommand"></a>

### LiveStrategySelectCommand
Params of `bsc.live.strategy.select` (prepared, FE-V8-09): LIVE takes one profile revision as ONE
atomic Strategy C configuration, or the custom settings; never a local draft.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| profile_id | [string](#string) |  | The profile; empty: the custom settings (StrategyCSettingsSetAllCommand) stay in effect. |
| profile_revision | [uint64](#uint64) |  | The revision bound (it must be the profile&#39;s current one). |
| config_sha256 | [string](#string) |  | The configuration hash of that revision the operator saw. |
| expected_settings_generation | [uint64](#uint64) |  | The stored settings generation the selection replaces (StrategyCSettingsState.stored_revision): the compare-and-swap of the ONE generation it writes. |






<a name="darkbot-bsc-v1-ReplaySource"></a>

### ReplaySource
One recorded run or journal the owner can replay, named by its catalogue id only (never a path).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| source_id | [string](#string) |  | The catalogue id a REPLAY start names. |
| kind | [string](#string) |  | RECORDED_RUN or MARKET_JOURNAL. |
| run_id | [string](#string) |  | The recorded training run, when it is one; empty otherwise. |
| from_block | [uint64](#uint64) | optional | Its first block; absent when unknown. |
| to_block | [uint64](#uint64) | optional | Its last block; absent when unknown. |
| from | [SourcedTime](#darkbot-bsc-v1-SourcedTime) |  | Its first record&#39;s time. |
| to | [SourcedTime](#darkbot-bsc-v1-SourcedTime) |  | Its last record&#39;s time. |
| records | [uint64](#uint64) |  | Records in it. |
| config_sha256 | [string](#string) |  | The configuration hash it was recorded with; empty for a market journal. |
| quality | [DataQuality](#darkbot-bsc-v1-DataQuality) |  | How complete it is (INCOMPLETE with gaps). |
| quality_reasons | [string](#string) | repeated | Why the quality is not VERIFIED. |






<a name="darkbot-bsc-v1-ReplaySourcePage"></a>

### ReplaySourcePage
One page of the replay catalogue, newest first, bound to one owner snapshot.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| availability | [Availability](#darkbot-bsc-v1-Availability) |  | Whether the native owner answered. |
| unavailable_reason | [string](#string) |  | Why it is not AVAILABLE. |
| provenance | [OwnerProvenance](#darkbot-bsc-v1-OwnerProvenance) |  | The owner and the snapshot the cursor is bound to. |
| sources | [ReplaySource](#darkbot-bsc-v1-ReplaySource) | repeated | The sources. |
| next_cursor | [string](#string) |  | The cursor of the next page; empty on the last one. |
| has_more | [bool](#bool) |  | Whether another page follows. |






<a name="darkbot-bsc-v1-ReplaySourceQuery"></a>

### ReplaySourceQuery
A bounded read of the owner&#39;s replay catalogue.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| cursor | [string](#string) |  | The previous page&#39;s next_cursor; empty: the first page. |
| limit | [uint32](#uint32) |  | At most this many sources; 0: the owner&#39;s default; above its cap: refused (LIMIT_EXCEEDED). |






<a name="darkbot-bsc-v1-StrategyCSettableValues"></a>

### StrategyCSettableValues
The settable Strategy C values of a command (set_all, profile create/update): the ten native keys.
A set_all carries the values it changes: an absent value (an enum UNSPECIFIED) stays as stored,
an empty scalar is refused. Its filters are always the complete form: every shown feature with
its enabled flag (the disabled rules are dropped natively, the enabled ones stored in the order
sent); an empty list is refused (InvalidParams), so no rules = every feature with enabled false.
A profile carries every value (an enum not UNSPECIFIED). The owner refuses a value outside its
StrategySettingSpec and stores nothing half.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| input_wei | [string](#string) | optional | BSC_STRATEGY_C_INPUT_WEI: the BUY input per entry, decimal wei. |
| target_net_multiple | [string](#string) | optional | BSC_STRATEGY_C_TARGET_NET_MULTIPLE: exact decimal at the ratio places, e.g. &#34;1.50&#34;. |
| stop_net_multiple | [string](#string) | optional | BSC_STRATEGY_C_STOP_NET_MULTIPLE: exact decimal at the ratio places. |
| observation_ms | [uint64](#uint64) | optional | BSC_STRATEGY_C_OBSERVATION_MS, milliseconds. |
| hold_ms | [uint64](#uint64) | optional | BSC_STRATEGY_C_HOLD_MS, milliseconds. |
| max_open | [uint32](#uint32) | optional | BSC_STRATEGY_C_MAX_OPEN, per venue. |
| creator_sold_before_fill | [CreatorSoldBeforeFill](#darkbot-bsc-v1-CreatorSoldBeforeFill) |  | BSC_STRATEGY_C_CREATOR_SOLD_BEFORE_FILL. |
| creator_sell_exit | [CreatorSellExit](#darkbot-bsc-v1-CreatorSellExit) |  | BSC_STRATEGY_C_CREATOR_SELL_EXIT. |
| filters | [StrategyFilterRule](#darkbot-bsc-v1-StrategyFilterRule) | repeated | BSC_STRATEGY_C_FILTERS: the rules in the native rule order. |
| first_buyer_window | [uint32](#uint32) | optional | BSC_STRATEGY_C_FIRST_BUYER_WINDOW: the buyers counted as early. |






<a name="darkbot-bsc-v1-StrategyCSettings"></a>

### StrategyCSettings
A Strategy C configuration as the owner reports it (state only, never a command&#39;s params): the
settable values 1–10 and the configuration-owned, read-only values 11–13. Each value is optional:
absent means the configuration file&#39;s value (the native owner&#39;s rule).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| input_wei | [string](#string) | optional | BSC_STRATEGY_C_INPUT_WEI: the BUY input per entry, decimal wei. |
| target_net_multiple | [string](#string) | optional | BSC_STRATEGY_C_TARGET_NET_MULTIPLE: exit at this net multiple of the input (after every cost), exact decimal at ratio_places, e.g. &#34;1.50&#34;. |
| stop_net_multiple | [string](#string) | optional | BSC_STRATEGY_C_STOP_NET_MULTIPLE: exit at this net multiple, exact decimal at ratio_places. |
| observation_ms | [uint64](#uint64) | optional | BSC_STRATEGY_C_OBSERVATION_MS: the observation window before an entry, milliseconds. |
| hold_ms | [uint64](#uint64) | optional | BSC_STRATEGY_C_HOLD_MS: the longest hold before the timeout exit, milliseconds. |
| max_open | [uint32](#uint32) | optional | BSC_STRATEGY_C_MAX_OPEN: the most positions open at once, per venue. |
| creator_sold_before_fill | [CreatorSoldBeforeFill](#darkbot-bsc-v1-CreatorSoldBeforeFill) |  | BSC_STRATEGY_C_CREATOR_SOLD_BEFORE_FILL. |
| creator_sell_exit | [CreatorSellExit](#darkbot-bsc-v1-CreatorSellExit) |  | BSC_STRATEGY_C_CREATOR_SELL_EXIT. |
| filters | [StrategyFilterRule](#darkbot-bsc-v1-StrategyFilterRule) | repeated | BSC_STRATEGY_C_FILTERS: the rules in the native rule order. |
| first_buyer_window | [uint32](#uint32) | optional | BSC_STRATEGY_C_FIRST_BUYER_WINDOW: the buyers counted as early, within its StrategySettingSpec. |
| ratio_places | [uint32](#uint32) | optional | BSC_STRATEGY_C_RATIO_PLACES: decimal places of ratios and multiples (configuration, read-only). |
| fill_delay_blocks | [uint32](#uint32) | optional | BSC_STRATEGY_C_FILL_DELAY_BLOCKS (configuration, read-only). |
| gas_price_wei | [string](#string) | optional | BSC_STRATEGY_C_GAS_PRICE_WEI (configuration, read-only), decimal wei. |






<a name="darkbot-bsc-v1-StrategyCSettingsQuery"></a>

### StrategyCSettingsQuery
A read of the stored and running Strategy C configuration.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| include_features | [bool](#bool) |  | Also the filterable feature catalogue (StrategyFeatureSpec). |






<a name="darkbot-bsc-v1-StrategyCSettingsSetAllCommand"></a>

### StrategyCSettingsSetAllCommand
Params of `bsc.strategy.set_all` (BSC-V8-04): the form&#39;s changes as ONE validated revision; a
refused field refuses the whole set, nothing is stored half. Its receipt detail line is
`outcome=&lt;…&gt; keys=&lt;n&gt; generation=&lt;g&gt; request_id=&lt;…&gt;[ reason=&lt;…&gt;]`.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| values | [StrategyCSettableValues](#darkbot-bsc-v1-StrategyCSettableValues) |  | Every settable value. |






<a name="darkbot-bsc-v1-StrategyCSettingsState"></a>

### StrategyCSettingsState
The stored and the running Strategy C configuration of the native owner.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| availability | [Availability](#darkbot-bsc-v1-Availability) |  | Whether the native owner answered. |
| unavailable_reason | [string](#string) |  | Why it is not AVAILABLE. |
| provenance | [OwnerProvenance](#darkbot-bsc-v1-OwnerProvenance) |  | The owner and its settings generation. |
| stored | [StrategyCSettings](#darkbot-bsc-v1-StrategyCSettings) |  | The stored settable values (absent keys: the configuration file&#39;s). |
| stored_revision | [uint64](#uint64) |  | The stored revision (settings generation): the compare-and-swap of the next save. |
| stored_config_sha256 | [string](#string) |  | The configuration hash of the stored revision&#39;s effective values. |
| running | [StrategyCSettings](#darkbot-bsc-v1-StrategyCSettings) |  | What the running strategy took at its start, from a current runtime status only; absent while it does not run or its status is stale. |
| running_revision | [uint64](#uint64) | optional | The revision the running strategy took; absent while it does not run. |
| running_config_sha256 | [string](#string) |  | The configuration hash of what the running strategy took; empty while it does not run. |
| apply_status | [SettingsApplyStatus](#darkbot-bsc-v1-SettingsApplyStatus) |  | Whether the stored revision is in effect. |
| refusal_reason | [string](#string) |  | The strategy&#39;s refusal of its settings, `&lt;SETTINGS_REFUSED|GAS_EVIDENCE_REFUSED&gt;: &lt;reason&gt;`; empty without one. |
| request_id | [string](#string) |  | The request that wrote the stored revision; empty while none. |
| features | [StrategyFeatureSpec](#darkbot-bsc-v1-StrategyFeatureSpec) | repeated | The filterable features with their unit and places, in native order. |
| effective | [StrategyCSettings](#darkbot-bsc-v1-StrategyCSettings) |  | The values in effect for the next start: every stored key over the configuration file&#39;s, the read-only configuration values included. |
| setting_specs | [StrategySettingSpec](#darkbot-bsc-v1-StrategySettingSpec) | repeated | The bounds of every settable key, in the native KEYS order. |






<a name="darkbot-bsc-v1-StrategyFeatureSpec"></a>

### StrategyFeatureSpec
One filterable native feature as the owner defines it (no frontend list of units or places).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| name | [string](#string) |  | The native feature name. |
| index | [uint32](#uint32) |  | Its position in features.rs NAMES. |
| unit | [string](#string) |  | Native unit: count, bnb (decimal BNB at 18 places), ratio, percent, milliseconds or flag (provider_gap_flag, never filterable). |
| places | [uint32](#uint32) |  | Decimal places a bound may have (BNB amounts 18, ratios BSC_STRATEGY_C_RATIO_PLACES). |
| signed | [bool](#bool) |  | Whether a bound may be negative (signed features such as net_buy_flow_bnb, price_change). |
| filterable | [bool](#bool) |  | False for provider_gap_flag: a data-quality flag, never a filter. |






<a name="darkbot-bsc-v1-StrategyFilterRule"></a>

### StrategyFilterRule
One entry filter `feature:min:max` of BSC_STRATEGY_C_FILTERS. A disabled rule is not part of the
serialized native string; an enabled rule without bounds still fails on a NULL feature. Rules keep
the native rule order of the stored string (the owner&#39;s, never re-sorted by a client).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| feature | [string](#string) |  | The native feature name (features.rs NAMES[0..15]); never provider_gap_flag. |
| enabled | [bool](#bool) |  | Whether the rule is part of the configuration. |
| min | [string](#string) | optional | Lower bound, exact decimal at the feature&#39;s places; absent: unbounded. |
| max | [string](#string) | optional | Upper bound, exact decimal at the feature&#39;s places; absent: unbounded. |






<a name="darkbot-bsc-v1-StrategySettingSpec"></a>

### StrategySettingSpec
One settable key&#39;s bounds as the native parser enforces them (no client literal of a bound).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| key | [string](#string) |  | The native key, e.g. BSC_STRATEGY_C_HOLD_MS. |
| unit | [string](#string) |  | Its unit: wei, ratio, ms, count, enum or rules (BSC_STRATEGY_C_FILTERS). |
| min | [string](#string) | optional | The smallest accepted value, exact decimal; absent: unbounded below. |
| max | [string](#string) | optional | The largest accepted value, exact decimal; absent: unbounded above. |
| places | [uint32](#uint32) |  | Decimal places a value may have. |






<a name="darkbot-bsc-v1-TrainingProfile"></a>

### TrainingProfile
A named, versioned Strategy C configuration of the native owner (one strategy, many profiles).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| profile_id | [string](#string) |  | Stable owner identity, never reused. |
| name | [string](#string) |  | The operator&#39;s name. |
| revision | [uint64](#uint64) |  | The profile revision: the compare-and-swap of an update, delete or run start. |
| config_sha256 | [string](#string) |  | The configuration hash of this revision&#39;s effective values. |
| venues | [string](#string) | repeated | The venues it trains on (BSC_VENUE_FLAP, BSC_VENUE_FOUR_CLASSIC, BSC_VENUE_OPENFOUR), as the owner offers them; never an implicit &#34;all markets&#34;. |
| created_unix_ms | [uint64](#uint64) |  | Creation time, unix ms. |
| updated_unix_ms | [uint64](#uint64) |  | Time of this revision, unix ms. |
| status | [TrainingProfileStatus](#darkbot-bsc-v1-TrainingProfileStatus) |  | Its lifecycle. |
| run_ids | [string](#string) | repeated | The runs that reference it, newest first (bounded; the run query has all). |
| in_use | [bool](#bool) |  | Whether a running run or the LIVE selection uses it (edits then need a clone). |
| in_use_reason | [string](#string) |  | What uses it; empty when not in use. |
| values | [StrategyCSettableValues](#darkbot-bsc-v1-StrategyCSettableValues) |  | Every settable value of this revision. |
| request_id | [string](#string) |  | The request that wrote this revision. |






<a name="darkbot-bsc-v1-TrainingProfileCloneCommand"></a>

### TrainingProfileCloneCommand
Params of `bsc.training.profile.clone` (prepared): a new profile from one revision.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| source_profile_id | [string](#string) |  | The profile copied. |
| source_revision | [uint64](#uint64) |  | The revision copied. |
| name | [string](#string) |  | The new profile&#39;s name. |






<a name="darkbot-bsc-v1-TrainingProfileCreateCommand"></a>

### TrainingProfileCreateCommand
Params of `bsc.training.profile.create` (prepared).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| name | [string](#string) |  | The operator&#39;s name. |
| venues | [string](#string) | repeated | The venues it trains on. |
| values | [StrategyCSettableValues](#darkbot-bsc-v1-StrategyCSettableValues) |  | Every settable value. |






<a name="darkbot-bsc-v1-TrainingProfileDeleteCommand"></a>

### TrainingProfileDeleteCommand
Params of `bsc.training.profile.delete` (prepared): refused while a run or LIVE uses it.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| profile_id | [string](#string) |  | The profile. |
| archive | [bool](#bool) |  | Archive instead of delete (runs and results stay readable either way). |






<a name="darkbot-bsc-v1-TrainingProfileUpdateCommand"></a>

### TrainingProfileUpdateCommand
Params of `bsc.training.profile.update` (prepared): a new revision of one profile.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| profile_id | [string](#string) |  | The profile (equal to CommandRequest.resource_id). |
| name | [string](#string) |  | The operator&#39;s name. |
| venues | [string](#string) | repeated | The venues it trains on. |
| values | [StrategyCSettableValues](#darkbot-bsc-v1-StrategyCSettableValues) |  | Every settable value. |






<a name="darkbot-bsc-v1-TrainingProfiles"></a>

### TrainingProfiles
The training profiles of the native owner.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| availability | [Availability](#darkbot-bsc-v1-Availability) |  | Whether the native owner answered. |
| unavailable_reason | [string](#string) |  | Why it is not AVAILABLE. |
| provenance | [OwnerProvenance](#darkbot-bsc-v1-OwnerProvenance) |  | The owner and its catalog version. |
| profiles | [TrainingProfile](#darkbot-bsc-v1-TrainingProfile) | repeated | Every non-deleted profile. |
| catalog_version | [uint64](#uint64) |  | The catalog version: the compare-and-swap of a create or clone. |
| live_profile_id | [string](#string) |  | The profile the LIVE selection names; empty without one. |
| live_profile_revision | [uint64](#uint64) | optional | The revision the LIVE selection is bound to. |






<a name="darkbot-bsc-v1-TrainingProfilesQuery"></a>

### TrainingProfilesQuery
A bounded read of the training profiles.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| include_archived | [bool](#bool) |  | Also archived profiles. |






<a name="darkbot-bsc-v1-TrainingResult"></a>

### TrainingResult
The results of one training run, from its native journal; counts are the journal&#39;s, its quality says
how complete; net_wei stays absent while unresolved, never zero.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| evaluated | [uint64](#uint64) |  | Launches the strategy evaluated. |
| passed | [uint64](#uint64) |  | Of them passed every filter. |
| rejected | [uint64](#uint64) |  | Of them rejected by a filter. |
| data_incomplete | [uint64](#uint64) |  | Of them DATA_INCOMPLETE (a coverage gap, never a pass). |
| not_tracked | [uint64](#uint64) |  | Launches not tracked (the venue or curve not exact). |
| unresolved | [uint64](#uint64) |  | Positions whose value is unresolved (the curve stopped being exact). |
| entered | [uint64](#uint64) |  | PAPER entries. |
| exits_target | [uint64](#uint64) |  | Exits at the target. |
| exits_stop | [uint64](#uint64) |  | Exits at the stop. |
| exits_creator_sell | [uint64](#uint64) |  | Exits by a creator sale. |
| exits_timeout | [uint64](#uint64) |  | Exits at the hold timeout. |
| wins | [uint64](#uint64) |  | Closed positions with a gain. |
| losses | [uint64](#uint64) |  | Closed positions with a loss. |
| gain_wei | [string](#string) |  | Sum of the gains, decimal wei. |
| loss_wei | [string](#string) |  | Sum of the losses, decimal wei (a positive amount). |
| net_wei | [string](#string) | optional | Net result after every cost, signed decimal wei; absent while unresolved positions exist. |
| open_positions | [uint64](#uint64) |  | Positions still open. |
| sources | [string](#string) | repeated | The ingress sources the run observed. |
| observable_ms | [uint64](#uint64) | optional | The observed market time with complete coverage, ms. |
| lost_records | [uint64](#uint64) |  | Records lost by the capture. |
| ingress_dropped | [uint64](#uint64) |  | Ingress messages dropped. |
| quality | [DataQuality](#darkbot-bsc-v1-DataQuality) |  | How far these counts are proven (INCOMPLETE while the run&#39;s journal is not sealed or has gaps); a result the owner cannot read at all is the whole message absent. |
| quality_reasons | [string](#string) | repeated | Why the quality is not VERIFIED. |
| slot_limit | [uint64](#uint64) |  | Entries refused because max_open slots were full. |
| creator_sold | [uint64](#uint64) |  | Launches whose creator sold before the BUY filled. |
| cancelled | [uint64](#uint64) |  | Entries cancelled before execution (the four counts below together). |
| cancelled_creator_sold | [uint64](#uint64) |  | Of them by a creator sale (FREE_CANCEL). |
| cancelled_not_tracked | [uint64](#uint64) |  | Of them because the launch is not tracked. |
| cancelled_quote | [uint64](#uint64) |  | Of them by a quote error. |
| cancelled_cost | [uint64](#uint64) |  | Of them by a cost error. |
| superseded | [uint64](#uint64) |  | Entries superseded by a later decision on the same token. |
| paper_unpriced | [uint64](#uint64) |  | PAPER positions without an exact price (no value, never zero). |
| open_at_end | [uint64](#uint64) |  | Positions still open when the run ended. |
| replay_verdict | [ReplayVerdict](#darkbot-bsc-v1-ReplayVerdict) |  | REPLAY: the comparison with the recorded run; NOT_RUN otherwise. |
| config_match | [bool](#bool) | optional | REPLAY: whether the replayed configuration hash equals the recorded run&#39;s; absent otherwise. |






<a name="darkbot-bsc-v1-TrainingRun"></a>

### TrainingRun
One training run of a profile revision.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| run_id | [string](#string) |  | Stable run identity, never reused. |
| profile_id | [string](#string) |  | The profile it runs. |
| profile_revision | [uint64](#uint64) |  | The profile revision it runs. |
| config_sha256 | [string](#string) |  | The configuration hash it runs. |
| mode | [TrainingRunMode](#darkbot-bsc-v1-TrainingRunMode) |  | How it executes. |
| venues | [string](#string) | repeated | The venues it trains on. |
| plan | [TrainingRunPlan](#darkbot-bsc-v1-TrainingRunPlan) |  | Its bound. |
| started_unix_ms | [uint64](#uint64) | optional | Start, unix ms; absent before it started. |
| ended_unix_ms | [uint64](#uint64) | optional | End, unix ms; absent while running. |
| status | [TrainingRunStatus](#darkbot-bsc-v1-TrainingRunStatus) |  | Its lifecycle. |
| stop_reason | [string](#string) |  | Why it ended (PLAN_DURATION, PLAN_LAUNCH_COUNT, STOP_REQUESTED, a failure); empty while running. |
| launches_processed | [uint64](#uint64) |  | Qualified launches processed so far. |
| result | [TrainingResult](#darkbot-bsc-v1-TrainingResult) |  | Its results so far. |
| provenance | [OwnerProvenance](#darkbot-bsc-v1-OwnerProvenance) |  | The owner and journal the run was read from. |
| request_id | [string](#string) |  | The request that started it. |
| replay_source | [string](#string) |  | REPLAY: the ReplaySource.source_id it replays (echo of the start); empty otherwise. |






<a name="darkbot-bsc-v1-TrainingRunPage"></a>

### TrainingRunPage
One page of training runs, newest first, bound to one owner snapshot.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| availability | [Availability](#darkbot-bsc-v1-Availability) |  | Whether the native owner answered. |
| unavailable_reason | [string](#string) |  | Why it is not AVAILABLE. |
| provenance | [OwnerProvenance](#darkbot-bsc-v1-OwnerProvenance) |  | The owner and the snapshot the cursor is bound to. |
| runs | [TrainingRun](#darkbot-bsc-v1-TrainingRun) | repeated | The runs. |
| next_cursor | [string](#string) |  | The cursor of the next page; empty on the last one. |
| has_more | [bool](#bool) |  | Whether another page follows. |






<a name="darkbot-bsc-v1-TrainingRunPlan"></a>

### TrainingRunPlan
The bound of a training run: exactly the field its stop_kind names is set, the owner refuses any
other combination.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| stop_kind | [TrainingRunStopKind](#darkbot-bsc-v1-TrainingRunStopKind) |  | What ends it. |
| duration_ms | [uint64](#uint64) | optional | DURATION only: the run&#39;s length, milliseconds. |
| max_launches | [uint64](#uint64) | optional | LAUNCH_COUNT only: the qualified launches (the engine&#39;s Decided outputs) it processes. |






<a name="darkbot-bsc-v1-TrainingRunQuery"></a>

### TrainingRunQuery
A bounded, cursor-paged read of training runs.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| profile_id | [string](#string) |  | Only runs of this profile; empty: every profile. |
| statuses | [TrainingRunStatus](#darkbot-bsc-v1-TrainingRunStatus) | repeated | Only runs in these states; empty: every state. |
| cursor | [string](#string) |  | The previous page&#39;s next_cursor; empty: the first page. |
| limit | [uint32](#uint32) |  | At most this many runs; 0: the owner&#39;s default; above its cap: refused (LIMIT_EXCEEDED). |






<a name="darkbot-bsc-v1-TrainingRunStartCommand"></a>

### TrainingRunStartCommand
Params of `bsc.training.run.start` (prepared).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| profile_id | [string](#string) |  | The profile. |
| mode | [TrainingRunMode](#darkbot-bsc-v1-TrainingRunMode) |  | How it executes. |
| venues | [string](#string) | repeated | The venues it trains on (a subset of the profile&#39;s). |
| plan | [TrainingRunPlan](#darkbot-bsc-v1-TrainingRunPlan) |  | Its bound. |
| replay_source | [string](#string) |  | REPLAY: a ReplaySource.source_id of the owner&#39;s catalogue (never a path); empty for PAPER and SHADOW. |






<a name="darkbot-bsc-v1-TrainingRunStopCommand"></a>

### TrainingRunStopCommand
Params of `bsc.training.run.stop` (prepared); CommandRequest.resource_id names the run.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| reason | [string](#string) |  | Why the operator stops it (free text, diagnostic only). |








<a name="darkbot-bsc-v1-CreatorSellExit"></a>

### CreatorSellExit
The creator-sell exit (BSC_STRATEGY_C_CREATOR_SELL_EXIT). A team-sell exit is NOT_IMPLEMENTED
(BSC-V8-06) and has no field until a native wallet attribution and sell signal exist.

| Name | Number | Description |
| ---- | ------ | ----------- |
| CREATOR_SELL_EXIT_UNSPECIFIED | 0 | Not set: in a state the configuration file&#39;s value; in a set_all the stored value stays. |
| CREATOR_SELL_EXIT_ENABLED | 1 | ENABLED: a sale by the token&#39;s creator ends the position. |
| CREATOR_SELL_EXIT_DISABLED | 2 | DISABLED: a creator sale does not end the position. |



<a name="darkbot-bsc-v1-CreatorSoldBeforeFill"></a>

### CreatorSoldBeforeFill
The creator selling before the BUY fills (BSC_STRATEGY_C_CREATOR_SOLD_BEFORE_FILL).

| Name | Number | Description |
| ---- | ------ | ----------- |
| CREATOR_SOLD_BEFORE_FILL_UNSPECIFIED | 0 | Not set: in a state the configuration file&#39;s value; in a set_all the stored value stays. |
| CREATOR_SOLD_BEFORE_FILL_FREE_CANCEL | 1 | FREE_CANCEL: the entry is cancelled before execution. |
| CREATOR_SOLD_BEFORE_FILL_TX_LANDS | 2 | TX_LANDS: the BUY transaction may still land; never a guaranteed fill. |



<a name="darkbot-bsc-v1-ReplayVerdict"></a>

### ReplayVerdict
A replay&#39;s comparison with the run it replays.

| Name | Number | Description |
| ---- | ------ | ----------- |
| REPLAY_VERDICT_UNSPECIFIED | 0 | Not set by any producer; never displayed as a value. |
| REPLAY_VERDICT_PASS | 1 | The replay&#39;s decisions and results equal the recorded ones. |
| REPLAY_VERDICT_DIFFERS | 2 | They differ (the first difference in quality_reasons). |
| REPLAY_VERDICT_INCOMPLETE | 3 | The recorded journal has gaps; no verdict. |
| REPLAY_VERDICT_NOT_RUN | 4 | Not a replay, or not compared. |



<a name="darkbot-bsc-v1-SettingsApplyStatus"></a>

### SettingsApplyStatus
Whether a stored Strategy C configuration is in effect.

| Name | Number | Description |
| ---- | ------ | ----------- |
| SETTINGS_APPLY_STATUS_UNSPECIFIED | 0 | The running state is unknown: the runtime status does not read (provenance quality INCOMPLETE, quality_reasons RUNTIME_STATUS_INVALID); never displayed as a value. |
| SETTINGS_APPLY_STATUS_IN_EFFECT | 1 | The running strategy took the stored revision at its start (its generation and its configuration hash equal the stored ones). |
| SETTINGS_APPLY_STATUS_SAVED_APPLIES_NEXT_START | 2 | Stored; the running strategy keeps the revision of its start until its next start. |
| SETTINGS_APPLY_STATUS_NOT_RUNNING | 3 | Strategy C does not run, or its runtime status is stale (quality_reasons RUNTIME_STATUS_STALE): the stored revision applies at its next start. |
| SETTINGS_APPLY_STATUS_REFUSED | 4 | The strategy refused its settings at its start (SETTINGS_REFUSED, GAS_EVIDENCE_REFUSED). |



<a name="darkbot-bsc-v1-TrainingProfileStatus"></a>

### TrainingProfileStatus
The lifecycle of a training profile.

| Name | Number | Description |
| ---- | ------ | ----------- |
| TRAINING_PROFILE_STATUS_UNSPECIFIED | 0 | Not set by any producer; never displayed as a value. |
| TRAINING_PROFILE_STATUS_ACTIVE | 1 | Usable for runs and the LIVE selection. |
| TRAINING_PROFILE_STATUS_ARCHIVED | 2 | Kept with its runs and results, not selectable. |
| TRAINING_PROFILE_STATUS_DELETED | 3 | Deleted; kept only while a run or result references it. |



<a name="darkbot-bsc-v1-TrainingRunMode"></a>

### TrainingRunMode
How a training run executes. Never with real funds or the LIVE wallet, nonce or budget.

| Name | Number | Description |
| ---- | ------ | ----------- |
| TRAINING_RUN_MODE_UNSPECIFIED | 0 | Not set by any producer; never displayed as a value. |
| TRAINING_RUN_MODE_PAPER | 1 | PAPER: the BSC_RUNTIME_MODE PAPER path on live market data to the PAPER sink, nothing sent. |
| TRAINING_RUN_MODE_SHADOW | 2 | SHADOW: the engine&#39;s valuation only (decisions and exits valued by the exact curve), no sink. |
| TRAINING_RUN_MODE_REPLAY | 3 | Offline replay of recorded journals (strategy-replay). |



<a name="darkbot-bsc-v1-TrainingRunStatus"></a>

### TrainingRunStatus
The lifecycle of a training run.

| Name | Number | Description |
| ---- | ------ | ----------- |
| TRAINING_RUN_STATUS_UNSPECIFIED | 0 | Not set by any producer; never displayed as a value. |
| TRAINING_RUN_STATUS_PENDING | 1 | Accepted, not started. |
| TRAINING_RUN_STATUS_RUNNING | 2 | Running. |
| TRAINING_RUN_STATUS_STOPPING | 3 | A stop was requested; the run seals its results. |
| TRAINING_RUN_STATUS_COMPLETED | 4 | Ended by its plan (duration or launch count). |
| TRAINING_RUN_STATUS_STOPPED | 5 | Ended by a stop request. |
| TRAINING_RUN_STATUS_FAILED | 6 | Ended by a failure (stop_reason). |
| TRAINING_RUN_STATUS_UNKNOWN | 7 | The owner cannot tell (state unreadable, crash before seal). |
| TRAINING_RUN_STATUS_INTERRUPTED | 8 | The runtime ended or restarted under it: sealed up to its last journal record, never resumed. |



<a name="darkbot-bsc-v1-TrainingRunStopKind"></a>

### TrainingRunStopKind
What ends a training run.

| Name | Number | Description |
| ---- | ------ | ----------- |
| TRAINING_RUN_STOP_KIND_UNSPECIFIED | 0 | Not set by any producer; never displayed as a value. |
| TRAINING_RUN_STOP_KIND_DURATION | 1 | After duration_ms. |
| TRAINING_RUN_STOP_KIND_LAUNCH_COUNT | 2 | After max_launches qualified launches processed. |
| TRAINING_RUN_STOP_KIND_UNTIL_STOPPED | 3 | Only by a stop request. |










<a name="darkbot_bsc_v1_history-proto"></a>
<p align="right"><a href="#top">Top</a></p>

## darkbot/bsc/v1/history.proto



<a name="darkbot-bsc-v1-TradeCounts"></a>

### TradeCounts
The counts of the whole filtered result (V8 trade summary).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| pending | [uint64](#uint64) |  | PENDING and UNKNOWN (PEND). |
| running | [uint64](#uint64) |  | OPEN and EXITING (RUN). |
| wins | [uint64](#uint64) |  | CLOSED with a gain (WIN). |
| losses | [uint64](#uint64) |  | CLOSED with a loss (LOSS). |
| flat | [uint64](#uint64) |  | CLOSED without gain or loss. |
| failed | [uint64](#uint64) |  | FAILED. |
| written_off | [uint64](#uint64) |  | WRITTEN_OFF. |






<a name="darkbot-bsc-v1-TradeDetail"></a>

### TradeDetail
The full detail of one lot.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| availability | [Availability](#darkbot-bsc-v1-Availability) |  | Whether the native owner answered. |
| unavailable_reason | [string](#string) |  | Why it is not AVAILABLE. |
| provenance | [OwnerProvenance](#darkbot-bsc-v1-OwnerProvenance) |  | The owner and the ledger snapshot. |
| row | [TradeRow](#darkbot-bsc-v1-TradeRow) |  | The row. |
| transactions | [TradeTransaction](#darkbot-bsc-v1-TradeTransaction) | repeated | Every transaction of the lot in nonce order. |
| sightings | [TradeSighting](#darkbot-bsc-v1-TradeSighting) | repeated | The launch&#39;s sightings per source (the first verified full one flagged); empty for a lot without an attributed launch. |
| grant_generation | [uint64](#uint64) | optional | The grant generation its BUY was admitted under. |
| decisions | [string](#string) | repeated | The operator decisions the ledger takes of this lot now (RESUME, WRITE_OFF, EXIT_NOW). |
| attempt | [LiveAttempt](#darkbot-bsc-v1-LiveAttempt) |  | The BUY&#39;s evaluated attempt record unchanged (launch, clock, T0, timings, placement, tokens); absent without one. |
| exit_terms | [TradeExitTerms](#darkbot-bsc-v1-TradeExitTerms) |  | Strategy C: the exit terms its entry took; absent for C3. |
| settings_generation | [uint64](#uint64) | optional | Strategy C: the settings generation its entry took; absent for C3. |






<a name="darkbot-bsc-v1-TradeDetailQuery"></a>

### TradeDetailQuery
A read of one lot.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| lot | [string](#string) |  | The lot, 0x hex. |






<a name="darkbot-bsc-v1-TradeExitTerms"></a>

### TradeExitTerms
What a Strategy C entry took for its exit (from its settings generation).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| ratio_places | [uint32](#uint32) |  | BSC_STRATEGY_C_RATIO_PLACES of the multiples. |
| target_net_multiple | [string](#string) |  | The net target multiple, exact decimal at ratio_places. |
| stop_net_multiple | [string](#string) |  | The net stop multiple, exact decimal at ratio_places. |
| hold_ms | [uint64](#uint64) |  | The hold timeout, milliseconds. |
| creator_sell_exit | [CreatorSellExit](#darkbot-bsc-v1-CreatorSellExit) |  | The creator-sell exit. |






<a name="darkbot-bsc-v1-TradeHistoryPage"></a>

### TradeHistoryPage
One page of trades, bound to one composite snapshot (ledger version, Strategy C journal and attempt
record positions), the filter digest and the sort: no gap, no duplicate across pages; a cursor
of another snapshot, filter or sort is refused (QueryRefusal).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| availability | [Availability](#darkbot-bsc-v1-Availability) |  | Whether the native owner answered. |
| unavailable_reason | [string](#string) |  | Why it is not AVAILABLE. |
| provenance | [OwnerProvenance](#darkbot-bsc-v1-OwnerProvenance) |  | The owner and the ledger version the cursor is bound to. |
| rows | [TradeRow](#darkbot-bsc-v1-TradeRow) | repeated | The rows. |
| next_cursor | [string](#string) |  | The cursor of the next page; empty on the last one. |
| has_more | [bool](#bool) |  | Whether another page follows. |
| total | [uint64](#uint64) |  | Rows matching the query in the snapshot. |
| counts | [TradeCounts](#darkbot-bsc-v1-TradeCounts) |  | The counts of the whole filtered result. |
| realized_net_pnl_wei | [string](#string) | optional | The ledger pnl_wei of every decided lot of the whole filtered result (CLOSED, FAILED, WRITTEN_OFF), signed decimal wei; absent while one is unresolved. |
| burn_gas_wei | [string](#string) | optional | Gas of the nonce burns in the snapshot (no lot&#39;s), decimal wei; absent when unknown. |






<a name="darkbot-bsc-v1-TradeHistoryQuery"></a>

### TradeHistoryQuery
A bounded, cursor-paged, read-only query of the LIVE trades.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| statuses | [TradeStatus](#darkbot-bsc-v1-TradeStatus) | repeated | Only these states; empty: every state. |
| gains | [bool](#bool) | optional | Only gains (pnl &gt; 0) or only losses (pnl &lt; 0) among CLOSED; unset: both. |
| venues | [string](#string) | repeated | Only these venues; empty: every venue. |
| from_unix_ms | [uint64](#uint64) | optional | BUY time from, unix ms. |
| to_unix_ms | [uint64](#uint64) | optional | BUY time to, unix ms. |
| search | [string](#string) |  | A token address or transaction hash (lot, BUY or SELL), exact 0x hex or a prefix of it. |
| sort | [TradeSort](#darkbot-bsc-v1-TradeSort) |  | The order. |
| cursor | [string](#string) |  | The previous page&#39;s next_cursor; empty: the first page. |
| limit | [uint32](#uint32) |  | At most this many rows; 0: the owner&#39;s default; above its cap: refused (LIMIT_EXCEEDED). |






<a name="darkbot-bsc-v1-TradeRow"></a>

### TradeRow
One lot&#39;s row of the trade table (the 17 V8 columns and their identity).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| lot | [string](#string) |  | The lot: its BUY&#39;s transaction hash, 0x hex (a token bought again is another lot). |
| token | [string](#string) |  | Column token: the token address, 0x hex (authoritative; no symbol without checked metadata). |
| status | [TradeStatus](#darkbot-bsc-v1-TradeStatus) |  | Column status: the ledger state. |
| venue | [string](#string) |  | Column market: the venue its BUY traded on (BSC_VENUE_FLAP, BSC_VENUE_FOUR_CLASSIC, BSC_VENUE_OPENFOUR), from live_ledger Position.venue. |
| component | [string](#string) |  | BSC_EXEC (C3) or BSC_STRATEGY_C. |
| input_wei | [string](#string) |  | Column invest: the BUY&#39;s input, decimal wei. |
| proceeds_wei | [string](#string) | optional | The SELL&#39;s proceeds, decimal wei; absent while not closed. |
| proceeds_unknown | [bool](#bool) |  | True when the SELL is included without the Portal&#39;s trade event (proceeds counted as 0, PnL a lower bound). |
| pnl_wei | [string](#string) | optional | Column pnl: the ledger&#39;s Position.pnl_wei exactly (proceeds − input − gas; FAILED: − gas), signed decimal wei; absent while open. No fee beyond gas is in it (fees_wei). |
| roi_ppm | [sint64](#sint64) | optional | Column roi: pnl over input, parts per million, signed, computed natively; absent with pnl. |
| buy_time | [SourcedTime](#darkbot-bsc-v1-SourcedTime) |  | Column buy: the BUY&#39;s time (its block header; the local send time is in TradeDetail). |
| sell_time | [SourcedTime](#darkbot-bsc-v1-SourcedTime) |  | Column sell: CLOSED only, the SELL&#39;s block header time; WRITTEN_OFF: the write-off&#39;s local wall time (source TIME_SOURCE_LOCAL_WALL, never a block time); absent otherwise (FAILED has none). |
| hold_ms | [uint64](#uint64) | optional | Column hold: CLOSED only, SELL block time − BUY block time, ms, natively; absent otherwise. |
| lead_before_block_us | [sint64](#sint64) | optional | Column lead: the trigger source&#39;s sighting lead before the launch block&#39;s header time, microseconds (negative: after it); absent without a trigger. Never the producer&#39;s T0. |
| detect_to_send_ns | [uint64](#uint64) | optional | Column send: detection (ingress or decision) to the first qualified send, ns, on the runtime clock of the release record; absent without one. |
| send_to_ack_ns | [uint64](#uint64) | optional | Column ack: the first qualified send to the first acknowledgement, ns; absent without one. |
| gas_wei | [string](#string) |  | Column gas: gas of every included or reverted transaction of the lot, decimal wei. |
| fees_wei | [string](#string) | optional | Other proven fees (taxes, builder tips), decimal wei; absent while no native owner attributes them (fees_reason). |
| placement | [TradePlacement](#darkbot-bsc-v1-TradePlacement) |  | Column placement: from the qualified PLACED fact only. |
| exit_reason | [ExitReason](#darkbot-bsc-v1-ExitReason) |  | Column exit: why it ended. |
| exit_reason_source | [ExitReasonSource](#darkbot-bsc-v1-ExitReasonSource) |  | Where the exit reason comes from. |
| exit_reason_detail | [string](#string) |  | The owner&#39;s native exit text (the journal&#39;s or the fact&#39;s); empty without one. |
| buy_tx_hash | [string](#string) |  | Column tx: the BUY transaction hash, 0x hex. |
| sell_tx_hash | [string](#string) |  | The SELL transaction hash, 0x hex; empty while not sold. |
| run | [uint64](#uint64) | optional | The run whose journal holds its release; absent without a release record. |
| ledger_version | [uint64](#uint64) |  | The ledger version the row was read at. |
| quality | [DataQuality](#darkbot-bsc-v1-DataQuality) |  | How far the row is proven (UNRESOLVED during a receipt or reorg window). |
| quality_reasons | [string](#string) | repeated | Why the quality is not VERIFIED, e.g. GAS_UPPER_BOUND (the gas is a bound, not the receipt&#39;s) or CLOCK_SUSPECT (the local timings&#39; clock is not trusted). |
| trigger_source | [string](#string) |  | Column source: LiveAttempt.trigger, the source whose sighting triggered the BUY (C3); empty for a Strategy C BUY (a decision, no trigger). |
| fees_reason | [string](#string) |  | Why fees_wei is absent (FEES_NOT_ATTRIBUTED); empty when present. |
| adopted | [bool](#bool) |  | The lot is an adopted G0.7 BUY (the ledger&#39;s ADOPTED fact). |
| evidence_sha256 | [string](#string) |  | An adopted lot&#39;s G0.7 evidence, SHA-256 lowercase hex; empty otherwise. |






<a name="darkbot-bsc-v1-TradeSighting"></a>

### TradeSighting
One ingress source&#39;s sighting of the lot&#39;s launch.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| source | [string](#string) |  | The source ID (BSC_INGRESS_GETH, …). |
| capability | [string](#string) |  | FULL (the full transaction) or HASH_ONLY. |
| seen | [SourcedTime](#darkbot-bsc-v1-SourcedTime) |  | The local wall-clock time of the sighting. |
| lead_before_block_us | [sint64](#sint64) | optional | Lead before the launch block&#39;s header time, microseconds (negative: after it). |
| first_verified | [bool](#bool) |  | Whether this is the first verified full sighting. |






<a name="darkbot-bsc-v1-TradeTransaction"></a>

### TradeTransaction
One transaction of a lot (BUY, APPROVE, SELL; a nonce burn is no lot&#39;s, its gas is in
TradeHistoryPage.burn_gas_wei): the ledger&#39;s LiveTransaction unchanged, plus what the row needs.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| block_time | [SourcedTime](#darkbot-bsc-v1-SourcedTime) |  | That block&#39;s header time. |
| gas_wei | [string](#string) | optional | Its gas, decimal wei; absent while not included. |
| released | [SourcedTime](#darkbot-bsc-v1-SourcedTime) |  | The local time its bytes were handed to the first adapter. |
| tx | [LiveTransaction](#darkbot-bsc-v1-LiveTransaction) |  | The ledger&#39;s transaction (state RELEASED, UNKNOWN, WITHDRAWN, INCLUDED, REVERTED or REPLACED). |
| orphaned | [bool](#bool) |  | Its including block left the canonical chain (the ledger&#39;s ORPHANED fact). |








<a name="darkbot-bsc-v1-ExitReason"></a>

### ExitReason
Why a position ended. The Strategy C journal&#39;s Y exit codes map one to one: 1 CREATOR_SELL,
2 TARGET, 3 STOP, 4 TIMEOUT (runtime_strategy.rs); any other code is OTHER with its detail.

| Name | Number | Description |
| ---- | ------ | ----------- |
| EXIT_REASON_UNSPECIFIED | 0 | Not set by any producer; never displayed as a value. |
| EXIT_REASON_NONE | 1 | Still held: no exit yet. |
| EXIT_REASON_TARGET | 2 | Y code 2: the net target multiple was reached. |
| EXIT_REASON_STOP | 3 | Y code 3: the net stop multiple was reached. |
| EXIT_REASON_TIMEOUT | 4 | Y code 4: the hold timeout ran out. |
| EXIT_REASON_CREATOR_SELL | 5 | Y code 1: the token&#39;s creator sold (creator-sell exit). |
| EXIT_REASON_OPERATOR_EXIT | 6 | The operator&#39;s EXIT_NOW. |
| EXIT_REASON_WRITE_OFF | 7 | The operator&#39;s WRITE_OFF. |
| EXIT_REASON_BUY_FAILED | 8 | The BUY failed; nothing to exit. |
| EXIT_REASON_OTHER | 10 | An exit whose reason the owner holds in exit_reason_detail only. |



<a name="darkbot-bsc-v1-ExitReasonSource"></a>

### ExitReasonSource
Where an exit reason comes from.

| Name | Number | Description |
| ---- | ------ | ----------- |
| EXIT_REASON_SOURCE_UNSPECIFIED | 0 | Not set by any producer; never displayed as a value. |
| EXIT_REASON_SOURCE_STRATEGY_JOURNAL | 1 | The Strategy C journal&#39;s exit decision (Y record). |
| EXIT_REASON_SOURCE_LEDGER_OPERATOR | 2 | An operator fact of the LIVE ledger (EXIT_NOW, WRITE_OFF). |
| EXIT_REASON_SOURCE_LEDGER_CHAIN | 3 | A chain fact of the LIVE ledger (a failed BUY, a DEX-routed SELL). |
| EXIT_REASON_SOURCE_UNKNOWN | 4 | No fact names it. |



<a name="darkbot-bsc-v1-TradePlacement"></a>

### TradePlacement
Where an own BUY landed relative to the launch (the ledger&#39;s PLACED fact).

| Name | Number | Description |
| ---- | ------ | ----------- |
| TRADE_PLACEMENT_UNSPECIFIED | 0 | Not set by any producer; never displayed as a value. |
| TRADE_PLACEMENT_UNKNOWN | 1 | No PLACED fact (yet). |
| TRADE_PLACEMENT_FIRST | 2 | First other-than-launch BUY of the token in the launch&#39;s block, from the qualified verdict. |
| TRADE_PLACEMENT_LATE | 3 | In the launch&#39;s block after other BUYs. |
| TRADE_PLACEMENT_OTHER_BLOCK | 4 | In another block than the launch. |
| TRADE_PLACEMENT_NO_TOKENS | 5 | Included without tokens received. |



<a name="darkbot-bsc-v1-TradeSort"></a>

### TradeSort
The order of a trade page.

| Name | Number | Description |
| ---- | ------ | ----------- |
| TRADE_SORT_UNSPECIFIED | 0 | Not set: ACTIVE_FIRST. |
| TRADE_SORT_ACTIVE_FIRST | 1 | Pending, open and exiting first, then the rest; each group newest BUY first (V8 default). |
| TRADE_SORT_NEWEST | 2 | Newest BUY first. |
| TRADE_SORT_OLDEST | 3 | Oldest BUY first. |



<a name="darkbot-bsc-v1-TradeStatus"></a>

### TradeStatus
The ledger state of a lot (LivePosition.state plus the owner&#39;s write-off and unknown).

| Name | Number | Description |
| ---- | ------ | ----------- |
| TRADE_STATUS_UNSPECIFIED | 0 | Not set by any producer; never displayed as a value. |
| TRADE_STATUS_PENDING | 1 | The BUY was released, the chain has not included it. |
| TRADE_STATUS_OPEN | 2 | The BUY is included; the position is held. |
| TRADE_STATUS_EXITING | 3 | An exit step is released. |
| TRADE_STATUS_CLOSED | 4 | The SELL is included. |
| TRADE_STATUS_FAILED | 5 | The BUY reverted (only a revert); its gas is the loss. |
| TRADE_STATUS_WRITTEN_OFF | 6 | The operator wrote the lot off (input and gas lost); never a normal SELL. |
| TRADE_STATUS_UNKNOWN | 7 | The owner cannot decide (an UNKNOWN handoff, unreadable facts). |










<a name="darkbot_bsc_v1_wallet-proto"></a>
<p align="right"><a href="#top">Top</a></p>

## darkbot/bsc/v1/wallet.proto



<a name="darkbot-bsc-v1-FiatQuote"></a>

### FiatQuote
A native fiat quote of BNB, the only basis of an EUR setting.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| currency | [string](#string) |  | ISO currency, e.g. EUR. |
| fiat_per_bnb | [string](#string) |  | Fiat per BNB, decimal. |
| observed | [SourcedTime](#darkbot-bsc-v1-SourcedTime) |  | When it was observed. |
| source | [string](#string) |  | Its source. |
| quality | [DataQuality](#darkbot-bsc-v1-DataQuality) |  | How far it is proven (STALE beyond max_age_ms). |
| max_age_ms | [uint64](#uint64) |  | The owner&#39;s freshness bound, ms. |






<a name="darkbot-bsc-v1-MainPayoutPolicy"></a>

### MainPayoutPolicy
Policy B (path 4): Main → external automatic payout, on the FREE Main balance.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| enabled | [bool](#bool) |  | Whether it is enabled (default off). |
| destination_id | [string](#string) |  | The saved, confirmed destination. |
| destination_address | [string](#string) |  | That destination&#39;s address, 0x hex. |
| unit | [PolicyUnit](#darkbot-bsc-v1-PolicyUnit) |  | The unit it is configured in. |
| threshold_available_main_wei | [string](#string) |  | The free Main balance above which it pays out, decimal wei (EUR mode: the owner&#39;s current conversion of threshold_eur). |
| threshold_eur | [string](#string) | optional | EUR mode: the operator&#39;s threshold, decimal EUR; absent in BNB mode. |
| amount | [PolicyAmount](#darkbot-bsc-v1-PolicyAmount) |  | How much it pays out. |
| retain_wei | [string](#string) |  | What always stays on Main (gas and minimum reserve), decimal wei. |
| policy_version | [uint64](#uint64) |  | Its version: the compare-and-swap of its next change. |
| trigger_cursor | [string](#string) |  | The last qualified balance event it acted on (one payout per event). |
| request_id | [string](#string) |  | The request that wrote this version. |
| last_payout | [WalletTransferState](#darkbot-bsc-v1-WalletTransferState) |  | The last payout it caused; absent before any. |
| available_main_wei | [string](#string) | optional | The free Main balance now, decimal wei. |
| effective_version | [uint64](#uint64) | optional | The version in effect at the owner. |
| block_reason | [string](#string) |  | The owner&#39;s reason it does not act (as policy A&#39;s). |
| authorization | [WalletAuthorization](#darkbot-bsc-v1-WalletAuthorization) |  | Its authorization; absent without one (then it acts never). |






<a name="darkbot-bsc-v1-MainPayoutPolicySetCommand"></a>

### MainPayoutPolicySetCommand
Params of `bsc.wallet.payout_policy.set` (prepared).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| enabled | [bool](#bool) |  | Enable or disable. |
| destination_id | [string](#string) |  | The saved, confirmed destination. |
| unit | [PolicyUnit](#darkbot-bsc-v1-PolicyUnit) |  | The unit (EUR only with a qualified native quote). |
| threshold_available_main_wei | [string](#string) |  | BNB mode: the free Main balance threshold, decimal wei. |
| threshold_eur | [string](#string) | optional | EUR mode: the threshold, decimal EUR. |
| amount | [PolicyAmount](#darkbot-bsc-v1-PolicyAmount) |  | Fixed amount or share. |
| retain_wei | [string](#string) |  | What always stays on Main, decimal wei. |
| destination_address | [string](#string) |  | The destination&#39;s address as the operator confirmed it, 0x hex (must equal the saved one). |
| authorization | [WalletAuthorizationScope](#darkbot-bsc-v1-WalletAuthorizationScope) |  | The authorization of an enable; absent for a disable. |






<a name="darkbot-bsc-v1-PolicyAmount"></a>

### PolicyAmount
How much an automatic transfer moves: a fixed amount or a share of what is eligible.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| fixed_wei | [string](#string) |  | A fixed amount, decimal wei (POLICY_UNIT_BNB). |
| fixed_eur | [string](#string) |  | A fixed EUR equivalent, decimal (policy B with POLICY_UNIT_EUR only, refused without a qualified quote; policy A refuses it). |
| share_bps | [uint32](#uint32) |  | A share of the eligible amount, basis points (1…10000): policy A of net_since_last_settlement_wei, policy B of the free Main balance above retain_wei. |






<a name="darkbot-bsc-v1-ProfitTransferPolicy"></a>

### ProfitTransferPolicy
Policy A (path 2): Buyer → Main automatic profit transfer, on REALIZED NET trading profit.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| enabled | [bool](#bool) |  | Whether it is enabled (default off). |
| threshold_realized_net_profit_wei | [string](#string) |  | The realized net profit since the last settlement (net_since_last_settlement_wei: after losses, gas and fees, a loss carried over) above which it transfers, decimal wei. |
| amount | [PolicyAmount](#darkbot-bsc-v1-PolicyAmount) |  | How much of net_since_last_settlement_wei it moves (fixed BNB or a share; never fixed EUR). |
| buyer_min_capital_wei | [string](#string) |  | The trading capital that always stays on the Buyer, decimal wei. |
| buyer_reserve_wei | [string](#string) |  | The exit and gas reserve that always stays on the Buyer, decimal wei. |
| policy_version | [uint64](#uint64) |  | Its version: the compare-and-swap of its next change. |
| settlement_cursor | [string](#string) |  | The ledger position up to which profit is settled (no profit transferred twice). |
| request_id | [string](#string) |  | The request that wrote this version. |
| last_transfer | [WalletTransferState](#darkbot-bsc-v1-WalletTransferState) |  | The last transfer it caused; absent before any. |
| untransferred_profit_wei | [string](#string) | optional | net_since_last_settlement_wei when positive, else 0, as the owner computes it now, decimal wei. |
| to_threshold_wei | [string](#string) | optional | What is still missing to the threshold, decimal wei (0: the next settled profit triggers). |
| effective_version | [uint64](#uint64) | optional | The version in effect at the owner (a stored version applies once the owner took it). |
| block_reason | [string](#string) |  | The owner&#39;s reason it does not act (DISABLED, NO_AUTHORIZATION, AUTHORIZATION_EXPIRED, RESET_BY_RESTART, ECONOMIC_LIVE_DISABLED, …). |
| authorization | [WalletAuthorization](#darkbot-bsc-v1-WalletAuthorization) |  | Its authorization; absent without one (then it acts never). |
| net_since_last_settlement_wei | [string](#string) | optional | Realized net PnL of the lots settled since the last settlement, signed decimal wei (a loss carries over); absent while unresolved. |
| settled_ledger_version | [uint64](#uint64) | optional | The ledger version the settlement cursor is at. |
| finality_depth | [uint64](#uint64) | optional | Blocks a lot&#39;s SELL must be deep before its profit counts. |






<a name="darkbot-bsc-v1-ProfitTransferPolicySetCommand"></a>

### ProfitTransferPolicySetCommand
Params of `bsc.wallet.profit_policy.set` (prepared).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| enabled | [bool](#bool) |  | Enable or disable. |
| threshold_realized_net_profit_wei | [string](#string) |  | The realized net profit threshold, decimal wei. |
| amount | [PolicyAmount](#darkbot-bsc-v1-PolicyAmount) |  | Fixed amount (wei) or share. |
| buyer_min_capital_wei | [string](#string) |  | The capital that stays on the Buyer, decimal wei. |
| buyer_reserve_wei | [string](#string) |  | The exit and gas reserve that stays on the Buyer, decimal wei. |
| authorization | [WalletAuthorizationScope](#darkbot-bsc-v1-WalletAuthorizationScope) |  | The authorization of an enable; absent for a disable. |






<a name="darkbot-bsc-v1-WalletAccount"></a>

### WalletAccount
One DarkBot account with its economics, as the native owner separates them.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| role | [WalletRole](#darkbot-bsc-v1-WalletRole) |  | The account. |
| address | [string](#string) |  | Its address, 0x hex; empty while the owner has none configured. |
| chain_id | [uint64](#uint64) |  | Chain identity. |
| balance | [WalletBalanceSnapshot](#darkbot-bsc-v1-WalletBalanceSnapshot) |  | Its confirmed balance. |
| capital_basis_wei | [string](#string) | optional | BUYER: the trading capital basis (deposits and fundings minus capital taken out), decimal wei. |
| realized_net_pnl_wei | [string](#string) | optional | BUYER: realized net trading PnL after losses, gas and fees, signed decimal wei. |
| transferred_profit_wei | [string](#string) | optional | BUYER: realized net profit already transferred to Main, decimal wei. |
| untransferred_profit_wei | [string](#string) | optional | BUYER: realized net profit not yet transferred, policy A&#39;s net_since_last_settlement_wei when positive, else 0, decimal wei. |
| profit_received_wei | [string](#string) | optional | MAIN: profit received from the Buyer (policy A), decimal wei. |
| other_capital_wei | [string](#string) | optional | MAIN: everything else on Main (deposits, returns): capital, never trading profit, decimal wei. |
| next_nonce | [uint64](#uint64) | optional | The account&#39;s next nonce; absent when unknown. |
| owner_key | [bool](#bool) |  | Whether the native owner holds this account&#39;s key (never for EXTERNAL). |
| reason | [string](#string) |  | Why the owner cannot use the account apart from its switch (NOT_CONFIGURED, KEY_MISSING, UNREADABLE); empty when usable. |
| control | [ControlSwitch](#darkbot-bsc-v1-ControlSwitch) |  | The account&#39;s native wallet switch (BSC_WALLET_MAIN, BSC_WALLET_BUYER, BSC_WALLET_LAUNCHER); absent for EXTERNAL. |






<a name="darkbot-bsc-v1-WalletAuthorization"></a>

### WalletAuthorization
An automatic policy&#39;s authorization as the native owner holds it.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| scope_text | [string](#string) |  | The canonical scope text. |
| scope_sha256 | [string](#string) |  | Its SHA-256, lowercase hex. |
| expires_unix_ms | [uint64](#uint64) |  | Its expiry, unix ms. |
| max_per_transfer_wei | [string](#string) |  | The most one transfer may move, decimal wei. |
| max_total_wei | [string](#string) |  | The most all its transfers together may move, decimal wei. |
| used_total_wei | [string](#string) |  | What its transfers moved so far, decimal wei. |
| request_id | [string](#string) |  | The request that wrote it. |
| block_reason | [string](#string) |  | Why it admits nothing (EXPIRED, TOTAL_USED, RESET_BY_RESTART); empty while it holds. |






<a name="darkbot-bsc-v1-WalletAuthorizationBounds"></a>

### WalletAuthorizationBounds
The owner&#39;s configured bounds of every wallet authorization (never a client literal).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| max_per_transfer_wei | [string](#string) |  | The most one transfer may move, decimal wei. |
| max_total_wei | [string](#string) |  | The most an authorization&#39;s transfers may move together, decimal wei. |
| max_validity_ms | [uint64](#uint64) |  | The longest an authorization may hold, ms. |
| draft_validity_ms | [uint64](#uint64) |  | How long a DRAFT may wait for its authorization, ms. |






<a name="darkbot-bsc-v1-WalletAuthorizationScope"></a>

### WalletAuthorizationScope
The authorization scope of an automatic policy as the operator sets it.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| expires_unix_ms | [uint64](#uint64) |  | Its expiry, unix ms (within WalletAuthorizationBounds.max_validity_ms). |
| max_per_transfer_wei | [string](#string) |  | The most one transfer may move, decimal wei. |
| max_total_wei | [string](#string) |  | The most all its transfers together may move, decimal wei. |
| scope_sha256 | [string](#string) |  | SHA-256 of the policy&#39;s canonical scope text the operator was shown, lowercase hex. |






<a name="darkbot-bsc-v1-WalletBalanceSnapshot"></a>

### WalletBalanceSnapshot
A wallet balance at one block, with its age and source.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| role | [WalletRole](#darkbot-bsc-v1-WalletRole) |  | The wallet. |
| address | [string](#string) |  | Its address, 0x hex. |
| chain_id | [uint64](#uint64) |  | Chain identity. |
| balance_wei | [string](#string) |  | The confirmed balance, decimal wei. |
| reserved_wei | [string](#string) | optional | Of it reserved (open positions, exit and gas budgets, pending transfers), decimal wei; absent when unknown. |
| available_wei | [string](#string) | optional | Of it freely available, decimal wei; absent when unknown. |
| block_number | [uint64](#uint64) | optional | The block it was read at. |
| observed | [SourcedTime](#darkbot-bsc-v1-SourcedTime) |  | When it was read. |
| source | [string](#string) |  | The reader (LIVE_PROBE, READINESS_SNAPSHOT, TREASURY_OWNER). |
| quality | [DataQuality](#darkbot-bsc-v1-DataQuality) |  | How far it is proven (STALE when older than the owner&#39;s freshness rule). |






<a name="darkbot-bsc-v1-WalletDestination"></a>

### WalletDestination
A saved, confirmed external destination (e.g. a MetaMask wallet&#39;s BSC address).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| destination_id | [string](#string) |  | Stable owner identity. |
| label | [string](#string) |  | The operator&#39;s label. |
| address | [string](#string) |  | The address, 0x hex (checksummed by the owner). |
| chain_id | [uint64](#uint64) |  | Chain identity. |
| confirmed | [bool](#bool) |  | Whether the operator confirmed it (bsc.wallet.destination.confirm; only confirmed ones receive). |
| created_unix_ms | [uint64](#uint64) |  | When it was saved, unix ms. |
| in_use | [bool](#bool) |  | Policy B pays to it: it cannot be changed or removed. |






<a name="darkbot-bsc-v1-WalletDestinationConfirmCommand"></a>

### WalletDestinationConfirmCommand
Params of `bsc.wallet.destination.confirm` (prepared): the operator checked this saved address.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| destination_id | [string](#string) |  | The saved destination (equal to CommandRequest.resource_id). |
| address | [string](#string) |  | Its address as the operator checked it, 0x hex (must equal the saved one). |






<a name="darkbot-bsc-v1-WalletDestinationSetCommand"></a>

### WalletDestinationSetCommand
Params of `bsc.wallet.destination.set` (prepared): saves an UNCONFIRMED destination, or removes one.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| label | [string](#string) |  | The operator&#39;s label. |
| address | [string](#string) |  | The address, 0x hex. |
| chain_id | [uint64](#uint64) |  | Chain identity (must be Main&#39;s). |
| remove | [bool](#bool) |  | Remove the destination instead (its id in CommandRequest.resource_id). |






<a name="darkbot-bsc-v1-WalletFundTradingCommand"></a>

### WalletFundTradingCommand
Path 1 of `bsc.wallet.transfer.prepare` (never an action of its own): Main → the fixed Buyer
address; capital.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| amount_wei | [string](#string) |  | The amount, decimal wei. |
| max_fee_wei | [string](#string) |  | The most gas it may cost, decimal wei; the owner refuses above it. |
| chain_id | [uint64](#uint64) |  | Chain identity (the owner refuses another). |
| buyer_address | [string](#string) |  | The Buyer address the operator was shown, 0x hex (must equal the owner&#39;s Buyer). |






<a name="darkbot-bsc-v1-WalletHistoryPage"></a>

### WalletHistoryPage
One page of wallet movements, newest first, bound to one owner snapshot.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| availability | [Availability](#darkbot-bsc-v1-Availability) |  | Whether the native owner answered. |
| unavailable_reason | [string](#string) |  | Why it is not AVAILABLE. |
| provenance | [OwnerProvenance](#darkbot-bsc-v1-OwnerProvenance) |  | The owner and the snapshot the cursor is bound to. |
| movements | [WalletMovement](#darkbot-bsc-v1-WalletMovement) | repeated | The movements. |
| next_cursor | [string](#string) |  | The cursor of the next page; empty on the last one. |
| has_more | [bool](#bool) |  | Whether another page follows. |
| deposited_total_wei | [string](#string) | optional | External deposits of the whole filtered result, decimal wei; absent while an UNATTRIBUTED movement makes the sum unproven. |
| paid_out_total_wei | [string](#string) | optional | Payouts to external addresses of the whole filtered result, decimal wei (a positive amount); absent while an UNATTRIBUTED movement makes the sum unproven. |






<a name="darkbot-bsc-v1-WalletHistoryQuery"></a>

### WalletHistoryQuery
A bounded, cursor-paged, read-only query of wallet movements.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| roles | [WalletRole](#darkbot-bsc-v1-WalletRole) | repeated | Only these wallets; empty: every DarkBot wallet. |
| kinds | [WalletMovementKind](#darkbot-bsc-v1-WalletMovementKind) | repeated | Only these kinds; empty: every kind. |
| from_unix_ms | [uint64](#uint64) | optional | Block time from, unix ms. |
| to_unix_ms | [uint64](#uint64) | optional | Block time to, unix ms. |
| cursor | [string](#string) |  | The previous page&#39;s next_cursor; empty: the first page. |
| limit | [uint32](#uint32) |  | At most this many movements; 0: the owner&#39;s default; above its cap: refused (LIMIT_EXCEEDED). |






<a name="darkbot-bsc-v1-WalletMovement"></a>

### WalletMovement
One wallet movement, proven by its block and receipt; an internal transfer is one movement in
each of the two ledgers.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| movement_id | [string](#string) |  | Stable owner identity of the movement. |
| role | [WalletRole](#darkbot-bsc-v1-WalletRole) |  | The wallet it belongs to. |
| wallet_address | [string](#string) |  | That wallet&#39;s address, 0x hex. |
| chain_id | [uint64](#uint64) |  | Chain identity (56 BSC Mainnet; 97 Chapel never mixed with it). |
| kind | [WalletMovementKind](#darkbot-bsc-v1-WalletMovementKind) |  | What it is. |
| counterparty_address | [string](#string) |  | The other side, 0x hex; empty for a fee. |
| counterparty_role | [WalletRole](#darkbot-bsc-v1-WalletRole) |  | The other side&#39;s role. |
| amount_wei | [string](#string) |  | The amount for this wallet, signed decimal wei (&#43; received, − sent). |
| fee_wei | [string](#string) |  | Its gas and fees, decimal wei. |
| tx_hash | [string](#string) |  | The transaction hash, 0x hex. |
| nonce | [uint64](#uint64) | optional | The sender&#39;s nonce, when this wallet sent it. |
| block_number | [uint64](#uint64) | optional | The including block; absent while not included. |
| block_time | [SourcedTime](#darkbot-bsc-v1-SourcedTime) |  | That block&#39;s header time. |
| status | [ChainTxStatus](#darkbot-bsc-v1-ChainTxStatus) |  | What the chain made of it. |
| origin | [string](#string) |  | The native fact it comes from: LIVE_LEDGER, TREASURY_OWNER or CHAIN_BACKFILL. |
| lot | [string](#string) |  | The lot of a trade movement; empty otherwise. |
| transfer_request_id | [string](#string) |  | The transfer request of a funding, profit transfer or payout; empty otherwise. |
| quality | [DataQuality](#darkbot-bsc-v1-DataQuality) |  | How far it is proven. |
| policy_version | [uint64](#uint64) | optional | The policy version that caused an automatic transfer; absent otherwise. |






<a name="darkbot-bsc-v1-WalletPayoutCommand"></a>

### WalletPayoutCommand
Path 3 of `bsc.wallet.transfer.prepare` (never an action of its own): Main → a saved, confirmed
external destination.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| destination_id | [string](#string) |  | The saved destination. |
| destination_address | [string](#string) |  | Its address as the operator confirmed it, 0x hex (must equal the saved one). |
| chain_id | [uint64](#uint64) |  | Chain identity. |
| amount_wei | [string](#string) |  | The amount, decimal wei. |
| max_fee_wei | [string](#string) |  | The most gas it may cost, decimal wei; the owner refuses above it. |






<a name="darkbot-bsc-v1-WalletTransferAuthorizeCommand"></a>

### WalletTransferAuthorizeCommand
Params of `bsc.wallet.transfer.authorize` (prepared): CommandRequest.resource_id names the DRAFT.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| scope_sha256 | [string](#string) |  | The DRAFT&#39;s scope_sha256 the operator was shown (must equal the owner&#39;s). |






<a name="darkbot-bsc-v1-WalletTransferPrepareCommand"></a>

### WalletTransferPrepareCommand
Params of `bsc.wallet.transfer.prepare` (prepared): a manual transfer as a native DRAFT.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| fund_trading | [WalletFundTradingCommand](#darkbot-bsc-v1-WalletFundTradingCommand) |  | Path 1: Main → Buyer capital funding. |
| payout | [WalletPayoutCommand](#darkbot-bsc-v1-WalletPayoutCommand) |  | Path 3: Main → a confirmed external destination. |






<a name="darkbot-bsc-v1-WalletTransferReceipt"></a>

### WalletTransferReceipt
The chain receipt of a transfer.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| request_id | [string](#string) |  | The request. |
| status | [TransferStatus](#darkbot-bsc-v1-TransferStatus) |  | Its final or current lifecycle state. |
| tx_hash | [string](#string) |  | Its transaction hash, 0x hex. |
| block_number | [uint64](#uint64) | optional | The including block. |
| block_time | [SourcedTime](#darkbot-bsc-v1-SourcedTime) |  | That block&#39;s header time. |
| gas_used | [uint64](#uint64) | optional | Gas used. |
| fee_wei | [string](#string) | optional | The fee paid, decimal wei. |
| reason | [string](#string) |  | The owner&#39;s reason (a revert, a replacement); empty when confirmed. |






<a name="darkbot-bsc-v1-WalletTransferState"></a>

### WalletTransferState
A transfer as its native owner holds it (any of the four paths).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| request_id | [string](#string) |  | The request that created it: the operator&#39;s CommandRequest.request_id, or for an automatic transfer the owner&#39;s own request for its trigger (never a policy&#39;s request_id). |
| command_id | [string](#string) |  | The owner&#39;s durable intent; a DRAFT&#39;s identity (the resource_id of its authorization). |
| kind | [WalletMovementKind](#darkbot-bsc-v1-WalletMovementKind) |  | The path. |
| source_role | [WalletRole](#darkbot-bsc-v1-WalletRole) |  | The wallet that sends. |
| source_address | [string](#string) |  | Its address, 0x hex. |
| destination_role | [WalletRole](#darkbot-bsc-v1-WalletRole) |  | The receiving wallet&#39;s role (BUYER, MAIN or EXTERNAL). |
| destination_address | [string](#string) |  | The destination address, 0x hex. |
| chain_id | [uint64](#uint64) |  | Chain identity. |
| amount_wei | [string](#string) |  | The amount, decimal wei. |
| nonce | [uint64](#uint64) | optional | The nonce it uses (fixed in the DRAFT); absent before the owner assigned one. |
| tx_hash | [string](#string) |  | Its transaction hash, 0x hex; empty before signing. |
| status | [TransferStatus](#darkbot-bsc-v1-TransferStatus) |  | Its lifecycle. |
| failure_reason | [string](#string) |  | Why it failed or is unknown; empty otherwise. |
| created_unix_ms | [uint64](#uint64) |  | Creation time, unix ms. |
| updated_unix_ms | [uint64](#uint64) |  | Last change, unix ms. |
| policy_version | [uint64](#uint64) | optional | The policy version that caused it; absent for a manual transfer. |
| trigger_id | [string](#string) |  | An automatic transfer&#39;s economic trigger (policy A: the settlement id of the closed, chain- confirmed lots it settles; policy B: the qualified balance event): at most one intent per trigger. Empty for a manual transfer. |
| receipt | [WalletTransferReceipt](#darkbot-bsc-v1-WalletTransferReceipt) |  | The chain receipt (fee paid, block); absent before inclusion. |
| fee_estimate_wei | [string](#string) | optional | The owner&#39;s fee estimate, decimal wei; absent when unknown. |
| reserve_after_wei | [string](#string) | optional | What stays on the sending wallet after it (amount and fee estimate), decimal wei. |
| scope_text | [string](#string) |  | The canonical scope text (manual: darkbot.bsc.wallet-transfer.v1; automatic: its policy&#39;s). |
| scope_sha256 | [string](#string) |  | SHA-256 of scope_text, lowercase hex. |
| expires_unix_ms | [uint64](#uint64) | optional | A DRAFT&#39;s expiry, unix ms: after it the owner never sends it (EXPIRED). |
| max_block | [uint64](#uint64) | optional | The last block at which the owner still releases it; absent when unbounded. |
| authorize_request_id | [string](#string) |  | The request that authorized it; empty while a DRAFT and for an automatic transfer. |






<a name="darkbot-bsc-v1-WalletTreasuryQuery"></a>

### WalletTreasuryQuery
A read of the treasury state.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| roles | [WalletRole](#darkbot-bsc-v1-WalletRole) | repeated | Only these wallets; empty: every DarkBot wallet. |






<a name="darkbot-bsc-v1-WalletTreasuryState"></a>

### WalletTreasuryState
The BSC wallet state of the native treasury owner (accounts, destinations, both policies).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| availability | [Availability](#darkbot-bsc-v1-Availability) |  | Whether the native owner answered. |
| unavailable_reason | [string](#string) |  | Why it is not AVAILABLE. |
| provenance | [OwnerProvenance](#darkbot-bsc-v1-OwnerProvenance) |  | The owner. |
| accounts | [WalletAccount](#darkbot-bsc-v1-WalletAccount) | repeated | MAIN, BUYER and the separate LAUNCHER. |
| destinations | [WalletDestination](#darkbot-bsc-v1-WalletDestination) | repeated | The saved external destinations. |
| profit_policy | [ProfitTransferPolicy](#darkbot-bsc-v1-ProfitTransferPolicy) |  | Policy A (Buyer → Main profit transfer); absent before it was ever set. |
| payout_policy | [MainPayoutPolicy](#darkbot-bsc-v1-MainPayoutPolicy) |  | Policy B (Main → external payout); absent before it was ever set. |
| transfers | [WalletTransferState](#darkbot-bsc-v1-WalletTransferState) | repeated | DRAFT, pending and recent transfers of every path, newest first (bounded; the history has all). |
| treasury_version | [uint64](#uint64) |  | The treasury version: the compare-and-swap of a destination change, a funding or a payout. |
| eur_quote | [FiatQuote](#darkbot-bsc-v1-FiatQuote) |  | The native BNB/EUR quote; absent without a qualified price owner (then no EUR setting). |
| transfer_fee_estimate_wei | [string](#string) | optional | The owner&#39;s current fee estimate of one plain BNB transfer (gas limit × gas price), decimal wei; absent when unknown (then no manual transfer is offered). |
| authorization_bounds | [WalletAuthorizationBounds](#darkbot-bsc-v1-WalletAuthorizationBounds) |  | The configured bounds of every authorization; absent while unreadable (then none is offered). |
| decided_request_ids | [string](#string) | repeated | The wallet requests the owner decided most recently (any wallet action), newest first, bounded: a request whose handoff was unknown is decided once it appears here. |








<a name="darkbot-bsc-v1-ChainTxStatus"></a>

### ChainTxStatus
What the chain made of a wallet transaction.

| Name | Number | Description |
| ---- | ------ | ----------- |
| CHAIN_TX_STATUS_UNSPECIFIED | 0 | Not set by any producer; never displayed as a value. |
| CHAIN_TX_STATUS_PENDING | 1 | Released, not included yet. |
| CHAIN_TX_STATUS_INCLUDED | 2 | Included in the canonical chain. |
| CHAIN_TX_STATUS_FINAL | 3 | Included and final. |
| CHAIN_TX_STATUS_REVERTED | 4 | Included and reverted. |
| CHAIN_TX_STATUS_REPLACED | 5 | Another transaction took its nonce. |
| CHAIN_TX_STATUS_ORPHANED | 6 | Its block left the canonical chain. |
| CHAIN_TX_STATUS_UNKNOWN | 7 | The owner cannot decide. |



<a name="darkbot-bsc-v1-PolicyUnit"></a>

### PolicyUnit
The unit an automatic policy is configured in.

| Name | Number | Description |
| ---- | ------ | ----------- |
| POLICY_UNIT_UNSPECIFIED | 0 | Not set: BNB. |
| POLICY_UNIT_BNB | 1 | BNB (wei on chain). |
| POLICY_UNIT_EUR | 2 | EUR equivalent: accepted only with a qualified, fresh native BNB/EUR quote; the transaction is still BNB/wei, converted by the native owner at the trigger. |



<a name="darkbot-bsc-v1-TransferStatus"></a>

### TransferStatus
The lifecycle of a transfer, from native evidence only.

| Name | Number | Description |
| ---- | ------ | ----------- |
| TRANSFER_STATUS_UNSPECIFIED | 0 | Not set by any producer; never displayed as a value. |
| TRANSFER_STATUS_DRAFT | 1 | Prepared by the owner (bsc.wallet.transfer.prepare): its scope fixed, nothing sent before its authorization; one open DRAFT per path. |
| TRANSFER_STATUS_PENDING | 2 | Authorized (bsc.wallet.transfer.authorize, or a policy within its authorization) and admitted; not released yet. |
| TRANSFER_STATUS_SENT | 3 | Its bytes were handed to the network. |
| TRANSFER_STATUS_CONFIRMED | 4 | Included (and final per the owner&#39;s depth). |
| TRANSFER_STATUS_FAILED | 5 | Refused, reverted or replaced (failure_reason). |
| TRANSFER_STATUS_UNKNOWN | 6 | Its handoff did not complete; the chain decides, never sent again automatically. |
| TRANSFER_STATUS_EXPIRED | 7 | A DRAFT not authorized before its expiry: never sent. |



<a name="darkbot-bsc-v1-WalletMovementKind"></a>

### WalletMovementKind
What a wallet movement is, economically separated (capital never counted as profit, an internal
transfer never counted twice).

| Name | Number | Description |
| ---- | ------ | ----------- |
| WALLET_MOVEMENT_KIND_UNSPECIFIED | 0 | Not set by any producer; never displayed as a value. |
| WALLET_MOVEMENT_KIND_DEPOSIT | 1 | BNB received from outside DarkBot (capital, never trading profit). |
| WALLET_MOVEMENT_KIND_CAPITAL_FUNDING | 2 | Main → Buyer manual capital funding (path 1). |
| WALLET_MOVEMENT_KIND_PROFIT_TRANSFER | 3 | Buyer → Main automatic profit transfer (path 2, policy A). |
| WALLET_MOVEMENT_KIND_PAYOUT_MANUAL | 4 | Main → external manual payout (path 3). |
| WALLET_MOVEMENT_KIND_PAYOUT_AUTO | 5 | Main → external automatic payout (path 4, policy B). |
| WALLET_MOVEMENT_KIND_TRADE_BUY | 6 | A trade BUY&#39;s input. |
| WALLET_MOVEMENT_KIND_TRADE_SELL | 7 | A trade SELL&#39;s proceeds. |
| WALLET_MOVEMENT_KIND_FEE | 8 | Gas or a proven fee without its own movement. |
| WALLET_MOVEMENT_KIND_EXTERNAL_OUT | 9 | BNB that left the wallet by a transaction no DarkBot owner made. |
| WALLET_MOVEMENT_KIND_UNATTRIBUTED | 10 | A balance change the owner cannot attribute to one transaction (internal value transfers are not visible without getLogs/traces, which are not free): never counted as a deposit or payout. |
| WALLET_MOVEMENT_KIND_CAPITAL_RETURN | 11 | Buyer → Main capital taken back (capital, never profit). |
| WALLET_MOVEMENT_KIND_LAUNCH | 12 | A G0.7 launcher&#39;s launch (its input and gas). |
| WALLET_MOVEMENT_KIND_NONCE_BURN | 13 | A nonce burn (a transfer of nothing to itself): gas only. |



<a name="darkbot-bsc-v1-WalletRole"></a>

### WalletRole
A wallet&#39;s role.

| Name | Number | Description |
| ---- | ------ | ----------- |
| WALLET_ROLE_UNSPECIFIED | 0 | Not set by any producer; never displayed as a value. |
| WALLET_ROLE_LAUNCHER | 1 | The G0.7 launcher wallet (controlled test launches only). |
| WALLET_ROLE_BUYER | 2 | The trading account (the LIVE buyer wallet). |
| WALLET_ROLE_EXTERNAL | 3 | An address outside DarkBot (a deposit&#39;s sender, a payout&#39;s receiver); no key in DarkBot. |
| WALLET_ROLE_MAIN | 4 | The main account / treasury (its key only at the native BSC owner). |










<a name="darkbot_bsc_v1_watch-proto"></a>
<p align="right"><a href="#top">Top</a></p>

## darkbot/bsc/v1/watch.proto



<a name="darkbot-bsc-v1-BscWatchQuery"></a>

### BscWatchQuery
A read of the WATCH projection.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| subject_ids | [string](#string) | repeated | Only these subjects; empty: every subject. |






<a name="darkbot-bsc-v1-BscWatchState"></a>

### BscWatchState
The WATCH projection for the Health window.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| availability | [Availability](#darkbot-bsc-v1-Availability) |  | Whether the native owner answered (NOT_IMPLEMENTED in REV.1.0). |
| unavailable_reason | [string](#string) |  | Why it is not AVAILABLE. |
| provenance | [OwnerProvenance](#darkbot-bsc-v1-OwnerProvenance) |  | The owner. |
| subjects | [WatchSubjectState](#darkbot-bsc-v1-WatchSubjectState) | repeated | Every watched subject. |
| recent_events | [WatchEvent](#darkbot-bsc-v1-WatchEvent) | repeated | The latest events, newest first (bounded). |






<a name="darkbot-bsc-v1-WatchEvent"></a>

### WatchEvent
One WATCH event.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| event_id | [string](#string) |  | Stable event identity. |
| subject_id | [string](#string) |  | The subject. |
| event | [string](#string) |  | DRIFT, UPDATE, REQUALIFICATION_STARTED, REQUALIFICATION_PASSED or REQUALIFICATION_FAILED. |
| from | [string](#string) |  | The binding before. |
| to | [string](#string) |  | The observation after. |
| source | [string](#string) |  | The source of the observation. |
| unix_ms | [uint64](#uint64) |  | When it happened, unix ms. |
| detail | [string](#string) |  | The owner&#39;s detail. |






<a name="darkbot-bsc-v1-WatchSubjectState"></a>

### WatchSubjectState
One watched subject.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| subject_id | [string](#string) |  | Stable subject identity (e.g. BSC_VENUE_FLAP). |
| kind | [WatchSubjectKind](#darkbot-bsc-v1-WatchSubjectKind) |  | What it is. |
| bound | [string](#string) |  | The version or code hash the qualification is bound to. |
| observed | [string](#string) |  | The version or code hash observed. |
| verdict | [WatchVerdict](#darkbot-bsc-v1-WatchVerdict) |  | What WATCH observed. |
| requalification | [RequalificationState](#darkbot-bsc-v1-RequalificationState) |  | The requalification it requires. |
| latched | [bool](#bool) |  | Whether a drift latch holds (cleared only by a passed requalification). |
| source | [string](#string) |  | The source of the observation (RPC, announcement, probe). |
| observed_unix_ms | [uint64](#uint64) | optional | When it was observed, unix ms. |
| drift_since_unix_ms | [uint64](#uint64) | optional | When the current drift was first observed, unix ms; absent without one. |
| reason | [string](#string) |  | The owner&#39;s reason. |








<a name="darkbot-bsc-v1-RequalificationState"></a>

### RequalificationState
The requalification a drift requires.

| Name | Number | Description |
| ---- | ------ | ----------- |
| REQUALIFICATION_STATE_UNSPECIFIED | 0 | Not set by any producer; never displayed as a value. |
| REQUALIFICATION_STATE_NOT_REQUIRED | 1 | Nothing to requalify. |
| REQUALIFICATION_STATE_REQUIRED | 2 | Required; the subject admits no BUY until it passes (latched). |
| REQUALIFICATION_STATE_RUNNING | 3 | Running. |
| REQUALIFICATION_STATE_PASSED | 4 | Passed against the new binding. |
| REQUALIFICATION_STATE_FAILED | 5 | Failed. |



<a name="darkbot-bsc-v1-WatchSubjectKind"></a>

### WatchSubjectKind
What a watched subject is.

| Name | Number | Description |
| ---- | ------ | ----------- |
| WATCH_SUBJECT_KIND_UNSPECIFIED | 0 | Not set by any producer; never displayed as a value. |
| WATCH_SUBJECT_KIND_VENUE_DEPLOYMENT | 1 | A venue&#39;s deployment (Flap Portal, Four.meme Classic TokenManager2, OpenFour Core). |
| WATCH_SUBJECT_KIND_CONTRACT | 2 | A pinned contract (implementation address, code hash). |
| WATCH_SUBJECT_KIND_PROVIDER | 3 | An RPC or ingress provider. |
| WATCH_SUBJECT_KIND_BUILDER | 4 | An inclusion builder. |



<a name="darkbot-bsc-v1-WatchVerdict"></a>

### WatchVerdict
What WATCH observed of a subject against its qualified binding.

| Name | Number | Description |
| ---- | ------ | ----------- |
| WATCH_VERDICT_UNSPECIFIED | 0 | Not set by any producer; never displayed as a value. |
| WATCH_VERDICT_UNKNOWN | 1 | No observation yet (WATCH not running). |
| WATCH_VERDICT_UNCHANGED | 2 | Observed equal to the qualified binding. |
| WATCH_VERDICT_DRIFT_DETECTED | 3 | Observed different from the qualified binding (version, code hash, behaviour). |
| WATCH_VERDICT_UPDATE_DETECTED | 4 | A new version or deployment was announced or observed. |










<a name="darkbot_bsc_v1_query-proto"></a>
<p align="right"><a href="#top">Top</a></p>

## darkbot/bsc/v1/query.proto



<a name="darkbot-bsc-v1-BscQueryRequest"></a>

### BscQueryRequest
One bounded query.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| request_id | [string](#string) |  | Client-generated identity for correlation (UUIDv4); a query changes nothing. |
| trades | [TradeHistoryQuery](#darkbot-bsc-v1-TradeHistoryQuery) |  | A page of LIVE trades. |
| trade | [TradeDetailQuery](#darkbot-bsc-v1-TradeDetailQuery) |  | One lot&#39;s detail. |
| training_runs | [TrainingRunQuery](#darkbot-bsc-v1-TrainingRunQuery) |  | A page of training runs. |
| training_profiles | [TrainingProfilesQuery](#darkbot-bsc-v1-TrainingProfilesQuery) |  | The training profiles. |
| wallet_movements | [WalletHistoryQuery](#darkbot-bsc-v1-WalletHistoryQuery) |  | A page of wallet movements. |
| treasury | [WalletTreasuryQuery](#darkbot-bsc-v1-WalletTreasuryQuery) |  | The treasury state. |
| series | [PerformanceSeriesQuery](#darkbot-bsc-v1-PerformanceSeriesQuery) |  | One performance series. |
| watch | [BscWatchQuery](#darkbot-bsc-v1-BscWatchQuery) |  | The WATCH projection (REV.2.0). |
| strategy_settings | [StrategyCSettingsQuery](#darkbot-bsc-v1-StrategyCSettingsQuery) |  | The stored and running Strategy C configuration. |
| replay_sources | [ReplaySourceQuery](#darkbot-bsc-v1-ReplaySourceQuery) |  | A page of the replay catalogue. |






<a name="darkbot-bsc-v1-BscQueryResponse"></a>

### BscQueryResponse
The answer to one query.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| request_id | [string](#string) |  | Echo of the request. |
| availability | [Availability](#darkbot-bsc-v1-Availability) |  | Whether the native owner answered. |
| unavailable_reason | [string](#string) |  | Why it is not AVAILABLE. |
| trades | [TradeHistoryPage](#darkbot-bsc-v1-TradeHistoryPage) |  | A page of LIVE trades. |
| trade | [TradeDetail](#darkbot-bsc-v1-TradeDetail) |  | One lot&#39;s detail. |
| training_runs | [TrainingRunPage](#darkbot-bsc-v1-TrainingRunPage) |  | A page of training runs. |
| training_profiles | [TrainingProfiles](#darkbot-bsc-v1-TrainingProfiles) |  | The training profiles. |
| wallet_movements | [WalletHistoryPage](#darkbot-bsc-v1-WalletHistoryPage) |  | A page of wallet movements. |
| treasury | [WalletTreasuryState](#darkbot-bsc-v1-WalletTreasuryState) |  | The treasury state. |
| series | [PerformanceSeries](#darkbot-bsc-v1-PerformanceSeries) |  | One performance series. |
| watch | [BscWatchState](#darkbot-bsc-v1-BscWatchState) |  | The WATCH projection (REV.2.0). |
| strategy_settings | [StrategyCSettingsState](#darkbot-bsc-v1-StrategyCSettingsState) |  | The stored and running Strategy C configuration. |
| replay_sources | [ReplaySourcePage](#darkbot-bsc-v1-ReplaySourcePage) |  | A page of the replay catalogue. |
| refusal | [QueryRefusal](#darkbot-bsc-v1-QueryRefusal) |  | Why the owner refused the query (no result then); UNSPECIFIED otherwise. |
| refusal_detail | [string](#string) |  | The owner&#39;s detail of the refusal; empty without one. |








<a name="darkbot-bsc-v1-QueryRefusal"></a>

### QueryRefusal
Why the owner refused a query.

| Name | Number | Description |
| ---- | ------ | ----------- |
| QUERY_REFUSAL_UNSPECIFIED | 0 | No refusal. |
| QUERY_REFUSAL_CURSOR_STALE | 1 | The cursor&#39;s snapshot is gone (compaction, restart): start again from the first page. |
| QUERY_REFUSAL_CURSOR_INVALID | 2 | The cursor is not the owner&#39;s, or belongs to another filter or sort. |
| QUERY_REFUSAL_LIMIT_EXCEEDED | 3 | The limit is above the owner&#39;s cap. |
| QUERY_REFUSAL_INVALID_FILTER | 4 | A filter value the owner does not accept (unknown state, venue or kind, a reversed period). |










<a name="darkbot_control_v1_topics-proto"></a>
<p align="right"><a href="#top">Top</a></p>

## darkbot/control/v1/topics.proto



<a name="darkbot-control-v1-TopicDescriptor"></a>

### TopicDescriptor
Statische Bindung Topic -&gt; Payload-Typ. Das Gateway sendet die Tabelle im Welcome;
der Client vergleicht sie mit seiner Compile-Time-Tabelle (core/realtime/topicTable.ts).
Abweichung =&gt; SCHEMA_INVALID, Topic nicht abonnieren. Das ersetzt google.protobuf.Any.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| topic | [TopicId](#darkbot-control-v1-TopicId) |  |  |
| class | [TopicClass](#darkbot-control-v1-TopicClass) |  |  |
| project | [ProjectId](#darkbot-control-v1-ProjectId) |  |  |
| wire_name | [string](#string) |  | z. B. &#34;project0.wallets.state&#34; (nur Anzeige/Logging, nie Routing-Schlüssel) |
| payload_type | [string](#string) |  | Voll qualifizierter Proto-Messagename des Payloads, z. B. &#34;darkbot.project0.v1.WalletsState&#34;. Leer, solange die Domänenprojektion (F0-VAR) nicht existiert =&gt; available = false. |
| available | [bool](#bool) |  | false =&gt; Subscribe wird mit TOPIC_UNAVAILABLE abgelehnt (kein Fake-Adapter). |








<a name="darkbot-control-v1-ProjectId"></a>

### ProjectId
Projektdomänen sind strikt getrennte Owner. Ein TopicId gehört genau einem Projekt
(Ausnahme: OVERVIEW_AGGREGATES, Publisher ist das Gateway selbst).

| Name | Number | Description |
| ---- | ------ | ----------- |
| PROJECT_ID_UNSPECIFIED | 0 |  |
| PROJECT_ID_PROJECT_0 | 1 |  |
| PROJECT_ID_BSC | 2 |  |
| PROJECT_ID_SOLANA | 3 |  |



<a name="darkbot-control-v1-TopicClass"></a>

### TopicClass
STATE: vollständiger kleiner Topic-Wert, last-value-wins, Ordnung (generation, seq).
EVENT: durables Journal-Ereignis, Ordnung (journal_epoch, durable_cursor), at-least-once.

| Name | Number | Description |
| ---- | ------ | ----------- |
| TOPIC_CLASS_UNSPECIFIED | 0 |  |
| TOPIC_CLASS_STATE | 1 |  |
| TOPIC_CLASS_EVENT | 2 |  |



<a name="darkbot-control-v1-TopicId"></a>

### TopicId
Nummernräume: 1-9 global, 10-29 Project_0, 30-49 BSC, 50-69 Solana. Nie wiederverwenden.
Wire-Name und Klasse laut Phasenplan §5 / Wire V1.3 §6 stehen im Kommentar und
verbindlich in der TopicDescriptor-Tabelle, die das Gateway im Welcome liefert.

| Name | Number | Description |
| ---- | ------ | ----------- |
| TOPIC_ID_UNSPECIFIED | 0 |  |
| TOPIC_ID_OVERVIEW_AGGREGATES | 1 | overview.aggregates STATE Publisher: Gateway (eigene generation) |
| TOPIC_ID_PROJECT0_STATUS | 10 | project0.status STATE enthält pending_commands[], recent_command_results[], config_version |
| TOPIC_ID_PROJECT0_FINANCE_SUMMARY | 11 | project0.finance.summary STATE gleiche finance_snapshot_id wie wallets.state |
| TOPIC_ID_PROJECT0_LIQUIDATOR_STATE | 12 | project0.liquidator.state STATE profile_version; Draft != Running |
| TOPIC_ID_PROJECT0_CAPTURE_STATE | 13 | project0.capture.state STATE |
| TOPIC_ID_PROJECT0_WALLETS_STATE | 14 | project0.wallets.state STATE |
| TOPIC_ID_PROJECT0_ANALYTICS_JOBS | 15 | project0.analytics.jobs STATE Job-Status und Report-Referenzen; Reports per History-GET |
| TOPIC_ID_PROJECT0_EXECUTION_EVENTS | 20 | project0.execution.events EVENT |
| TOPIC_ID_PROJECT0_CAPTURE_EVENTS | 21 | project0.capture.events EVENT ungefiltert |
| TOPIC_ID_BSC_STATUS | 30 | bsc.status STATE config_version, Gate-Verdikte |
| TOPIC_ID_BSC_LAUNCH_STATE | 31 | bsc.launch.state STATE plan_version |
| TOPIC_ID_BSC_MONITORING_STATE | 32 | bsc.monitoring.state STATE |
| TOPIC_ID_BSC_WALLETS_STATE | 33 | bsc.wallets.state STATE |
| TOPIC_ID_BSC_LIVE_STATE | 34 | bsc.live.state STATE LIVE-Gate, Grant, Positionen (Plan 28.10.) |
| TOPIC_ID_SOLANA_STATUS | 50 | solana.status STATE erst mit qualifiziertem Adapter |










<a name="darkbot_control_v1_commands-proto"></a>
<p align="right"><a href="#top">Top</a></p>

## darkbot/control/v1/commands.proto



<a name="darkbot-control-v1-CommandLookupResponse"></a>

### CommandLookupResponse



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| found | [bool](#bool) |  |  |
| command_id | [string](#string) |  |  |
| phase | [CommandPhase](#darkbot-control-v1-CommandPhase) |  | Dieselbe native Engine-Phase wie in pending_commands/recent_command_results. |
| detail | [string](#string) |  |  |






<a name="darkbot-control-v1-CommandRequest"></a>

### CommandRequest



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| project | [ProjectId](#darkbot-control-v1-ProjectId) |  |  |
| request_id | [string](#string) |  | Idempotency-Key, vom Client erzeugt (UUIDv4), pro Nutzeraktion genau einer; nie wiederverwendet. Zeitlich begrenzte Idempotenz (Wire V1.3 §7), solange das Gateway den Key kennt – mindestens command_retention_ms nach der letzten Aktualisierung seines Receipts, darüber hinaus bis zu 2 × command_ledger_records neuere Receipts: gleicher Key &#43; gleiche Payload =&gt; derselbe Intent (Original-Receipt, keine zweite Ausführung); gleicher Key &#43; andere Payload =&gt; REJECTED. Danach ist der Key vergessen: die Original-Payload ist nie mehr annehmbar (issued_at_unix_ns außerhalb des Fensters); derselbe Key mit anderer Payload gilt dann als neuer Intent. |
| action | [string](#string) |  | Domänenaktion, z. B. &#34;project0.capture.start&#34; – serverseitiger strikter Allowlist-Katalog nach F0-VAR; keine freie RPC-/Shell-/Signer-Durchleitung. |
| resource_id | [string](#string) |  | Zielressource (Bot-ID, Job-ID, Wallet-Alias). Leer, wenn die Aktion keine hat. |
| expected | [ExpectedVersion](#darkbot-control-v1-ExpectedVersion) |  |  |
| params | [bytes](#bytes) |  | Serialisierte typisierte Parameter; Messagetyp ist durch action festgelegt. |
| issued_at_unix_ns | [int64](#int64) |  | Zeit der Nutzeraktion im Browser (Unix-ns), ohne fachliche Bedeutung. Das Gateway nimmt einen neuen Command nur an, wenn der Wert höchstens command_clock_window_ms von seiner Empfangszeit abweicht (Uhrabweichung &#43; Zustelldauer), sonst INVALID_PARAMS ohne Ledger-Wirkung. Damit begrenzt das Feld das Idempotenzfenster; es kann nur ablehnen, nie eine Ausführung bewirken. |






<a name="darkbot-control-v1-CommandResponse"></a>

### CommandResponse



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| status | [CommandStatus](#darkbot-control-v1-CommandStatus) |  |  |
| command_id | [string](#string) |  | Von der Engine vergebene ID des durablen Intents. Nur bei ACCEPTED. |
| request_id | [string](#string) |  | Echo des request_id für Korrelation bei verlorener/verzögerter Antwort. |
| reject_reason | [CommandRejectReason](#darkbot-control-v1-CommandRejectReason) |  |  |
| detail | [string](#string) |  |  |
| persisted | [ExpectedVersion](#darkbot-control-v1-ExpectedVersion) |  | Version der Ressource nach Annahme (für die nächste CAS). Nur bei ACCEPTED. |
| phase | [CommandPhase](#darkbot-control-v1-CommandPhase) |  | Ledger-Phase bei Erzeugung dieser Antwort (PENDING_RECONCILE, solange die Ausführung unentschieden ist); eine Wiederholung liefert die aktuelle Phase. Nur bei ACCEPTED. Nie ein On-Chain- oder wirtschaftliches Ergebnis. |






<a name="darkbot-control-v1-ExpectedVersion"></a>

### ExpectedVersion
Native Compare-and-Swap-Vorbedingung. Die Werte kommen aus der zuletzt applizierten
STATE-Projektion (config_version / profile_version / plan_version), NIE aus WS seq.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| config_version | [uint64](#uint64) |  |  |
| profile_version | [uint64](#uint64) |  |  |
| plan_version | [uint64](#uint64) |  |  |
| wallet_policy_version | [uint64](#uint64) |  |  |








<a name="darkbot-control-v1-CommandPhase"></a>

### CommandPhase
Einmalige gezielte Nachfrage bei verlorenem ACK:
  GET /api/v1/command?project=&lt;id&gt;&amp;request_id=&lt;uuid&gt;   -&gt; CommandLookupResponse
Kein Polling. Wird nur ausgelöst, wenn pending_commands[]/recent_command_results[]
den request_id nach Reconnect nicht enthalten.

| Name | Number | Description |
| ---- | ------ | ----------- |
| COMMAND_PHASE_UNSPECIFIED | 0 |  |
| COMMAND_PHASE_PENDING | 1 |  |
| COMMAND_PHASE_COMPLETED | 2 |  |
| COMMAND_PHASE_FAILED | 3 |  |
| COMMAND_PHASE_UNKNOWN | 4 |  |
| COMMAND_PHASE_PENDING_RECONCILE | 5 |  |



<a name="darkbot-control-v1-CommandRejectReason"></a>

### CommandRejectReason


| Name | Number | Description |
| ---- | ------ | ----------- |
| COMMAND_REJECT_REASON_UNSPECIFIED | 0 |  |
| COMMAND_REJECT_REASON_IDEMPOTENCY_CONFLICT | 1 | Key bekannt, Payload anders |
| COMMAND_REJECT_REASON_VERSION_CONFLICT | 2 | CAS fehlgeschlagen |
| COMMAND_REJECT_REASON_GATE_BLOCKED | 3 | fachliches Gate (FE-06, G0.7, Treasury) nicht freigegeben |
| COMMAND_REJECT_REASON_FORBIDDEN | 4 | Principal/Projekt/Capability |
| COMMAND_REJECT_REASON_INVALID_PARAMS | 5 |  |
| COMMAND_REJECT_REASON_RESOURCE_UNKNOWN | 6 |  |
| COMMAND_REJECT_REASON_ENGINE_UNAVAILABLE | 7 |  |



<a name="darkbot-control-v1-CommandStatus"></a>

### CommandStatus


| Name | Number | Description |
| ---- | ------ | ----------- |
| COMMAND_STATUS_UNSPECIFIED | 0 |  |
| COMMAND_STATUS_ACCEPTED | 1 |  |
| COMMAND_STATUS_REJECTED | 2 |  |
| COMMAND_STATUS_NOT_IMPLEMENTED | 3 | Backend/Capability existiert nicht. Kein Button beweist Ausführbarkeit. |










<a name="darkbot_control_v1_stream-proto"></a>
<p align="right"><a href="#top">Top</a></p>

## darkbot/control/v1/stream.proto



<a name="darkbot-control-v1-CatchupComplete"></a>

### CatchupComplete
Grenze Archiv -&gt; Live. Wird VOR dem ersten Live-Event &gt; through gesendet.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| topic | [TopicId](#darkbot-control-v1-TopicId) |  |  |
| through | [EventCursor](#darkbot-control-v1-EventCursor) |  |  |
| quality | [CatchupQuality](#darkbot-control-v1-CatchupQuality) |  |  |
| reason | [CatchupReason](#darkbot-control-v1-CatchupReason) |  |  |
| evidence_scope | [EvidenceScope](#darkbot-control-v1-EvidenceScope) |  | Anzeige-Scope für den Eventfeed. Explizit typisiert statt freier String. |
| last_proven | [EventCursor](#darkbot-control-v1-EventCursor) |  | Letzter bewiesener Cursor bei INCOMPLETE (für erneutes Subscribe). |






<a name="darkbot-control-v1-ClientCapabilities"></a>

### ClientCapabilities



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| max_state_full_bytes | [uint32](#uint32) |  | Höchste StateFull-Payloadgröße, die der Client puffern will (Startwert 256 KiB). |
| accepts_state_part | [bool](#bool) |  | Immer false in V1. StatePart ist reserviert und nicht implementiert. |






<a name="darkbot-control-v1-ClientMessage"></a>

### ClientMessage



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| hello | [Hello](#darkbot-control-v1-Hello) |  |  |
| subscribe | [Subscribe](#darkbot-control-v1-Subscribe) |  |  |
| unsubscribe | [Unsubscribe](#darkbot-control-v1-Unsubscribe) |  |  |
| pong | [Pong](#darkbot-control-v1-Pong) |  |  |






<a name="darkbot-control-v1-ControlAck"></a>

### ControlAck



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| request_id | [uint32](#uint32) |  |  |
| topic | [TopicId](#darkbot-control-v1-TopicId) |  |  |
| status | [AckStatus](#darkbot-control-v1-AckStatus) |  |  |
| reason | [AckReason](#darkbot-control-v1-AckReason) |  |  |
| detail | [string](#string) |  |  |






<a name="darkbot-control-v1-Error"></a>

### Error



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| topic | [TopicId](#darkbot-control-v1-TopicId) |  | UNSPECIFIED =&gt; Verbindungsebene. |
| request_id | [uint32](#uint32) | optional |  |
| code | [ErrorCode](#darkbot-control-v1-ErrorCode) |  |  |
| retryable | [bool](#bool) |  |  |
| message | [string](#string) |  |  |






<a name="darkbot-control-v1-Event"></a>

### Event
Ordering/Dedupe: (topic, source_id, cursor.journal_epoch, cursor.durable_cursor).
Eine Journalposition repräsentiert genau EIN Event. Andere event_id oder anderer
Payload unter demselben Cursor =&gt; PROOF_CONFLICT, niemals als zweites Event zählen.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| topic | [TopicId](#darkbot-control-v1-TopicId) |  |  |
| cursor | [EventCursor](#darkbot-control-v1-EventCursor) |  |  |
| event_id | [string](#string) |  | Fachliche Ereignis-ID (Signatur, Hash, Receipt-ID). Opak. |
| source_id | [string](#string) |  | Run-/Journal-Quelle, zu der cursor gehört. |
| committed_at_unix_ns | [int64](#int64) |  | Zeitpunkt des durablen Commits in der Engine. |
| evidence_ref | [string](#string) |  | Opake Evidenzreferenz (Journal-Record, Offset, Hash) für Provenienz in der UI. |
| payload | [bytes](#bytes) |  | Serialisiertes Domänenereignis; Typ laut TopicDescriptor.payload_type. |






<a name="darkbot-control-v1-EventCursor"></a>

### EventCursor
Ordnungsschlüssel für EVENT-Topics. Niemals nackter Cursor ohne Epoche.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| journal_epoch | [string](#string) |  | Engine-/Journal-eigene Epoche (Run-ID, Journaldatei, Rollover-Generation). Opak, gleichheitsgeprüft. |
| durable_cursor | [uint64](#uint64) |  | Monoton steigende durable Position innerhalb der Epoche. Engine-definiert (F0-VAR: Typ je Backend prüfen). |






<a name="darkbot-control-v1-Heartbeat"></a>

### Heartbeat



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| heartbeat_id | [uint64](#uint64) |  |  |
| server_time_unix_ns | [int64](#int64) |  |  |
| state_watermarks | [StateWatermark](#darkbot-control-v1-StateWatermark) | repeated |  |






<a name="darkbot-control-v1-Hello"></a>

### Hello



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| protocol_major | [uint32](#uint32) |  |  |
| protocol_minor | [uint32](#uint32) |  |  |
| schema_id | [string](#string) |  | Diagnose-Fingerprint der lokalen Schemas. Bei kompatiblem Minor-Upgrade darf ein anderer Hash allein NICHT zum Reject führen (README §3). |
| capabilities | [ClientCapabilities](#darkbot-control-v1-ClientCapabilities) |  |  |






<a name="darkbot-control-v1-Pong"></a>

### Pong



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| heartbeat_id | [uint64](#uint64) |  |  |






<a name="darkbot-control-v1-ServerLimits"></a>

### ServerLimits



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| max_state_full_bytes | [uint32](#uint32) |  | Startkandidat 262144 |
| heartbeat_interval_ms | [uint32](#uint32) |  | Startkandidat 15000 |
| stale_after_missed_heartbeats | [uint32](#uint32) |  | Startkandidat 2 |
| unsubscribe_grace_ms | [uint32](#uint32) |  | Startkandidat 2000 |
| max_catchup_retries | [uint32](#uint32) |  | Startkandidat 3 |






<a name="darkbot-control-v1-ServerMessage"></a>

### ServerMessage



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| welcome | [Welcome](#darkbot-control-v1-Welcome) |  |  |
| control_ack | [ControlAck](#darkbot-control-v1-ControlAck) |  |  |
| state_full | [StateFull](#darkbot-control-v1-StateFull) |  |  |
| event | [Event](#darkbot-control-v1-Event) |  |  |
| catchup_complete | [CatchupComplete](#darkbot-control-v1-CatchupComplete) |  |  |
| heartbeat | [Heartbeat](#darkbot-control-v1-Heartbeat) |  |  |
| error | [Error](#darkbot-control-v1-Error) |  |  |






<a name="darkbot-control-v1-StateFull"></a>

### StateFull
Client-Apply-Regel (Wire V1.3 §2):
  generation &gt; applied.generation  =&gt; akzeptieren, last_applied_seq := seq
  ENGINE-INVARIANTE: generation ist pro Topic persistent streng monoton über Restarts.
  Gateway prüft Publisher-/Source-Identität; bei fehlender Persistenz F3 BLOCKED.
  Kein Wraparound oder blindes Akzeptieren fremder Generationen.
  generation == applied.generation =&gt; akzeptieren, wenn seq &gt;= last_applied_seq
                                      (&#39;&gt;=&#39; weil ein Quality-Overlay dieselbe (generation, seq) tragen darf)
  generation &lt; applied.generation  =&gt; verwerfen
  Lücken in seq sind normal (Coalescing). Nie als Eventverlust deuten.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| topic | [TopicId](#darkbot-control-v1-TopicId) |  |  |
| generation | [uint64](#uint64) |  |  |
| seq | [uint64](#uint64) |  |  |
| source_timestamp_unix_ns | [int64](#int64) |  | Zeitstempel der Projektion beim Publisher (nicht Gateway-Sendezeit). |
| quality | [StateQuality](#darkbot-control-v1-StateQuality) |  |  |
| source_id | [string](#string) |  | Publisher-Identität (Engine-Instanz/Boot-ID) für Reattach-Prüfung. Opak. |
| payload | [bytes](#bytes) |  | Serialisierte Domänenprojektion; Typ laut TopicDescriptor.payload_type. Größe &lt;= ServerLimits.max_state_full_bytes, sonst Error(FRAME_TOO_LARGE) statt Frame. |






<a name="darkbot-control-v1-StateWatermark"></a>

### StateWatermark
Höchste (generation, seq), die an GENAU DIESEN Client bereits in den geordneten
Sendpfad übergeben wurde. Nicht Publisher-Head. Client vergleicht mit last_applied;
Abweichung =&gt; lokalen Apply-/Decode-Fehler diagnostizieren und Topic STALE setzen;
maximal begrenzte Recovery, kein heißer Resubscribe-Loop.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| topic | [TopicId](#darkbot-control-v1-TopicId) |  |  |
| generation | [uint64](#uint64) |  |  |
| highest_seq_sent | [uint64](#uint64) |  |  |






<a name="darkbot-control-v1-Subscribe"></a>

### Subscribe



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| request_id | [uint32](#uint32) |  | Pro Verbindung monoton; korreliert ControlAck. Nie fachlich. |
| topic | [TopicId](#darkbot-control-v1-TopicId) |  |  |
| cursor | [EventCursor](#darkbot-control-v1-EventCursor) |  | Nur EVENT. Bei STATE gesetzt =&gt; ControlAck(REJECTED, INVALID_CURSOR_FOR_STATE). |
| start_policy | [StartPolicy](#darkbot-control-v1-StartPolicy) |  | Nur EVENT ohne cursor. Fehlt beides =&gt; ControlAck(REJECTED, MISSING_START_POLICY). |






<a name="darkbot-control-v1-Unsubscribe"></a>

### Unsubscribe



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| request_id | [uint32](#uint32) |  |  |
| topic | [TopicId](#darkbot-control-v1-TopicId) |  |  |






<a name="darkbot-control-v1-Welcome"></a>

### Welcome



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| status | [WelcomeStatus](#darkbot-control-v1-WelcomeStatus) |  |  |
| protocol_major | [uint32](#uint32) |  |  |
| protocol_minor | [uint32](#uint32) |  |  |
| schema_id | [string](#string) |  |  |
| connection_id | [string](#string) |  | Eindeutig pro Verbindung; nur für Logging/Diagnose, nie fachlicher Schlüssel. |
| server_time_unix_ns | [int64](#int64) |  |  |
| limits | [ServerLimits](#darkbot-control-v1-ServerLimits) |  |  |
| topics | [TopicDescriptor](#darkbot-control-v1-TopicDescriptor) | repeated | Topics, die DIESE Session abonnieren darf, inkl. Payload-Bindung. |
| reject_reason | [string](#string) |  | Nur bei REJECTED_*. |








<a name="darkbot-control-v1-AckReason"></a>

### AckReason


| Name | Number | Description |
| ---- | ------ | ----------- |
| ACK_REASON_UNSPECIFIED | 0 |  |
| ACK_REASON_UNKNOWN_TOPIC | 1 |  |
| ACK_REASON_FORBIDDEN_TOPIC | 2 |  |
| ACK_REASON_TOPIC_UNAVAILABLE | 3 |  |
| ACK_REASON_INVALID_CURSOR_FOR_STATE | 4 |  |
| ACK_REASON_MISSING_START_POLICY | 5 |  |
| ACK_REASON_EPOCH_UNKNOWN | 6 |  |
| ACK_REASON_CURSOR_BEFORE_RETENTION | 7 |  |
| ACK_REASON_JOURNAL_ROLLOVER | 8 |  |
| ACK_REASON_ALREADY_SUBSCRIBED | 9 |  |
| ACK_REASON_NOT_SUBSCRIBED | 10 |  |



<a name="darkbot-control-v1-AckStatus"></a>

### AckStatus


| Name | Number | Description |
| ---- | ------ | ----------- |
| ACK_STATUS_UNSPECIFIED | 0 |  |
| ACK_STATUS_ACCEPTED | 1 |  |
| ACK_STATUS_REJECTED | 2 |  |
| ACK_STATUS_RESET_REQUIRED | 3 | Nur EVENT: Cursor/Epoche im Source-Scope nicht resume-fähig. Client wählt neuen Subscribe (StartPolicy oder bewiesener Cursor) oder zeigt INCOMPLETE. |



<a name="darkbot-control-v1-CatchupQuality"></a>

### CatchupQuality


| Name | Number | Description |
| ---- | ------ | ----------- |
| CATCHUP_QUALITY_UNSPECIFIED | 0 |  |
| CATCHUP_QUALITY_COMPLETE | 1 | Alle Journal-Events (C,H_final] lückenlos abgedeckt; Auslieferung ist at-least-once und kann Replay-Duplikate enthalten (Client dedupliziert). Upstream durchgehend attached. |
| CATCHUP_QUALITY_INCOMPLETE | 2 | Nachzug nicht beweisbar vollständig. UI bleibt INCOMPLETE, zeigt reason. |



<a name="darkbot-control-v1-CatchupReason"></a>

### CatchupReason


| Name | Number | Description |
| ---- | ------ | ----------- |
| CATCHUP_REASON_UNSPECIFIED | 0 |  |
| CATCHUP_REASON_NONE | 1 |  |
| CATCHUP_REASON_SOURCE_INCOMPLETE | 2 |  |
| CATCHUP_REASON_CATCHUP_LIMIT | 3 |  |
| CATCHUP_REASON_EPOCH_ROLLOVER | 4 |  |
| CATCHUP_REASON_PRUNED | 5 |  |
| CATCHUP_REASON_UPSTREAM_DETACHED | 6 |  |
| CATCHUP_REASON_BUFFER_OVERFLOW | 7 |  |



<a name="darkbot-control-v1-ErrorCode"></a>

### ErrorCode


| Name | Number | Description |
| ---- | ------ | ----------- |
| ERROR_CODE_UNSPECIFIED | 0 |  |
| ERROR_CODE_PROTOCOL_VIOLATION | 1 | fatal für die Verbindung |
| ERROR_CODE_AUTH_REQUIRED | 2 |  |
| ERROR_CODE_SESSION_EXPIRED | 3 |  |
| ERROR_CODE_INTERNAL | 4 |  |
| ERROR_CODE_SCHEMA_INVALID | 10 | fatal für das Topic, stabil (kein Re-Subscribe-Sturm) |
| ERROR_CODE_FRAME_TOO_LARGE | 11 |  |
| ERROR_CODE_TOPIC_UNAVAILABLE | 12 |  |
| ERROR_CODE_EVENT_OVERFLOW | 20 | retryable für das Topic (max. 3 Backoff&#43;Jitter, dann INCOMPLETE/manuell) |
| ERROR_CODE_CATCHUP_LIMIT | 21 |  |
| ERROR_CODE_SOURCE_INCOMPLETE | 22 |  |
| ERROR_CODE_SOURCE_DETACHED | 23 |  |



<a name="darkbot-control-v1-EvidenceScope"></a>

### EvidenceScope


| Name | Number | Description |
| ---- | ------ | ----------- |
| EVIDENCE_SCOPE_UNSPECIFIED | 0 |  |
| EVIDENCE_SCOPE_LIVE_ONLY | 1 |  |
| EVIDENCE_SCOPE_ARCHIVE | 2 |  |
| EVIDENCE_SCOPE_ARCHIVE_PARTIAL | 3 |  |



<a name="darkbot-control-v1-StartPolicy"></a>

### StartPolicy


| Name | Number | Description |
| ---- | ------ | ----------- |
| START_POLICY_UNSPECIFIED | 0 |  |
| START_POLICY_LIVE_FROM_BOUNDARY | 1 | Kein Archiv: Gateway liest H, sendet CatchupComplete(COMPLETE, H, scope=LIVE_ONLY), dann nur &gt; H. |
| START_POLICY_CATCHUP_FROM_AUTHORIZED_START | 2 | Begrenzter, autorisierter Archivstart (Gateway-/Engine-Policy), paginiert, dann Live. |



<a name="darkbot-control-v1-StateQuality"></a>

### StateQuality


| Name | Number | Description |
| ---- | ------ | ----------- |
| STATE_QUALITY_UNSPECIFIED | 0 |  |
| STATE_QUALITY_OK | 1 |  |
| STATE_QUALITY_STALE | 2 | Publisher liefert, aber Source-Freshness laut Projektion abgelaufen. |
| STATE_QUALITY_SOURCE_DETACHED | 3 | Gateway hat die Engine verloren. Wert ist letzter bekannter Stand, NICHT aktuell. Wird sofort an bestehende Subscriber gepusht; neue Subscriber bekommen ihn nie als OK. |
| STATE_QUALITY_NOT_CONNECTED | 4 | Publisher hat seit Gateway-Start noch nichts geliefert. |
| STATE_QUALITY_UNKNOWN | 5 |  |



<a name="darkbot-control-v1-WelcomeStatus"></a>

### WelcomeStatus


| Name | Number | Description |
| ---- | ------ | ----------- |
| WELCOME_STATUS_UNSPECIFIED | 0 |  |
| WELCOME_STATUS_ACCEPTED | 1 |  |
| WELCOME_STATUS_REJECTED_MAJOR | 2 | Major inkompatibel: Client MUSS fail-closed bleiben, keine Topics, kein JSON-Fallback. |
| WELCOME_STATUS_REJECTED_SCHEMA | 3 | wirklich inkompatibler Vertrag/Topic-Bindung, nicht nur neuer Minor-Hash |
| WELCOME_STATUS_REJECTED_AUTH | 4 |  |










<a name="darkbot_control_v1_engine_feed-proto"></a>
<p align="right"><a href="#top">Top</a></p>

## darkbot/control/v1/engine_feed.proto



<a name="darkbot-control-v1-EngineFrame"></a>

### EngineFrame
Engine StateFull must carry the Hello source_id and EXACT generation, seq &gt; 0.
Engine reports StateQuality OK; Gateway owns STALE/DETACHED overlays.
Ein EngineFrame ist höchstens max_state_full_bytes &#43; 4096 Byte Envelope-Reserve groß.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| hello | [EngineHello](#darkbot-control-v1-EngineHello) |  |  |
| state | [StateFull](#darkbot-control-v1-StateFull) |  |  |
| heartbeat | [Heartbeat](#darkbot-control-v1-Heartbeat) |  |  |
| state_too_large | [StateTooLarge](#darkbot-control-v1-StateTooLarge) |  |  |






<a name="darkbot-control-v1-EngineHello"></a>

### EngineHello



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| project | [ProjectId](#darkbot-control-v1-ProjectId) |  |  |
| schema_id | [string](#string) |  |  |
| source_id | [string](#string) |  |  |
| generation | [uint64](#uint64) |  |  |
| topics | [TopicId](#darkbot-control-v1-TopicId) | repeated |  |






<a name="darkbot-control-v1-StateTooLarge"></a>

### StateTooLarge
Ein STATE, dessen Payload ServerLimits.max_state_full_bytes (262144) übersteigt: die Engine
sendet statt des Payloads nur diese Meldung mit derselben Identität (source_id, generation,
seq &gt; 0) – nie abgeschnitten. Das Gateway behandelt sie wie einen zu großen StateFull: nur
dieses Topic wird FRAME_TOO_LARGE, die Quelle und alle anderen Topics bleiben verbunden.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| topic | [TopicId](#darkbot-control-v1-TopicId) |  |  |
| generation | [uint64](#uint64) |  |  |
| seq | [uint64](#uint64) |  |  |
| source_timestamp_unix_ns | [int64](#int64) |  |  |
| source_id | [string](#string) |  |  |
| payload_bytes | [uint64](#uint64) |  | Größe des nicht gesendeten Payloads in Byte. |















<a name="darkbot_control_v1_numeric-proto"></a>
<p align="right"><a href="#top">Top</a></p>

## darkbot/control/v1/numeric.proto



<a name="darkbot-control-v1-SignedDecimal"></a>

### SignedDecimal



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| value | [string](#string) |  |  |






<a name="darkbot-control-v1-UnsignedDecimal"></a>

### UnsignedDecimal



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| value | [string](#string) |  |  |















<a name="darkbot_control_v1_overview-proto"></a>
<p align="right"><a href="#top">Top</a></p>

## darkbot/control/v1/overview.proto



<a name="darkbot-control-v1-FinanceSummary"></a>

### FinanceSummary



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| finance_snapshot_id | [string](#string) |  | Identität des Backend-Snapshots; gleiche ID wie in project0.finance.summary / wallets.state. |
| quality | [FinanceQuality](#darkbot-control-v1-FinanceQuality) |  |  |
| realized_net_pnl_usd_micro_signed | [SignedDecimal](#darkbot-control-v1-SignedDecimal) |  | Vorzeichenbehaftet (Rust i128). Fehlt, wenn quality != FINALIZED. |
| total_capital_usd_micro | [uint64](#uint64) | optional |  |
| as_of_unix_ns | [int64](#int64) |  |  |






<a name="darkbot-control-v1-GlobalFinance"></a>

### GlobalFinance



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| available | [bool](#bool) |  | Nur true bei gleicher Bewertung, Finality, Zeitbasis und ohne Wallet-Doppelzählung. |
| unavailable_reason | [string](#string) |  |  |
| net_pnl_usd_micro_signed | [SignedDecimal](#darkbot-control-v1-SignedDecimal) |  |  |
| total_capital_usd_micro | [uint64](#uint64) | optional |  |






<a name="darkbot-control-v1-LatencySample"></a>

### LatencySample



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| p95_ms | [uint32](#uint32) | optional |  |
| window_s | [uint32](#uint32) |  |  |
| samples | [uint32](#uint32) |  |  |






<a name="darkbot-control-v1-OverviewAggregates"></a>

### OverviewAggregates



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| projects | [ProjectSummary](#darkbot-control-v1-ProjectSummary) | repeated |  |
| global | [GlobalFinance](#darkbot-control-v1-GlobalFinance) |  |  |
| projects_connected | [uint32](#uint32) |  |  |
| computed_at_unix_ns | [int64](#int64) |  |  |






<a name="darkbot-control-v1-ProjectJob"></a>

### ProjectJob
Copy of the project source&#39;s job lifecycle; the source stays the only authority.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| job | [string](#string) |  |  |
| state | [string](#string) |  | Process lifecycle: RUNNING, STOPPING, STOPPED, FAILED or UNKNOWN. |
| launch_id | [string](#string) |  |  |
| result_code | [string](#string) |  | Native verdict or STOPPED/FAILED of this launch; empty while running. |
| controller_available | [bool](#bool) | optional |  |






<a name="darkbot-control-v1-ProjectSummary"></a>

### ProjectSummary



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| project | [ProjectId](#darkbot-control-v1-ProjectId) |  |  |
| label | [string](#string) |  | &#34;Project 0&#34; |
| domain | [string](#string) |  | &#34;Solana / Marginfi&#34; |
| source | [SourceState](#darkbot-control-v1-SourceState) |  |  |
| readiness | [Readiness](#darkbot-control-v1-Readiness) |  |  |
| live_authorized | [bool](#bool) | optional | Nur Anzeige. Autorität bleibt die Engine; das Gateway setzt den Wert nie selbst auf true. Presence distinguishes unknown from explicit false; no UI authority is implied. |
| active_bots | [uint32](#uint32) | optional |  |
| active_jobs | [uint32](#uint32) | optional |  |
| active_captures | [uint32](#uint32) | optional |  |
| warnings | [string](#string) | repeated |  |
| rpc_latency | [LatencySample](#darkbot-control-v1-LatencySample) |  |  |
| finance | [FinanceSummary](#darkbot-control-v1-FinanceSummary) |  |  |
| source_timestamp_unix_ns | [int64](#int64) |  |  |
| source_id | [string](#string) |  |  |
| jobs | [ProjectJob](#darkbot-control-v1-ProjectJob) | repeated | Supervised read-only jobs as the project source reports them (BSC: readiness job). |








<a name="darkbot-control-v1-FinanceQuality"></a>

### FinanceQuality


| Name | Number | Description |
| ---- | ------ | ----------- |
| FINANCE_QUALITY_UNSPECIFIED | 0 |  |
| FINANCE_QUALITY_UNKNOWN | 1 | keine qualifizierte Quelle |
| FINANCE_QUALITY_PARTIAL | 2 | Quelle vorhanden, nicht finalisiert |
| FINANCE_QUALITY_FINALIZED | 3 | finalisierter Backend-Snapshot |



<a name="darkbot-control-v1-Readiness"></a>

### Readiness
Runtime-Readiness ist keine LIVE-Freigabe. READY heißt: Engine läuft und ist erreichbar.

| Name | Number | Description |
| ---- | ------ | ----------- |
| READINESS_UNSPECIFIED | 0 |  |
| READINESS_UNKNOWN | 1 |  |
| READINESS_BLOCKED | 2 |  |
| READINESS_READY | 3 |  |



<a name="darkbot-control-v1-SourceState"></a>

### SourceState


| Name | Number | Description |
| ---- | ------ | ----------- |
| SOURCE_STATE_UNSPECIFIED | 0 |  |
| SOURCE_STATE_NOT_CONNECTED | 1 | Gateway hat die Engine seit Start nie gesehen |
| SOURCE_STATE_CONNECTED | 2 |  |
| SOURCE_STATE_STALE | 3 | Engine liefert, aber Freshness abgelaufen |
| SOURCE_STATE_DETACHED | 4 | Engine war da und ist weg |










<a name="darkbot_project0_v1_analytics-proto"></a>
<p align="right"><a href="#top">Top</a></p>

## darkbot/project0/v1/analytics.proto



<a name="darkbot-project0-v1-AnalyticsJob"></a>

### AnalyticsJob
One analytics job of the controller&#39;s store.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| job_id | [string](#string) |  | The job id (the command&#39;s request id when started from the GUI). |
| kind | [string](#string) |  | BACKTEST, OPTIMIZE, POST_CAPTURE or H0. |
| run | [string](#string) |  | The run as requested (an id under the run roots); empty for H0 (every saved run). |
| state | [string](#string) |  | QUEUED, RUNNING, INTERRUPTED (the worker is gone without an outcome), COMPLETED, FAILED or CANCELLED. |
| code | [string](#string) |  | The refusal code of a failed or cancelled job; empty otherwise. |
| requested_unix_ms | [uint64](#uint64) | optional | Wall clock of the request; unset when it was not readable. |
| started_unix_ms | [uint64](#uint64) | optional | Wall clock the worker started; unset before or when not readable. |
| finished_unix_ms | [uint64](#uint64) | optional | Wall clock of the outcome; unset before or when not readable. |
| max_seconds | [uint64](#uint64) |  | The job&#39;s explicit time budget. |
| max_ram_bytes | [uint64](#uint64) |  | The job&#39;s explicit memory budget. |
| params | [JobParams](#darkbot-project0-v1-JobParams) |  | The parameters (backtest) or the baseline (optimization). |
| grid | [JobGrid](#darkbot-project0-v1-JobGrid) |  | Optimization only: the grid. |
| verification_result | [string](#string) |  | The verification result the job reads; empty: the run&#39;s canonical one. |
| backtest | [BacktestResult](#darkbot-project0-v1-BacktestResult) |  | A completed backtest. |
| optimize | [OptimizeResult](#darkbot-project0-v1-OptimizeResult) |  | A completed optimization. |
| post_capture | [PostCaptureResult](#darkbot-project0-v1-PostCaptureResult) |  | A completed post-capture chain. |
| h0 | [H0Result](#darkbot-project0-v1-H0Result) |  | A completed H0 report. |
| post_capture_spec | [PostCaptureSpec](#darkbot-project0-v1-PostCaptureSpec) |  | Post-capture only: the verification request. |
| h0_spec | [H0Spec](#darkbot-project0-v1-H0Spec) |  | H0 only: the report&#39;s window request. |






<a name="darkbot-project0-v1-AnalyticsJobs"></a>

### AnalyticsJobs
project0.analytics.jobs (topic 15, STATE; B03/U-9, STARTUP §4 B/C/E): the runs of the project a
job can read and its native analytics jobs, as the Rust job controller&#39;s store holds them
(data/analytics-jobs). Its one publisher is the standalone analytics reporter on its own socket
(the gateway&#39;s auxiliary `project0.analytics` feed); a capture or runtime source never declares it. Jobs are started and cancelled through the same controller as
`p0ctl job …`; a counterfactual Shadow result is never realized profit. Amounts are exact:
i128 as SignedDecimal (*_signed), u128 sums as UnsignedDecimal (*_sum).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| published_unix_ms | [uint64](#uint64) |  | Wall clock of this projection. |
| runner_busy | [bool](#bool) |  | Whether a job of the project runs now (one at a time). |
| runs | [AnalyticsRun](#darkbot-project0-v1-AnalyticsRun) | repeated | Newest runs first, at most analytics_runs_listed. |
| jobs | [AnalyticsJob](#darkbot-project0-v1-AnalyticsJob) | repeated | Newest requests first, at most analytics_jobs_listed. |
| store_error | [string](#string) |  | Why the job store was not readable (jobs is then empty); empty when read. |
| runner_error | [string](#string) |  | Why the runner state is unknown (runner_busy is then not a fact); empty when known. |
| boot | [BootFlow](#darkbot-project0-v1-BootFlow) |  | DEC-07: the boot controller&#39;s flow (data/boot, `p0ctl boot …`) — the one with an active step, else the newest — shown on the Capture tab: this always-on feed is the controller&#39;s projection while no capture runs. Unset without a flow. |
| boot_error | [string](#string) |  | DEC-07: why the boot store was not (completely) readable; empty when read. |
| until_stopped | [UntilStopped](#darkbot-project0-v1-UntilStopped) |  | DEC-16: unbounded runs beside the boot flow — the standing autostart approval, the newest raw-capture qualification and the newest unbounded raw capture. |
| providers | [ProviderRegistryView](#darkbot-project0-v1-ProviderRegistryView) |  | DEC-18: the provider registry (data/providers, `p0ctl provider …`) as its store holds it — staged: a change is effective at the next controlled start (the running run&#39;s providers: project0.status EffectiveRecord.providers). Unset when the store is unreadable. |
| providers_error | [string](#string) |  | DEC-18: why the provider registry was not readable (providers is then unset); empty when read. |






<a name="darkbot-project0-v1-AnalyticsRun"></a>

### AnalyticsRun
One run directory under the project&#39;s run roots.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| run_id | [string](#string) |  | Directory name of the run (the id a job request names). |
| root | [string](#string) |  | Run root under the project (data/econ4h, data/runs, data/market, data/shadow). |
| layout | [string](#string) |  | NATIVE_V2 (a job can read it), NATIVE_V1 (no native reader) or HISTORICAL (no native records). |
| capture_complete | [bool](#bool) |  | The capture recorded its completion. |
| qualification | [RunQualification](#darkbot-project0-v1-RunQualification) |  | The newest post-capture qualification (A12/F-03); unset when none is recorded. |
| modified_unix_ms | [uint64](#uint64) | optional | Directory modification time; unset when unreadable. |
| running | [bool](#bool) |  | The runtime owner runs it now. |
| verification | [VerificationMethod](#darkbot-project0-v1-VerificationMethod) | repeated | DEC-08: every chain verification method of the run (its typed verification records): the Capture tab&#39;s state and the Analytics tab&#39;s methods side by side. |






<a name="darkbot-project0-v1-AutostartGrant"></a>

### AutostartGrant
DEC-16: a standing autostart approval (&#34;Dauerfreigabe&#34;) and the binding it was given for.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| digest | [string](#string) |  | The 16-hex digest the operator confirms (over the whole binding). |
| mode | [string](#string) |  | The run: its mode, configuration hash, executables and horizon. |
| config_sha256 | [string](#string) |  | The run&#39;s configuration hash (runtime_config_sha256). |
| binary_sha256 | [string](#string) |  | The executables&#39; identity (p0ctl and the capture). |
| duration | [string](#string) |  | until-stopped:&lt;window&gt;:&lt;runway&gt; as audited. |
| checkpoint_seconds | [uint64](#uint64) |  | The window (checkpoint) in seconds. |
| minimum_runway_seconds | [uint64](#uint64) |  | The minimum runway in seconds. |
| qualification_id | [string](#string) |  | The qualification the run is admitted with. |
| provider_evidence_id | [string](#string) |  | The provider evidence the network audit of every autostart reads with. |
| genesis_hash | [string](#string) |  | The operator&#39;s genesis pin for that network audit. |
| capture_source | [string](#string) |  | The capture source (logs | transactions). |
| rpc_rps | [uint64](#uint64) |  | The provider budget of one window. |
| max_requests | [uint64](#uint64) |  | Requests per window. |
| max_total_mib | [uint64](#uint64) |  | MiB per window. |
| max_runtime_seconds | [uint64](#uint64) |  | The longest an autostarted run runs before it ends in order. |
| expires_unix_s | [uint64](#uint64) |  | The last second the approval permits an autostart. |
| recorded_unix_s | [uint64](#uint64) |  | When it was granted, respectively previewed. |






<a name="darkbot-project0-v1-AutostartGrantCommand"></a>

### AutostartGrantCommand
DEC-16: parameters of project0.autostart.grant — the operator confirms exactly the previewed
binding by its digest.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| confirm_digest | [string](#string) |  | The digest the preview showed. |






<a name="darkbot-project0-v1-AutostartPreviewCommand"></a>

### AutostartPreviewCommand
DEC-16: parameters of project0.autostart.preview — the terms of a standing autostart
approval of an unbounded raw capture; the controller binds them to the run as it would start
now and records the preview with its digest (project0.analytics.jobs, until_stopped.preview).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| qualification | [string](#string) |  | The run&#39;s qualification id (16 hex digits). |
| checkpoint | [string](#string) |  | The window (a duration s/m/h/d) of the qualified horizon. |
| minimum_runway | [string](#string) |  | The minimum runway (a duration). |
| genesis_hash | [string](#string) |  | The operator&#39;s genesis pin for the network audit of every autostart. |
| max_runtime | [string](#string) |  | The longest an autostarted run runs (a duration). |
| valid_for | [string](#string) |  | How long the approval permits autostarts from now (a duration). |
| capture_source | [string](#string) |  | logs | transactions (the configured one). |






<a name="darkbot-project0-v1-BacktestJobCommand"></a>

### BacktestJobCommand
Parameters (CommandRequest.params) of project0.analytics.backtest.start: the options of
`p0ctl job start backtest`, validated again by the native controller. The command&#39;s
request_id becomes the job id.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| run_id | [string](#string) |  | The run id under the project&#39;s run roots (never a path). |
| min_profit_usd | [string](#string) |  | Minimum profit in USD as an exact decimal with at most 6 fraction digits (&#34;1.5&#34;). |
| capital_usd | [uint64](#uint64) | repeated | Capital scenarios in whole USD (positive; at most 32). |
| funding | [string](#string) | repeated | Admitted funding classes; empty: every class. |
| max_seconds | [uint64](#uint64) |  | The job&#39;s explicit time budget. |
| max_ram_bytes | [uint64](#uint64) |  | The job&#39;s explicit memory budget. |
| verification_result | [string](#string) |  | A verification result id to read instead of the run&#39;s canonical one; empty: canonical. |






<a name="darkbot-project0-v1-BacktestResult"></a>

### BacktestResult
A completed backtest (OBSERVED_MARKET and COUNTERFACTUAL_SHADOW).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| result_id | [string](#string) |  | The content-addressed result id (reports/backtests/&lt;id&gt;.p0j of the run). |
| capture_run_id | [string](#string) |  | The capture run the result read. |
| rows | [uint64](#uint64) |  | Verified liquidations joined with the Shadow facts. |
| attempts | [uint64](#uint64) |  | False positives: executable evaluations without a verified liquidation. |
| liquidations | [uint64](#uint64) |  | Verified chain liquidations of the window. |
| counterfactual | [ShadowCounterfactual](#darkbot-project0-v1-ShadowCounterfactual) |  | The counterfactual Shadow executions. |
| realized | [string](#string) |  | REALIZED_ONCHAIN is not part of a capture: realized results come from the FE-04 journals. |
| result_sha256 | [bytes](#bytes) |  | Hash over the parameters, rows, attempts and summary. |






<a name="darkbot-project0-v1-BootFlow"></a>

### BootFlow
DEC-07 (N7): one boot flow — offline preflight, the operator&#39;s network approval, the network
audit, the operator&#39;s start approval and the start; each step runs once (a new attempt is a
new flow). Tokens are the 16-character content hashes an approval names, never keys.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| flow_id | [string](#string) |  | The flow&#39;s id (data/boot/&lt;flow_id&gt;). |
| mode | [string](#string) |  | The mode the start launches: econ4h | shadow. |
| created_unix_ms | [uint64](#uint64) | optional | Wall clock of the offline step; unset when it was not readable. |
| offline_verdict | [string](#string) |  | READY | OFFLINE_READY_NETWORK_NOT_RUN | BLOCKED |
| offline_token | [string](#string) |  | What the network approval names. |
| offline_blocked | [string](#string) | repeated | `id: reason` of every critical offline check that did not pass. |
| ttl_seconds | [uint64](#uint64) |  | The flow&#39;s TTL (registry boot_token_ttl_seconds, 10..=120 s, fixed at its offline step) and the source of that value as the resolution names it (profile:&lt;name&gt; | environment | config_file | cli). |
| ttl_source | [string](#string) |  | Where ttl_seconds came from. |
| network | [BootStep](#darkbot-project0-v1-BootStep) |  | The network audit step. |
| network_verdict | [string](#string) | optional | The network record&#39;s verdict and what the start approval names; unset before the record. |
| network_token | [string](#string) | optional | What the start approval names. |
| network_blocked | [string](#string) | repeated | `id: reason` of every critical network check that did not pass. |
| observed_unix_s | [uint64](#uint64) | optional | The record&#39;s TTL window: its oldest network read and its expiry (unix seconds). |
| expires_unix_s | [uint64](#uint64) | optional | observed_unix_s &#43; ttl_seconds; a start is admitted only before it. |
| start | [BootStep](#darkbot-project0-v1-BootStep) |  | The start step. |
| duration | [string](#string) |  | The run&#39;s --duration the flow audits and starts (fixed at its offline step). |






<a name="darkbot-project0-v1-BootNetworkCommand"></a>

### BootNetworkCommand
Parameters of project0.boot.network: the operator&#39;s approval of exactly this offline token;
the network audit reads Mainnet read-only with the operator&#39;s provider evidence.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| flow_id | [string](#string) |  | The flow whose offline step is approved. |
| approved_token | [string](#string) |  | The flow&#39;s offline token (16 characters) as the operator saw it. |
| genesis_hash | [string](#string) |  | The operator&#39;s genesis pin (an independent source; must equal the checked-in pin). |
| provider_evidence | [string](#string) |  | The record id (16 lowercase hex digits) of the PROVIDER_EVIDENCE imported into the boot controller&#39;s store (`p0ctl boot evidence import`); never a path: the controller resolves it and refuses an unknown one. |
| capture_source | [string](#string) |  | logs | transactions |






<a name="darkbot-project0-v1-BootOfflineCommand"></a>

### BootOfflineCommand
Parameters (CommandRequest.params) of project0.boot.offline: `p0ctl boot offline` — a new
flow; its TTL is fixed here (between runs).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| mode | [string](#string) |  | econ4h | shadow | market (the desktop raw capture) |
| ttl_seconds | [uint64](#uint64) | optional | 10..=120 s; unset: the registry&#39;s value. |
| duration | [string](#string) |  | The run&#39;s --duration (10m | 1h | 5h | 24h): part of the audited configuration. |






<a name="darkbot-project0-v1-BootStartCommand"></a>

### BootStartCommand
Parameters of project0.boot.start: the operator&#39;s approval of exactly this network token.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| flow_id | [string](#string) |  | The flow whose network audit is approved. |
| approved_token | [string](#string) |  | The flow&#39;s network token (16 characters) as the operator saw it. |
| duration | [string](#string) |  | Must equal the flow&#39;s duration (`p0ctl boot start` refuses another one). |
| capture_source | [string](#string) |  | logs | transactions |
| chain_verify | [string](#string) |  | off (the GUI start; a history read online needs the operator&#39;s provider evidence and stays terminal: `p0ctl boot start --chain-verify index|strict|crosscheck --provider-evidence`) |
| qualification | [string](#string) | optional | DEC-16: an unbounded flow&#39;s qualification id (16 hex digits); required for until-stopped. |






<a name="darkbot-project0-v1-BootStep"></a>

### BootStep
One step of a boot flow.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| state | [string](#string) |  | NOT_RUN | QUEUED | RUNNING | INTERRUPTED | COMPLETED | FAILED | CANCELLED |
| code | [string](#string) |  | The refusal code of a failed or cancelled step; empty otherwise. |
| run_id | [string](#string) | optional | The run a completed start launched. |
| started_unix_ms | [uint64](#uint64) | optional | When its worker took the step. |
| finished_unix_ms | [uint64](#uint64) | optional | When the step ended. |






<a name="darkbot-project0-v1-CapitalScenario"></a>

### CapitalScenario
One capital scenario of the counterfactual.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| capital_usd_micro | [uint64](#uint64) |  | The scenario&#39;s capital. |
| wins | [uint64](#uint64) |  | Wins within the capital. |
| win_net_usd_micro_signed | [darkbot.control.v1.SignedDecimal](#darkbot-control-v1-SignedDecimal) |  | Net of the wins. |
| failed_attempts | [uint64](#uint64) |  | Lost races and false positives within the capital. |
| failed_cost_usd_micro_sum | [darkbot.control.v1.UnsignedDecimal](#darkbot-control-v1-UnsignedDecimal) |  | Known cost of the failed attempts. |
| failed_cost_unknown | [uint64](#uint64) |  | Failed attempts whose cost is unknown. |
| over_budget | [uint64](#uint64) |  | Evaluations beyond the capital. |
| net_usd_micro_signed | [darkbot.control.v1.SignedDecimal](#darkbot-control-v1-SignedDecimal) |  | Wins minus failed costs; unset when a failed cost is unknown (never 0). |






<a name="darkbot-project0-v1-FundingSet"></a>

### FundingSet
Admitted funding classes; empty: every class.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| classes | [string](#string) | repeated | Funding class names. |






<a name="darkbot-project0-v1-H0JobCommand"></a>

### H0JobCommand
Parameters (CommandRequest.params) of project0.analytics.h0.start: the options of
`p0ctl job start h0` (every saved run; the operator&#39;s minimum sample is a terminal option).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| window_days | [uint64](#uint64) |  | Days of the window; 0: the registry&#39;s h0_window_days. |
| max_seconds | [uint64](#uint64) |  | The job&#39;s explicit time budget. |
| max_ram_bytes | [uint64](#uint64) |  | The job&#39;s explicit memory budget. |






<a name="darkbot-project0-v1-H0Result"></a>

### H0Result
A completed H0 report&#39;s headline (the full report is the job&#39;s typed H0 journal and render).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| result_id | [string](#string) |  | The result id (the job id; its output is data/h0/&lt;id&gt;/). |
| window_start_unix_s | [int64](#int64) |  | The window&#39;s start as reported (earlier when the minimum sample extended it). |
| window_end_unix_s | [int64](#int64) |  | The window&#39;s end (block time, unix s). |
| liquidations | [uint64](#uint64) |  | Verified liquidations of the window, each once. |
| duplicates | [uint64](#uint64) |  | The same events seen again in another run. |
| runs_verified | [uint64](#uint64) |  | Runs with a verified result. |
| runs_without_result | [uint64](#uint64) |  | Runs without a verified result or unreadable. |
| winners | [uint64](#uint64) |  | Distinct winners of the competitive liquidations. |
| top_winner_share_bps | [uint64](#uint64) | optional | The top winner&#39;s share of the competitive liquidations (bps); unset without any. |
| bands | [uint64](#uint64) |  | Recurring winners with an implied minimum-profit band (an upper bound, never a config). |
| sample_status | [string](#string) |  | SUFFICIENT, SMALL_SAMPLE or MIN_SAMPLE_UNDECIDED (over the competitive liquidations). |
| tags | [string](#string) |  | The tag timeline&#39;s state (BLOCKED until 0.1.12, H0-04). |
| competitive | [uint64](#uint64) | optional | Fields 13–19: unset for a result of the layout before the review (9a41e808), never 0. Permissionless profit opportunities (classic, receivership): the winners&#39; sample. |
| transactions | [uint64](#uint64) | optional | Distinct liquidation transactions. |
| failed_attempts | [uint64](#uint64) | optional | Failed classic or receivership attempts in the window (the competition that lost). |
| covered_seconds | [uint64](#uint64) | optional | Seconds of the window the runs&#39; captures cover. |
| bands_bounded | [uint64](#uint64) | optional | Bands with an upper bound. |
| bands_below_our_min_profit | [uint64](#uint64) | optional | Bands whose bound lies below our minimum profit. |
| tip_accounts | [uint64](#uint64) | optional | Configured Jito tip accounts (0: tips unknown). |






<a name="darkbot-project0-v1-H0Spec"></a>

### H0Spec
H0 (Track H0): the window of a historical liquidation report over every saved run.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| window_days | [uint64](#uint64) |  | Days before the window&#39;s end (the registry&#39;s h0_window_days unless given). |
| min_sample | [uint64](#uint64) | optional | The operator&#39;s minimum sample for the longer window; unset: undecided (no extension). |
| end_unix_s | [int64](#int64) |  | The window&#39;s end (block time, unix s), fixed at the request. |






<a name="darkbot-project0-v1-JobGrid"></a>

### JobGrid
The bounded explicit grid of an optimization.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| min_profit_usd_micro_signed | [darkbot.control.v1.SignedDecimal](#darkbot-control-v1-SignedDecimal) | repeated | Minimum profits of the grid, ascending. |
| funding_sets | [FundingSet](#darkbot-project0-v1-FundingSet) | repeated | Funding sets of the grid. |
| holdout_bps | [uint64](#uint64) |  | Holdout share of the capture window. |
| max_points | [uint64](#uint64) |  | The most grid points the job evaluates. |
| save_profile | [bool](#bool) |  | Save the chosen point as an offline Shadow candidate profile (never a LIVE setting). |






<a name="darkbot-project0-v1-JobParams"></a>

### JobParams
Economic parameters of a backtest point.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| min_profit_usd_micro_signed | [darkbot.control.v1.SignedDecimal](#darkbot-control-v1-SignedDecimal) |  | A known net must exceed it (never negative). |
| capital_usd_micro | [uint64](#uint64) | repeated | Capital scenarios, ascending. |
| funding | [string](#string) | repeated | Admitted funding classes; empty: every class. |






<a name="darkbot-project0-v1-NamedCount"></a>

### NamedCount
A count under a bounded name.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| name | [string](#string) |  | Bounded name (a kind, marker or status code). |
| count | [uint64](#uint64) |  | Occurrences. |






<a name="darkbot-project0-v1-OptimizeJobCommand"></a>

### OptimizeJobCommand
Parameters (CommandRequest.params) of project0.analytics.optimize.start: the options of
`p0ctl job start optimize` (the baseline is a backtest point, the grid spans its capital).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| baseline | [BacktestJobCommand](#darkbot-project0-v1-BacktestJobCommand) |  | The run, budget and baseline point. |
| min_profit_usd_grid | [string](#string) | repeated | Minimum profits of the grid in USD (exact decimals, at most 6 fraction digits). |
| funding_sets | [FundingSet](#darkbot-project0-v1-FundingSet) | repeated | Funding sets of the grid; an empty set: every class. |
| holdout_bps | [uint64](#uint64) |  | Holdout share of the capture window. |
| max_points | [uint64](#uint64) |  | The most grid points the job may evaluate. |
| save_profile | [bool](#bool) |  | Save the chosen point as an offline Shadow candidate profile (never a LIVE setting). |






<a name="darkbot-project0-v1-OptimizeResult"></a>

### OptimizeResult
A completed optimization: the point chosen on the training part and its holdout score.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| result_id | [string](#string) |  | The content-addressed result id (reports/optimize/&lt;id&gt;.p0j of the run). |
| capture_run_id | [string](#string) |  | The capture run the result read. |
| points | [uint64](#uint64) |  | Grid points evaluated. |
| chosen | [JobParams](#darkbot-project0-v1-JobParams) |  | The chosen point; unset: no point has an objective (NO_SAMPLE). |
| chosen_holdout | [OptimizeScore](#darkbot-project0-v1-OptimizeScore) |  | The chosen point&#39;s score on the holdout part. |
| baseline_holdout | [OptimizeScore](#darkbot-project0-v1-OptimizeScore) |  | The baseline&#39;s score on the holdout part. |
| result_sha256 | [bytes](#bytes) |  | Hash over the result. |
| profile | [string](#string) |  | The saved candidate profile&#39;s file name under data/profiles; empty: none saved. |






<a name="darkbot-project0-v1-OptimizeScore"></a>

### OptimizeScore
The score of one point on one part of the window.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| counterfactual | [ShadowCounterfactual](#darkbot-project0-v1-ShadowCounterfactual) |  | The point&#39;s counterfactual on that part. |
| objective_bps_signed | [darkbot.control.v1.SignedDecimal](#darkbot-control-v1-SignedDecimal) |  | Best return on capital over its scenarios; unset when not known (never 0). |






<a name="darkbot-project0-v1-PostCaptureJobCommand"></a>

### PostCaptureJobCommand
Parameters (CommandRequest.params) of project0.capture.verify.start: the options of
`p0ctl job start post-capture` the GUI may give (online strict needs the operator&#39;s provider
evidence and stays terminal-only).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| run_id | [string](#string) |  | The run id under the project&#39;s run roots (never a path). |
| chain_verify | [string](#string) |  | off, or index | strict | crosscheck from the checkpoint only (with offline; online needs the operator&#39;s provider evidence and stays terminal). |
| offline | [bool](#bool) |  | A history read from the verification checkpoint only (no network). |
| resume_verification | [bool](#bool) |  | Resume an incomplete verification. |
| max_seconds | [uint64](#uint64) |  | The verification&#39;s time budget. |
| max_ram_bytes | [uint64](#uint64) |  | The verification&#39;s memory budget. |






<a name="darkbot-project0-v1-PostCaptureResult"></a>

### PostCaptureResult
A completed post-capture chain: each stage&#39;s outcome (the qualification record is the run&#39;s).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| verification_ok | [bool](#bool) |  | off (NOT_RUN recorded) or a CANONICAL_VERIFIED strict result. |
| report_ok | [bool](#bool) |  | The native report completed. |
| verdict | [string](#string) |  | The master verdict; empty: the qualification could not run. |






<a name="darkbot-project0-v1-PostCaptureSpec"></a>

### PostCaptureSpec
The verification request of a post-capture job (A08/F-02).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| chain_verify | [string](#string) |  | off (identities only, no chain read), index, strict or crosscheck (DEC-08). |
| offline | [bool](#bool) |  | A history read from the verification checkpoint only (no network). |
| resume | [bool](#bool) |  | Resume an incomplete verification. |
| provider_evidence | [bool](#bool) |  | An operator provider evidence was given (an online history read; terminal only). |






<a name="darkbot-project0-v1-ProviderCredentialCommand"></a>

### ProviderCredentialCommand
DEC-18: params of `project0.provider.credential.set` and `…credential.rotate`. The gateway
separates the secret before any durable record (its command ledger holds the request without
it) and hands it to the native controller&#39;s standard input only.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| credential | [string](#string) |  | Credential id. |
| secret | [bytes](#bytes) |  | The secret (in transit only). |






<a name="darkbot-project0-v1-ProviderDescriptorCommand"></a>

### ProviderDescriptorCommand
DEC-18: params of `project0.provider.add` and `project0.provider.update` (the registry&#39;s
rules decide: a canonical endpoint without a credential in it, FREE_ONLY, a known kind). An
update changes only the fields it sets.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| id | [string](#string) |  | Provider id. |
| kind | [string](#string) | optional | rpc | wss | geyser | jupiter | jito (add: required). |
| endpoint | [string](#string) | optional | The endpoint (add: required); refused with a credential in it, never echoed. |
| auth | [string](#string) | optional | none | query:NAME | header:NAME | path. |
| credential | [string](#string) | optional | The credential id it authenticates with. |
| priority | [uint32](#uint32) | optional | Priority among its kind. |
| capacity_rps | [uint64](#uint64) | optional | Configured capacity in requests per second. |
| plan | [string](#string) | optional | free | unknown. |
| entitlement | [string](#string) | optional | The plan&#39;s name. |
| disabled | [bool](#bool) |  | Added disabled (add only). |






<a name="darkbot-project0-v1-ProviderPriorityCommand"></a>

### ProviderPriorityCommand
DEC-18: params of `project0.provider.priority`.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| id | [string](#string) |  | Provider id. |
| priority | [uint32](#uint32) |  | The new priority. |






<a name="darkbot-project0-v1-ProviderQualifyCommand"></a>

### ProviderQualifyCommand
DEC-18: params of `project0.provider.qualify`.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| id | [string](#string) |  | Provider id. |
| evidence | [string](#string) |  | The stored provider evidence record id (16 hex), or &#34;-&#34; to clear the qualification. |






<a name="darkbot-project0-v1-ProviderRegistryView"></a>

### ProviderRegistryView
DEC-18: one committed version of the provider registry — secret-free (hosts, credential ids
and revisions; never an endpoint&#39;s path, a key or a keychain account).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| version | [uint64](#uint64) |  | The committed version (0: no provider yet); the expected version of every change (CAS). |
| providers | [RegistryProvider](#darkbot-project0-v1-RegistryProvider) | repeated | Every provider, by kind, priority and id. |
| credentials | [RegistryCredential](#darkbot-project0-v1-RegistryCredential) | repeated | Every credential&#39;s newest revision. |
| restart_rule | [string](#string) |  | When a change takes effect: next_controlled_start (DEC-05). |






<a name="darkbot-project0-v1-ProviderSelectCommand"></a>

### ProviderSelectCommand
DEC-18: params of `project0.provider.select`.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| kind | [string](#string) |  | rpc | wss | jupiter | jito. |
| id | [string](#string) |  | Provider id. |






<a name="darkbot-project0-v1-RegistryCredential"></a>

### RegistryCredential
DEC-18: a credential&#39;s newest revision (the secret is the macOS keychain&#39;s alone).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| id | [string](#string) |  | Credential id. |
| revision | [uint64](#uint64) |  | Its newest revision (a rotation is a new one, never reused). |
| state | [string](#string) |  | active | revoked. |
| origin | [string](#string) |  | input | env:NAME (the one-time .env import). |
| created_unix_s | [uint64](#uint64) |  | When the revision was created. |






<a name="darkbot-project0-v1-RegistryProvider"></a>

### RegistryProvider
DEC-18: one provider of the registry.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| id | [string](#string) |  | Provider id ([a-z0-9_-]{1,32}). |
| kind | [string](#string) |  | rpc | wss | geyser | jupiter | jito. |
| host | [string](#string) |  | The endpoint&#39;s host (its path and scheme stay in the engine). |
| auth | [string](#string) |  | How it authenticates: none | query:NAME | header:NAME | path (a name, never a value). |
| credential | [string](#string) |  | The credential it authenticates with (its id); empty without authentication. |
| priority | [uint32](#uint32) |  | Lower first among the providers of its kind. |
| capacity_rps | [uint64](#uint64) | optional | Configured capacity in requests per second; unset when not configured. |
| plan | [string](#string) |  | free | unknown (FREE_ONLY: paid is refused, unknown never selected). |
| entitlement | [string](#string) |  | The plan&#39;s name as the provider publishes it. |
| qualification | [string](#string) | optional | The provider evidence record that qualified its binding; unset when not qualified. |
| enabled | [bool](#bool) |  | Enabled (a disabled provider is never selected). |
| selected | [bool](#bool) |  | The selected provider of its kind. |
| binding | [string](#string) | optional | Its secret-free binding (hex SHA-256) with the active credential revision; unset when its credential is revoked or unknown. |
| selection_block | [string](#string) | optional | Why it cannot be the selected one of its kind now; unset when it can. |






<a name="darkbot-project0-v1-RunQualification"></a>

### RunQualification
The newest C0-E1 master qualification record of a run.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| verdict | [string](#string) |  | C0_E1_MASTER_PASS, C0_E1_NOT_QUALIFIED, or UNREADABLE (see error). |
| verification | [string](#string) |  | The selected verification result&#39;s status (CANONICAL_VERIFIED, …) or NOT_RUN. |
| failed | [string](#string) | repeated | Every failed master gate, in check order. |
| qualification_id | [string](#string) |  | Identity of the qualification record. |
| error | [string](#string) |  | Why the newest record is unreadable; empty when read. |






<a name="darkbot-project0-v1-ShadowCounterfactual"></a>

### ShadowCounterfactual
Counterfactual Shadow executions: hypothetical, landing UNPROVEN, safety NOT_EVALUATED.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| matched | [uint64](#uint64) |  | Verified liquidations joined with an evaluation. |
| unknown | [uint64](#uint64) |  | Evaluations whose outcome is unknown. |
| funding_excluded | [uint64](#uint64) |  | Evaluations outside the admitted funding classes. |
| below_minimum | [uint64](#uint64) |  | Evaluations below the minimum profit. |
| wins | [uint64](#uint64) |  | Counterfactual wins. |
| lost_races | [uint64](#uint64) |  | Lost races. |
| no_chain_event | [uint64](#uint64) |  | Executable evaluations without a chain event. |
| scenarios | [CapitalScenario](#darkbot-project0-v1-CapitalScenario) | repeated | One entry per capital scenario. |
| landing | [string](#string) |  | Landing status of every counterfactual execution (UNPROVEN). |
| safety | [string](#string) |  | Safety status of every counterfactual execution (NOT_EVALUATED). |






<a name="darkbot-project0-v1-UntilStopped"></a>

### UntilStopped
DEC-16: unbounded runs (RunHorizon::UntilStopped) as the stores hold them. Expiry and
evidence freshness are compared with the viewer&#39;s clock; every native gate decides again at
the start, nothing here admits a run.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| autostart | [AutostartGrant](#darkbot-project0-v1-AutostartGrant) |  | The standing autostart approval in force (p0ctl autostart grant); unset without one. |
| preview | [AutostartGrant](#darkbot-project0-v1-AutostartGrant) |  | The previewed approval awaiting the operator&#39;s confirmation of its digest; unset without one. |
| qualification | [UntilStoppedQualification](#darkbot-project0-v1-UntilStoppedQualification) |  | The newest qualification of the raw capture (p0ctl horizon qualify --mode market). |
| run | [UntilStoppedRun](#darkbot-project0-v1-UntilStoppedRun) |  | The newest unbounded raw capture&#39;s windows; unset without one. |
| errors | [string](#string) | repeated | Why a store was not readable (its part is then unset); empty when read. |






<a name="darkbot-project0-v1-UntilStoppedQualification"></a>

### UntilStoppedQualification
DEC-16: a qualification of an unbounded run and the parts that did not pass.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| id | [string](#string) |  | The record id (16 hex digits). |
| mode | [string](#string) |  | econ4h | market | shadow |
| checkpoint_seconds | [uint64](#uint64) |  | The window (checkpoint) in seconds. |
| minimum_runway_seconds | [uint64](#uint64) |  | The minimum runway in seconds. |
| complete | [bool](#bool) |  | Every part passed (the start still admits it against the run and the clock). |
| open | [string](#string) | repeated | `part: STATE · reason` of every part that did not pass. |
| evidence_id | [string](#string) | optional | The provider evidence record it qualified with. |
| evidence_valid_until_unix_s | [uint64](#uint64) | optional | The last second its provider evidence stays fresh. |
| qualified_at_unix_s | [uint64](#uint64) |  | When it was recorded. |






<a name="darkbot-project0-v1-UntilStoppedRun"></a>

### UntilStoppedRun
DEC-16: an unbounded raw capture&#39;s windows journal as far as it is written.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| run_id | [string](#string) |  | The run directory&#39;s name (data/market/&lt;run_id&gt;). |
| qualification_id | [string](#string) |  | The qualification it was admitted with. |
| started_unix_s | [uint64](#uint64) |  | When its first window opened. |
| checkpoint_seconds | [uint64](#uint64) |  | The window (checkpoint) in seconds. |
| windows | [uint64](#uint64) |  | Windows closed so far. |
| last_verdict | [string](#string) | optional | The last closed window&#39;s verdict (CONTINUE, UNTIL_STOPPED_CRASH_RECOVERED or its HALT code). |
| last_detail | [string](#string) |  | What the last window measured against its gates. |
| last_ended_unix_s | [uint64](#uint64) | optional | When the last window closed. |
| halted | [string](#string) | optional | The verdict that halted the run; unset while every window held. |
| sealed | [bool](#bool) |  | The run ended: its windows journal is sealed. |
| complete | [bool](#bool) |  | Sealed complete: an ordered end (operator stop or maximum runtime) of a run that never halted. |






<a name="darkbot-project0-v1-VerificationMethod"></a>

### VerificationMethod
DEC-08 (ENTSCHEIDUNGEN §8.6): one chain verification method of a run, its spec with the
newest attempt and its newest finished result. Never a promotion: INDEX_VERIFIED &lt;
CROSSCHECK_VERIFIED &lt; CANONICAL_VERIFIED, only STRICT is canonical; an absent count is unknown.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| method | [string](#string) |  | off | index | strict | crosscheck |
| spec_id | [string](#string) |  | The verification spec (content id). |
| attempts | [uint64](#uint64) |  | Attempts of the spec. |
| running | [bool](#bool) |  | The newest attempt has started and not ended. |
| status | [string](#string) |  | RUNNING, NOT_RUN, INDEX_VERIFIED, CROSSCHECK_VERIFIED, CANONICAL_VERIFIED or HISTORICAL_INCOMPLETE. |
| reason | [string](#string) | optional | The newest attempt&#39;s block or incompleteness reason (block_reason). |
| result_id | [string](#string) | optional | The newest finished attempt&#39;s result id (the counts below are its). |
| offline | [bool](#bool) |  | The newest attempt read its checkpoint only. |
| provider_binding | [string](#string) | optional | SHA-256 of the provider endpoint(s) the spec binds (never the endpoint); unset offline. |
| checkpoint_reads | [uint64](#uint64) |  | Typed chain reads held in the checkpoint (a resume reads them without the network). |
| calls | [uint64](#uint64) | optional | Requests of the newest result. |
| bytes | [uint64](#uint64) | optional | Response bytes of the newest result. |
| progress_unit | [string](#string) |  | pages (INDEX, CROSSCHECK: both sources) or slots (STRICT). |
| progress_done | [uint64](#uint64) |  | Progress done. |
| progress_total | [uint64](#uint64) | optional | Progress expected; unset when unknown. |
| qualified | [NamedCount](#darkbot-project0-v1-NamedCount) | repeated | Qualified events per liquidation kind (empty: unknown). |
| observed | [NamedCount](#darkbot-project0-v1-NamedCount) | repeated | Program events per kind including unqualified transactions (empty: unknown). |
| markers | [NamedCount](#darkbot-project0-v1-NamedCount) | repeated | The six markers (empty: unknown). |
| comparison | [NamedCount](#darkbot-project0-v1-NamedCount) | repeated | Detection statuses of the window&#39;s liquidation events against the capture (MATCH, MISSING_*, *_MISMATCH, …): found and missing events and deviations (empty: unknown). |
| disagreements | [uint64](#uint64) | optional | CROSSCHECK: transactions the two sources read differently. |
| unresolved | [uint64](#uint64) | optional | Transactions of the window the provider left unresolved (null or pruned). |
| unknown | [uint64](#uint64) | optional | Transactions of the window that stayed unqualified. |
| transactions | [NamedCount](#darkbot-project0-v1-NamedCount) | repeated | Transaction statuses of the window against the capture (MATCH, SLOT_MISMATCH, MISSING_LIVE) and the capture&#39;s live transactions the index lacks (BOT_ONLY_INDEX_MISSING); empty: unknown. |
| calls_total | [uint64](#uint64) | optional | Requests of every attempt of the spec. |
| bytes_total | [uint64](#uint64) | optional | Response bytes of every attempt of the spec. |















<a name="darkbot_project0_v1_capture-proto"></a>
<p align="right"><a href="#top">Top</a></p>

## darkbot/project0/v1/capture.proto



<a name="darkbot-project0-v1-CaptureState"></a>

### CaptureState
Live state of a Project_0 market capture: STATE topic project0.capture.state. The reporter
sends it when a block changed, at most once per P0_REPORT_MIN_INTERVAL_MS (newest wins).
Each block mirrors one engine structure and stays unset until the engine reported it.
Units are part of the field names.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| run | [RunInfo](#darkbot-project0-v1-RunInfo) |  | Run identity and configuration (market/configuration.rs, market/rows.rs::CaptureStart); once at start. |
| phase | [PhaseStatus](#darkbot-project0-v1-PhaseStatus) |  | Capture lifecycle (market/capture.rs); on every phase change. |
| transport | [TransportStatus](#darkbot-project0-v1-TransportStatus) |  | WebSocket ingress health and last transport event (market/core.rs); on ingress events. |
| market | [MarketCounters](#darkbot-project0-v1-MarketCounters) |  | Every capture counter (market/core.rs::MarketStats::counters); on coverage events and slots. |
| assessment | [MarketAssessment](#darkbot-project0-v1-MarketAssessment) |  | Completeness, live verdict and edge distribution (market/core.rs::ObserverCore); every 2 s and at the end. |
| writer | [WriterLane](#darkbot-project0-v1-WriterLane) |  | Writer lane of the market journal (storage/writer.rs::StorageStatus); every 2 s. |
| provider | [ProviderBudget](#darkbot-project0-v1-ProviderBudget) |  | Process provider budget every RPC of the capture runs through (solana/src/provider.rs); every 2 s. |






<a name="darkbot-project0-v1-Counter"></a>

### Counter
One named engine counter.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| name | [string](#string) |  | Engine counter name (MarketStats::counters); the unit follows the name suffix. |
| value | [uint64](#uint64) |  | Counter value. |






<a name="darkbot-project0-v1-MarketAssessment"></a>

### MarketAssessment
Completeness and live verdict of the capture so far (pre-verification).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| verdict | [string](#string) |  | MARKET_OBSERVER_STREAM_NOT_READY | MARKET_DATA_INCOMPLETE | MARKET_DATA_SOURCE_COVERAGE_UNVERIFIED (never VALID during capture). |
| coverage_complete | [bool](#bool) |  | No gap, overflow, fatal or gap-capacity loss so far. |
| decode_complete | [bool](#bool) |  | No missing logs, decoder error or unqualified/mismatching truncation so far. |
| enrichment_complete | [bool](#bool) |  | Every requested enrichment succeeded. |
| incomplete_reasons | [string](#string) | repeated | Reason codes that make the capture incomplete. |
| truncated_unqualified_reasons | [ReasonCount](#darkbot-project0-v1-ReasonCount) | repeated | Reasons of unqualified truncated logs with counts. |
| gap_records | [uint64](#uint64) |  | Recorded transport gap ranges. |
| truncated_pending | [uint64](#uint64) |  | Truncated-log transactions awaiting post-capture qualification. |
| deferred_liquidations | [uint64](#uint64) |  | Liquidations awaiting deferred processing. |
| distinct_targets | [uint64](#uint64) |  | Distinct liquidated target accounts. |
| distinct_receivers | [uint64](#uint64) |  | Distinct liquidation receivers. |
| gross_edge_total_usd | [double](#double) |  | Summed gross edge of all counted liquidation events. |
| gross_edge_p50_usd | [double](#double) | optional | Median gross edge per event (nearest rank); unset without events. |
| gross_edge_p90_usd | [double](#double) | optional | 90th percentile gross edge per event (nearest rank). |
| gross_edge_p99_usd | [double](#double) | optional | 99th percentile gross edge per event (nearest rank). |
| top_receivers | [ReceiverTotal](#darkbot-project0-v1-ReceiverTotal) | repeated | Up to 10 receivers by liquidation events, then gross edge. |
| deployment_error | [string](#string) | optional | Program deployment check error, if the check failed. |
| assessed_unix_ms | [uint64](#uint64) |  | Wall clock of this assessment. |






<a name="darkbot-project0-v1-MarketCounters"></a>

### MarketCounters
Capture counters.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| observing | [bool](#bool) |  | Whether the observation window is open. |
| stream_ready | [bool](#bool) |  | Whether the stream became ready. |
| counters | [Counter](#darkbot-project0-v1-Counter) | repeated | Every MarketStats counter in engine order; first/last_observed_slot appear once known. |






<a name="darkbot-project0-v1-PhaseStatus"></a>

### PhaseStatus
Capture lifecycle state.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| phase | [CapturePhase](#darkbot-project0-v1-CapturePhase) |  | Current phase. |
| changed_unix_ms | [uint64](#uint64) |  | Wall clock of the last phase change. |
| stream_ready_unix_s | [uint64](#uint64) | optional | StreamReady time; unset before the stream became ready. |
| stop_reason | [string](#string) |  | Why the capture loop ended (TIMER | OPERATOR_STOP | C0_E1_ABORT | STREAM_NOT_READY | INGRESS_THREAD_ENDED | WRITE_ERROR); empty before. |
| drain_complete | [bool](#bool) | optional | Whether the drain reached the end of the channel; unset before finalization. |
| write_error | [string](#string) | optional | Evidence write error, if any. |
| end_result | [string](#string) |  | After ENDED: CAPTURE_COMPLETE, or exactly the error the capture returned; empty before. |






<a name="darkbot-project0-v1-ProviderBudget"></a>

### ProviderBudget
Process-wide provider budget, sampled by the capture at its 2 s display cadence. A local client
ceiling, never a qualification of purchased provider capacity.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| sampled_unix_ms | [uint64](#uint64) |  | Engine wall clock when the capture published the sample. |
| scope | [string](#string) |  | Scope of the account counters (e.g. PROCESS_SOLANA_PROVIDER_ACCOUNT). |
| requests | [uint64](#uint64) |  | Requests admitted so far, all workloads. |
| http_429_count | [uint64](#uint64) |  | HTTP 429 answers so far. |
| in_flight | [uint64](#uint64) |  | Requests holding a permit now. |
| queued | [uint64](#uint64) |  | Requests waiting for a permit now. |
| max_rps | [uint64](#uint64) |  | Configured request ceiling per second. |
| effective_rps | [double](#double) |  | Current adaptive request rate per second. |
| max_concurrency | [uint64](#uint64) |  | Configured concurrent requests. |
| burst | [uint64](#uint64) |  | Configured burst. |
| bytes | [uint64](#uint64) |  | Request and response bytes accounted so far. |
| cooldown_cause | [string](#string) |  | Why the account waits: NONE | LOCAL_BACKOFF | SERVER_RETRY_AFTER | RETRY_AFTER_UNREPRESENTABLE. |
| cooldown_remaining_ms | [uint64](#uint64) | optional | Remaining wait, rounded up; unset when the server delay is unrepresentable. |
| halt_code | [string](#string) | optional | Code with which the budget refuses every admission; unset while it admits. |
| quota_qualified | [bool](#bool) |  | Provider quota qualified by evidence; these local counters never qualify it (false). |






<a name="darkbot-project0-v1-ReasonCount"></a>

### ReasonCount
Count per reason code.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| reason | [string](#string) |  | Bounded reason code. |
| count | [uint64](#uint64) |  | Occurrences. |






<a name="darkbot-project0-v1-ReceiverTotal"></a>

### ReceiverTotal
Aggregate of one liquidation receiver.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| receiver | [string](#string) |  | Receiver account, base58. |
| liquidation_events | [uint64](#uint64) |  | Liquidation events of this receiver. |
| gross_edge_total_usd | [double](#double) |  | Summed gross edge of this receiver. |






<a name="darkbot-project0-v1-RunInfo"></a>

### RunInfo
Run identity and configuration. Endpoints only as scheme://host, never a key or query.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| run_id | [string](#string) |  | Run id: P0_MARKET_RUN_ID or market_&lt;utc&gt;. |
| program_id | [string](#string) |  | Observed program, base58 (P0_PROGRAM_ID). |
| source | [string](#string) |  | Market source: logs | transactions (P0_MARKET_SOURCE). |
| commitment | [string](#string) |  | Commitment: finalized | confirmed (P0_MARKET_COMMITMENT). |
| ws_host | [string](#string) |  | WebSocket endpoint as scheme://host. |
| rpc_host | [string](#string) | optional | RPC endpoint as scheme://host; unset without RPC endpoint. |
| configured_duration_seconds | [uint64](#uint64) |  | Observation window after StreamReady (P0_MARKET_DURATION_SECS). |
| ready_timeout_seconds | [uint64](#uint64) |  | Maximum wait for StreamReady (P0_MARKET_READY_TIMEOUT_SECS). |
| drain_timeout_seconds | [uint64](#uint64) |  | Maximum drain after the capture loop (P0_MARKET_DRAIN_TIMEOUT_SECS). |
| heartbeat_seconds | [uint64](#uint64) |  | WebSocket heartbeat interval (P0_MARKET_HEARTBEAT_SECS). |
| min_sample_liquidations | [uint64](#uint64) |  | Liquidation events below which statistics are an insufficient sample (P0_MARKET_MIN_SAMPLE). |
| ingress_buffer_events | [uint64](#uint64) |  | Ingress channel capacity (P0_INGRESS_BUFFER). |
| run_dir | [string](#string) |  | Output directory of the run. |
| enrich_max_pending | [uint64](#uint64) |  | Post-capture enrichment: maximum pending fetches (P0_MARKET_ENRICH_MAX_PENDING). |
| enrich_max_attempts | [uint64](#uint64) |  | Post-capture enrichment: attempts per transaction (P0_MARKET_ENRICH_MAX_ATTEMPTS). |
| enrich_retry_ms | [uint64](#uint64) |  | Post-capture enrichment: retry delay (P0_MARKET_ENRICH_RETRY_MS). |
| enrich_timeout_ms | [uint64](#uint64) |  | Post-capture enrichment: request timeout (P0_MARKET_ENRICH_TIMEOUT_MS). |
| enrich_phase_max_seconds | [uint64](#uint64) |  | Post-capture enrichment: phase budget (P0_MARKET_ENRICH_PHASE_MAX_SECS). |
| source_coverage_verified | [bool](#bool) |  | Whether full delivery of program transactions is proven for the source (false today). |
| enrichment_available | [bool](#bool) |  | Whether post-capture enrichment can run (transactions source or RPC endpoint). |
| recovery_budget_ms | [uint64](#uint64) | optional | WebSocket outage budget; unset when only attempts bound the recovery. |
| recovery_attempts | [uint32](#uint32) |  | Maximum failed WebSocket attempts before the capture aborts. |
| idle_timeout_ms | [uint64](#uint64) |  | WebSocket idle timeout. |
| healthy_reset_ms | [uint64](#uint64) |  | Healthy period after which the WebSocket backoff resets. |
| readiness | [string](#string) |  | Readiness rule of the stream (ALL_ACKS_AND_NEW_SLOT_PROGRESS). |
| process_started_unix_s | [uint64](#uint64) |  | Process start, wall clock. |






<a name="darkbot-project0-v1-TransportStatus"></a>

### TransportStatus
WebSocket ingress state.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| connected | [bool](#bool) | optional | Whether the ingress WebSocket is connected; unset before the first health report. |
| last_frame_age_ns | [uint64](#uint64) | optional | Age of the last application frame. |
| application_frames | [uint64](#uint64) | optional | Application frames received by the ingress. |
| last_slot | [uint64](#uint64) | optional | Last slot seen by the ingress. |
| last_state | [string](#string) |  | Last transport state event (RECOVERING | RECOVERED | INCOMPLETE_ABORT | ...); empty before the first one. |
| last_code | [string](#string) |  | Code of the last transport state event (e.g. WSS_HTTP_429). |
| last_attempt | [uint32](#uint32) | optional | Attempt number of the last transport state event. |
| last_valid_slot | [uint64](#uint64) | optional | Last valid slot reported with the last transport state event. |
| last_state_unix_ms | [uint64](#uint64) | optional | Wall clock of the last transport state event. |
| reconnects | [uint32](#uint32) | optional | Reconnects of the ingress before its current session (A01); unset before the first health report. |






<a name="darkbot-project0-v1-WriterLane"></a>

### WriterLane
Writer lane of the market journal, sampled by the capture at its 2 s display cadence.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| sampled_unix_ms | [uint64](#uint64) |  | Engine wall clock when the capture published the sample. |
| queue_bytes | [uint64](#uint64) |  | Bytes admitted but not yet written. |
| queue_peak_bytes | [uint64](#uint64) |  | Highest queue so far. |
| admission_limit_bytes | [uint64](#uint64) |  | Current admission limit of the memory budget. |
| max_bytes | [uint64](#uint64) |  | Hard maximum of the memory budget. |
| free_budget_bytes | [uint64](#uint64) |  | Budget left below the maximum. |
| ingress_bytes_per_s | [double](#double) |  | Admitted bytes per second since the previous sample. |
| write_bytes_per_s | [double](#double) |  | Written bytes per second since the previous sample. |
| lag_frames | [uint64](#uint64) |  | Frames admitted but not yet written. |
| budget_exhaustion_s | [double](#double) | optional | Seconds until the budget is exhausted at current rates; unset when not draining slower than arriving. |
| disk_free_bytes | [uint64](#uint64) | optional | Free space on the journal filesystem; unset when the query failed. |
| disk_reserve_bytes | [uint64](#uint64) |  | Disk space that must remain free. |
| disk_exhaustion_s | [double](#double) | optional | Seconds until the disk reserve is reached at the current write rate; unset without writes. |








<a name="darkbot-project0-v1-CapturePhase"></a>

### CapturePhase
Capture lifecycle phase.

| Name | Number | Description |
| ---- | ------ | ----------- |
| CAPTURE_PHASE_UNSPECIFIED | 0 | Not reported. |
| CAPTURE_PHASE_WAITING_FOR_STREAM | 1 | Started; waiting for StreamReady. |
| CAPTURE_PHASE_CAPTURE | 2 | Observation window open. |
| CAPTURE_PHASE_DRAIN | 3 | Capture loop ended; ingress stopped, channel draining. |
| CAPTURE_PHASE_FINALIZE | 4 | Drain done; finalization, summary and report running. |
| CAPTURE_PHASE_ENDED | 5 | Finished; see end_result. |










<a name="darkbot_project0_v1_owner-proto"></a>
<p align="right"><a href="#top">Top</a></p>

## darkbot/project0/v1/owner.proto



<a name="darkbot-project0-v1-AssetAmount"></a>

### AssetAmount
One amount of one asset.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| asset | [string](#string) |  | &#34;SOL&#34; or the mint, base58. |
| units | [uint64](#uint64) |  | Amount in base units (lamports for SOL). |






<a name="darkbot-project0-v1-Capability"></a>

### Capability
One switchable capability (P0-PROD-CTRL-03).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| name | [string](#string) |  | LIVE | SUBMISSION | JITO_BUNDLE | V1_MESSAGES | JIT_EXACT_IN | INVENTORY_REBALANCING | THRESHOLD_TRIGGER. |
| requested | [string](#string) |  | What is configured: OFF | OBSERVE | ACTIVE. |
| effective | [string](#string) |  | What runs: OFF | OBSERVE | ACTIVE | BLOCKED | NOT_AUTHORIZED | UNSUPPORTED. |
| block_reason | [string](#string) |  | Why effective differs from requested (gate, missing qualification), else empty. |
| roles | [string](#string) | repeated | P8-CTRL.1: the validated roles of a WALLET:&lt;pubkey&gt; signer (LIQUIDATOR, FEE_PAYER). |






<a name="darkbot-project0-v1-CapabilityControlCommand"></a>

### CapabilityControlCommand
DEC-13: parameters (CommandRequest.params) of project0.capability.set: the operands of
`p0ctl capability set CAPABILITY REQUESTED`; the running owner decides what is effective
(LIVE stays BLOCKED until a separate FE-06 authorization).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| capability | [string](#string) |  | LIVE, HOLD_B, JITO_BUNDLE, THRESHOLD_TRIGGER, ECON_SIGN … ECON_LIVE, WALLET:&lt;pubkey&gt;, FE06_GRANT or FE06_REVOKE. |
| requested | [string](#string) |  | One of the capability&#39;s values (a scope digest for FE06_GRANT/FE06_REVOKE). |
| owner_epoch | [uint64](#uint64) |  | P8-CTRL.6: the owner epoch the view showed (LiquidatorState.capability_epoch). |






<a name="darkbot-project0-v1-DataSource"></a>

### DataSource
One controlled data source (the owner&#39;s arbiter: at most one canonical unit).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| unit | [string](#string) |  | wss, geyser:&lt;provider&gt;, … |
| role | [string](#string) |  | AUTHORITATIVE or PRE_EXECUTION_SENSOR. |
| mode | [string](#string) |  | The applied mode: off | observe | active. |
| qualification | [string](#string) |  | NOT_RUN | LOCAL_PASS | QUALIFIED:&lt;evidence&gt; | BLOCKED:&lt;reason&gt;. |
| generation | [uint64](#uint64) |  | 1 at the start, &#43;1 per applied change. |
| canonical | [bool](#bool) |  | The unit feeds canonical state now. |






<a name="darkbot-project0-v1-Fe04Admission"></a>

### Fe04Admission
DEC-13b/DEC-02: the owner&#39;s FE-04 admission of LIVE candidates and exits in this run
(state_owner/fe04.rs). It never opens LIVE itself: that needs the separate FE-06 authorization.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| state | [string](#string) |  | OFF (LIVE not effective) | WAITING (no LIVE candidate yet) | READY | BLOCKED. |
| block_reason | [string](#string) |  | Why no admission is open (OFF: the LIVE block reason); empty when READY. |
| submitted | [uint64](#uint64) |  | Attempts handed off and submitted in this run. |
| refused | [uint64](#uint64) |  | Attempts refused before anything durable. |
| unknown | [uint64](#uint64) |  | Attempts whose handoff is UNKNOWN (every hold kept, Safety halted). |
| last_refusal | [string](#string) |  | The last refusal reason, else empty. |
| simulation_reused | [uint64](#uint64) |  | DEC-02: pipeline simulations reused at the handoff. |
| simulation_resimulated | [uint64](#uint64) |  | DEC-02: payloads simulated again at the handoff. |
| simulation_refused | [uint64](#uint64) |  | DEC-02: handoffs refused without a reusable simulation and without time for a new one. |
| last_reuse_refusal | [string](#string) |  | DEC-02: why the last reuse was refused, else empty. |
| exits_submitted | [uint64](#uint64) |  | Exits of the open inventory handed off in this run. |
| exits_refused | [uint64](#uint64) |  | Exits refused before anything durable. |
| exits_unknown | [uint64](#uint64) |  | Exits whose handoff is UNKNOWN. |
| last_exit_refusal | [string](#string) |  | The last exit refusal or UNKNOWN reason, else empty. |






<a name="darkbot-project0-v1-Fe04Approval"></a>

### Fe04Approval
DEC-10: secret-free status of the FE-04 approval file (native key = value) the runtime is
configured with, bound to its wallet, program, observed ProgramData, clean HEAD, binaries and
Safety ceilings, single use checked against its FE-04 journals.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| state | [string](#string) |  | NONE | VALID | EXPIRED | CONSUMED | BLOCKED | INVALID | BINDING_MISMATCH. |
| reason | [string](#string) |  | Refusal code or the mismatching bindings; empty when VALID or NONE. |
| kind | [string](#string) |  | attempt | exit; empty without an approval. |
| approval_id | [string](#string) |  | Approval id (spent once). |
| funding_class | [string](#string) |  | Funding class of an attempt approval. |
| expires_unix_s | [uint64](#uint64) |  | Expiry, Unix seconds. |
| max_capital_usd_micro | [uint64](#uint64) |  | Attempt approval: capital bound, USD-micro. |
| max_loss_usd_micro | [uint64](#uint64) |  | Attempt approval: loss bound, USD-micro. |
| min_net_usd_micro | [uint64](#uint64) |  | Attempt approval: minimum conservative net, USD-micro. |
| max_slippage_bps | [uint64](#uint64) |  | Slippage bound (both kinds). |
| allowed_targets | [uint64](#uint64) |  | Number of allowed targets of an attempt approval. |






<a name="darkbot-project0-v1-Fe06Preview"></a>

### Fe06Preview
One previewed FE-06 scope: its digest and exactly what a grant of it permits.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| digest | [string](#string) |  | SHA-256 of the scope&#39;s canonical bytes (64 hex digits). |
| scope_mode | [string](#string) |  | ALL | EXIT_ONLY | SIM_ONLY. |
| owner_epoch | [uint64](#uint64) |  | The owner epoch the scope binds. |
| valid_until_unix_s | [uint64](#uint64) |  | The first second after the scope&#39;s validity. |
| terminal | [string](#string) |  | The native full view the operator confirms (`p0ctl live scope`). |






<a name="darkbot-project0-v1-Fe06State"></a>

### Fe06State
P8-CTRL.3/.5: the owner&#39;s FE-06 state. A grant is a permission, never a command; its
operator authentication is a separate gate (block_reason while no grant can be taken).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| grant_digest | [string](#string) | optional | The digest of the grant in force; unset without one. |
| scope_mode | [string](#string) |  | ALL | EXIT_ONLY | SIM_ONLY of the grant in force; empty without one. |
| valid_from_unix_s | [uint64](#uint64) | optional | The first second of the grant&#39;s validity; unset without a grant. |
| valid_until_unix_s | [uint64](#uint64) | optional | The first second after the grant&#39;s validity; unset without a grant. |
| in_force | [bool](#bool) |  | In force now (its validity holds and the owner&#39;s clock is readable). |
| campaign_id | [string](#string) |  | The grant&#39;s campaign. |
| max_attempts_per_utc_day | [uint64](#uint64) |  | The scope&#39;s new attempts per UTC day. |
| max_attempts_campaign | [uint64](#uint64) |  | The scope&#39;s new attempts per campaign. |
| max_attempts_grant | [uint64](#uint64) |  | The scope&#39;s new attempts per grant. |
| attempts_today | [uint64](#uint64) | optional | Attempts booked this UTC day; unset while the FE-04 journals are unreadable (never 0). |
| attempts_campaign | [uint64](#uint64) | optional | Attempts booked against the campaign; unset while the journals are unreadable. |
| attempts_grant | [uint64](#uint64) | optional | Attempts booked against the grant; unset while the journals are unreadable. |
| block_reason | [string](#string) |  | Why no grant is in force or can be taken (the first gate), else empty. |
| previews | [Fe06Preview](#darkbot-project0-v1-Fe06Preview) | repeated | The scopes previewed for this owner epoch (`p0ctl live scope`), each grantable by its digest. |
| max_exits_grant | [uint64](#uint64) |  | P8-CTRL.5: the scope&#39;s exits per grant (0 without a grant). |
| exits_grant | [uint64](#uint64) | optional | Exits booked against the grant; unset while the journals are unreadable (never 0). |






<a name="darkbot-project0-v1-FinanceSummary"></a>

### FinanceSummary
Finance summary of the liquidator: STATE topic project0.finance.summary (same
finance_snapshot_id as project0.wallets.state). Unknown values stay unset.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| finance_snapshot_id | [string](#string) |  | Same id as the WalletsState of this state. |
| as_of_slot | [uint64](#uint64) |  | Newest slot of the state the summary was taken at. |
| total_capital_usd_micro | [int64](#int64) | optional | Needs reconciled wallet balances; unset until they exist. |
| available_capital_usd_micro | [int64](#int64) | optional | Needs reconciled wallet balances; unset until they exist. |
| reserved_capital_usd_micro | [int64](#int64) | optional | Capital reserved by in-flight attempts. |
| realized_net_pnl_usd_micro | [int64](#int64) | optional | Realized net of the settled steps (FE-04 accounting). |
| withdrawable_profit_usd_micro | [int64](#int64) | optional | Withdrawable realized profit; unset unless every input is known. |
| transferred_to_treasury_usd_micro | [int64](#int64) | optional | No transfer path exists: 0 once the journal was read. |
| status | [string](#string) |  | VERIFIED (every field known) | PARTIAL (journal read, a field unknown) | UNKNOWN (no journal read). |






<a name="darkbot-project0-v1-HoldAttempt"></a>

### HoldAttempt
DEC-04: one attempt&#39;s holds: A (wallet reservation, inventory hold) and B against the
finalized flows.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| mode | [string](#string) |  | The mode it was admitted in: OFF | OBSERVE | ACTIVE. |
| a_wallet | [AssetAmount](#darkbot-project0-v1-AssetAmount) | repeated | A&#39;s wallet reservation per asset (&#34;SOL&#34; lamports or mint units). |
| a_inventory_usd_micro | [uint64](#uint64) |  | A&#39;s inventory hold: the conservative value of the whole seizure, USD micro. |
| b_class | [string](#string) |  | EXACT | UNQUALIFIED. |
| b_reason | [string](#string) |  | Why B is unqualified (A applies); empty when exact. |
| b_kind | [string](#string) |  | PLAIN | KAMINO | DRIFT | JUPLEND; empty when unqualified. |
| b_inventory_usd_micro | [uint64](#uint64) | optional | B&#39;s inventory hold, USD micro; unset when unqualified. |
| flows | [HoldFlow](#darkbot-project0-v1-HoldFlow) | repeated | B per asset against the finalized delta: every asset of B or of the deltas. |
| reserved_inventory_usd_micro | [uint64](#uint64) |  | The inventory hold Safety reserved for the attempt, USD micro (A&#39;s unless an effective ACTIVE qualified B). |
| a_flows | [HoldFlow](#darkbot-project0-v1-HoldFlow) | repeated | A per asset against the finalized delta: every asset of A&#39;s wallet reservation or of the deltas (A keeps nothing in the wallet; it holds the seizure in the inventory limit). |
| settled | [bool](#bool) |  | The attempt is settled: the finalized deltas and inventory mark are its own. |
| actual_inventory_usd_micro | [sint64](#sint64) | optional | The finalized open-inventory mark, USD micro; unset until settled (or beyond int64). |






<a name="darkbot-project0-v1-HoldB"></a>

### HoldB
DEC-04 (V7): the B hold mode — requested and effective (ACTIVE only with the user&#39;s N and the
journals&#39; evidence) — the evidence, Safety&#39;s reserved inventory and capital, and the held
attempt&#39;s holds.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| requested | [string](#string) |  | OFF | OBSERVE | ACTIVE. |
| effective | [string](#string) |  | OFF | OBSERVE | ACTIVE. |
| block_reason | [string](#string) |  | Why the effective mode is not the requested one; empty when it is. |
| n | [uint64](#uint64) | optional | The user&#39;s N (B-8); unset while undecided. |
| evidence_exact | [uint64](#uint64) |  | Landed, settled attempts with an exact B and no underestimation. |
| evidence_underestimated | [uint64](#uint64) |  | Landed, settled attempts whose B underestimated a flow. |
| evidence_unqualified | [uint64](#uint64) |  | Landed, settled attempts without an exact B. |
| inventory_reserved_usd_micro | [uint64](#uint64) |  | Inventory holds reserved by unsettled attempts, USD micro. |
| capital_in_use_usd_micro | [uint64](#uint64) |  | Capital in use by unsettled attempts, USD micro. |
| attempt | [HoldAttempt](#darkbot-project0-v1-HoldAttempt) |  | The held attempt&#39;s holds; unset without one (or one admitted in OFF). |






<a name="darkbot-project0-v1-HoldFlow"></a>

### HoldFlow
DEC-04: one asset of B: what the wallet holds before the attempt and keeps at most after it,
the finalized delta and whether B underestimated it.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| asset | [string](#string) |  | &#34;SOL&#34; or the mint, base58. |
| reserve | [uint64](#uint64) |  | Base units held before the attempt. |
| residual_max | [uint64](#uint64) |  | Base units kept at most after it. |
| actual | [sint64](#sint64) | optional | The finalized wallet delta; unset until settled (or beyond int64). |
| underestimated | [bool](#bool) |  | The delta is a debit beyond the reserve or an inflow beyond the residual. |
| residual_unbounded | [bool](#bool) |  | No bound in units (a swap&#39;s output; its value is bounded in B&#39;s inventory hold); residual_max is then 0. |






<a name="darkbot-project0-v1-LatencyNs"></a>

### LatencyNs
A latency distribution in ns; percentiles unset without a sample (never 0).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| count | [uint64](#uint64) |  | Samples. |
| p50 | [uint64](#uint64) | optional | Median (conservative bucket bound). |
| p95 | [uint64](#uint64) | optional | 95th percentile. |
| p99 | [uint64](#uint64) | optional | 99th percentile. |
| max | [uint64](#uint64) | optional | Largest sample. |






<a name="darkbot-project0-v1-LiquidatorState"></a>

### LiquidatorState
Live state of the Shadow/econ4h state owner: STATE topic project0.liquidator.state
(orchestration/state_owner, status_tap::OwnerSnapshot). Sent at most every owner_status_ms.
Values are what the owner holds; an unknown value stays unset, never 0.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| run | [OwnerRun](#darkbot-project0-v1-OwnerRun) |  | Run identity of the owner. |
| engine | [OwnerEngine](#darkbot-project0-v1-OwnerEngine) |  | Canonical state, ingress and recovery (solana/src/state, ingress session). |
| trigger | [OwnerTrigger](#darkbot-project0-v1-OwnerTrigger) |  | Candidate trigger and its lanes (state_owner/candidates.rs). |
| pipeline | [OwnerPipeline](#darkbot-project0-v1-OwnerPipeline) |  | Pipeline evaluations and decisions of this run (analytics/shadow_ledger.rs, MT0). |
| safety | [OwnerSafety](#darkbot-project0-v1-OwnerSafety) |  | Safety layer: limits, capital in use, halts (liquidation/src/safety.rs). |
| capabilities | [Capability](#darkbot-project0-v1-Capability) | repeated | Every switchable capability: requested, effective and why it is blocked. |
| rebalance | [RebalancePlan](#darkbot-project0-v1-RebalancePlan) |  | B01: the rebalancing observation of the inventory bands; unset while OFF. |
| sources | [DataSource](#darkbot-project0-v1-DataSource) | repeated | S1-02: every controlled data source of the owner with its applied control. |
| last_source_decision | [SourceDecision](#darkbot-project0-v1-SourceDecision) |  | The owner&#39;s last source-control decision; unset before the first one. |
| source_stats | [SourceStats](#darkbot-project0-v1-SourceStats) |  | Source counters of this run. |
| source_config_version | [uint64](#uint64) | optional | CTRL-03: the source configuration version the owner has decided up to — the ExpectedVersion.config_version a source change states; unset without source control. |
| approval | [Fe04Approval](#darkbot-project0-v1-Fe04Approval) |  | DEC-10: the configured FE-04 approval, secret-free; it never authorizes LIVE (FE-06). |
| capability_config_version | [uint64](#uint64) | optional | DEC-13: the capability configuration version the owner has decided up to — the ExpectedVersion.config_version a capability change states; unset without capability control. |
| last_capability_decision | [SourceDecision](#darkbot-project0-v1-SourceDecision) |  | DEC-13: the owner&#39;s last capability-control decision; unset before the first one. |
| admission | [Fe04Admission](#darkbot-project0-v1-Fe04Admission) |  | DEC-13b/DEC-02: the owner&#39;s FE-04 admission of LIVE candidates and exits. |
| capability_epoch | [uint64](#uint64) | optional | P8-CTRL.6: this owner start&#39;s capability epoch — the epoch a capability change states (CapabilityControlCommand.owner_epoch); unset without capability control. |
| fe06 | [Fe06State](#darkbot-project0-v1-Fe06State) |  | P8-CTRL.3/.5: the FE-06 grant, its scope&#39;s limits and use, and the previewed scopes. |






<a name="darkbot-project0-v1-LossCount"></a>

### LossCount
One loss reason with its count.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| reason | [string](#string) |  | Loss reason (record name). |
| count | [uint64](#uint64) |  | Count of the day. |






<a name="darkbot-project0-v1-NonLanding"></a>

### NonLanding
DEC-03 (N-2): the non-landing status of one FE-04 subject — exactly one result class, the
latest read&#39;s B sources and C result, the hold duration and the read&#39;s calls and bytes. No
switch and no manual resolution.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| subject | [string](#string) |  | ATTEMPT | EXIT. |
| result_class | [string](#string) |  | B_C_CONFIRMED | C_ALONE_QUALIFIED | VETO_LANDING_CONFIRMED | VETO_CONTRADICTION_OPEN | UNKNOWN_INCOMPLETE; NONE before the handoff or for a release before N-2. |
| alarm | [bool](#bool) |  | An open contradiction (a sighting without a confirmed landing): holds stay, read again. |
| sources | [NonLandingSource](#darkbot-project0-v1-NonLandingSource) | repeated | The latest read&#39;s B sources. |
| c_result | [string](#string) |  | The latest read&#39;s C result: NEGATIVE | SIGHTING | C_&lt;reason&gt;; empty without a read. |
| hold_ms | [uint64](#uint64) | optional | How long the hold has been held, milliseconds; unset without a hold. |
| calls | [uint64](#uint64) | optional | Calls of the latest read; unset without a read in this run. |
| bytes | [uint64](#uint64) | optional | Request and response bytes of the latest read; unset without a read in this run. |
| reads | [uint64](#uint64) |  | Reads of this subject in this run. |






<a name="darkbot-project0-v1-NonLandingSource"></a>

### NonLandingSource
One B source of a non-landing read.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| operator | [string](#string) |  | Operator label of the source. |
| class | [string](#string) |  | USABLE | UNUSABLE_&lt;reason&gt; | SIGHTING. |






<a name="darkbot-project0-v1-OwnerEngine"></a>

### OwnerEngine
Canonical state, ingress and recovery.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| run_state | [string](#string) |  | BOOTSTRAPPING | RECONCILING | READY | HALTED. |
| halt_reason | [string](#string) |  | Halt reason while HALTED (e.g. FeedGap), else empty. |
| ingress_generation | [uint64](#uint64) |  | Current ingress session generation. |
| reconnects | [uint64](#uint64) |  | Reconnected events applied to the state in this run (A01: counted, never a placeholder). |
| gap_open | [bool](#bool) |  | A feed gap is open (no new stream generation yet). |
| highest_slot | [uint64](#uint64) |  | Newest slot seen by the state. |
| ingress_halt_raised | [bool](#bool) |  | The out-of-band ingress halt is raised. |
| program_upgrade_latched | [bool](#bool) |  | A ProgramData change halted the owner. |
| counters | [StateCounters](#darkbot-project0-v1-StateCounters) |  | State write counters. |
| program_pin | [ProgramPin](#darkbot-project0-v1-ProgramPin) |  | The startup ProgramData verdict against the qualified deployment (absent before it is read). |






<a name="darkbot-project0-v1-OwnerPipeline"></a>

### OwnerPipeline
Pipeline evaluations and decisions of the current day (Shadow ledger) and this process (MT0).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| evaluations | [uint64](#uint64) |  | Pipeline evaluations of this process (MT0). |
| health_checked | [uint64](#uint64) |  | Exact Health evaluations of the day. |
| liquidatable | [uint64](#uint64) |  | Accounts found liquidatable on the day. |
| plans_built | [uint64](#uint64) |  | Plans built / rejected on the day. |
| plans_rejected | [uint64](#uint64) |  | Plans rejected on the day. |
| simulations_passed | [uint64](#uint64) |  | Simulations passed on the day. |
| simulations_failed | [uint64](#uint64) |  | Simulations failed on the day. |
| shadow_ready | [uint64](#uint64) |  | Shadow-ready decisions on the day. |
| shadow_wins | [uint64](#uint64) |  | Shadow wins against the observed winner on the day. |
| losses | [LossCount](#darkbot-project0-v1-LossCount) | repeated | Loss reasons of the day. |
| decision_p95_ns | [uint64](#uint64) | optional | p95 of T_rx→T_ready (decision ready: economics and the built plan) of the day; unset without a sample. |
| fee_quote_p95_ns | [uint64](#uint64) | optional | p95 of the fee-quote RPC of this process; unset without a sample. |
| simulation_p95_ns | [uint64](#uint64) | optional | p95 of the simulation RPC of this process; unset without a sample. |






<a name="darkbot-project0-v1-OwnerRun"></a>

### OwnerRun
Run identity of the owner.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| run_id | [string](#string) |  | Run id (p0ctl run id, or &#34;-&#34; for an unnamed local run). |
| mode | [string](#string) |  | Owner mode: shadow | econ4h. |
| published_unix_ms | [uint64](#uint64) |  | Wall clock of this publication. |
| wallet | [string](#string) |  | Liquidator wallet, base58 (public key only). |
| program_id | [string](#string) |  | Marginfi program id, base58. |
| end_result | [string](#string) |  | Empty while the owner runs; at its end RUN_COMPLETE or the terminal code. |






<a name="darkbot-project0-v1-OwnerSafety"></a>

### OwnerSafety
Safety layer.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| capital_in_use_usd_micro | [uint64](#uint64) |  | Capital reserved by in-flight attempts, USD micro. |
| max_total_capital_usd_micro | [uint64](#uint64) |  | Configured limits. |
| max_daily_loss_usd_micro | [uint64](#uint64) |  | Configured maximum daily loss, USD micro. |
| max_exposure_per_attempt_usd_micro | [uint64](#uint64) |  | Configured maximum exposure per attempt, USD micro. |
| max_concurrent_positions | [uint32](#uint32) |  | Configured maximum concurrent positions. |
| max_outstanding_attempts | [uint32](#uint32) |  | Configured maximum outstanding attempts. |
| max_failed_attempts_per_day | [uint32](#uint32) |  | Configured maximum failed attempts per day. |
| halt_reason | [string](#string) |  | Halt reason of the Safety layer, empty when not halted. |
| day_realized_usd_micro | [int64](#int64) | optional | Booked day: realized PnL (USD micro) and failed / total attempts. |
| day_failed_attempts | [uint32](#uint32) |  | Failed attempts of the day. |
| day_attempts | [uint32](#uint32) |  | Attempts of the day. |






<a name="darkbot-project0-v1-OwnerThreshold"></a>

### OwnerThreshold
DEC-06: the threshold mode — the operator&#39;s request, the owner&#39;s effective mode and why it
differs; OBSERVE&#39;s comparison with the sweep on the same oracle ticks (hits, misses, extra
wake-ups, both detection times) and the cost of both paths; ACTIVE&#39;s sweep cross-check. Counters
are the owner run&#39;s (a resumed run continues them). ACTIVE is no LIVE.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| requested | [string](#string) |  | OFF | OBSERVE | ACTIVE. |
| requested_source | [string](#string) |  | config | controller seq N | resume. |
| effective | [string](#string) |  | OFF | SWEEP | OBSERVE | ACTIVE. |
| block_reason | [string](#string) |  | Why effective is not the requested mode (INDEX_BOOTSTRAP, STATE_NOT_TRADABLE, B8_UNDECIDED, THRESHOLD_EVIDENCE_SHORT, THRESHOLD_MISS_FALLBACK, THRESHOLD_BARRIER); empty when it is. |
| generation | [uint64](#uint64) |  | Bumped by every change of the effective mode or its block reason. |
| min_ticks | [uint64](#uint64) | optional | N (B-8, threshold_min_ticks); unset while the user has not decided it (ACTIVE blocked). |
| ticks | [uint64](#uint64) |  | Comparable oracle ticks in OBSERVE (same state and input revision as the sweep). |
| streak | [uint64](#uint64) |  | Comparable oracle ticks since the last miss (ACTIVE needs N). |
| uncomparable_ticks | [uint64](#uint64) |  | Oracle ticks not comparable (sampled, a price ACTIVE projects first, generation change). |
| hits | [uint64](#uint64) |  | Liquidatable accounts found by the sweep and woken by the threshold trigger. |
| misses | [uint64](#uint64) |  | Liquidatable accounts the threshold trigger did not wake (OBSERVE and the ACTIVE cross-check). |
| extra | [uint64](#uint64) |  | Accounts the threshold trigger woke that the sweep found healthy. |
| sampled_out | [uint64](#uint64) |  | OBSERVE work skipped for want of budget (B-9). |
| fallbacks | [uint64](#uint64) |  | Times the effective mode left ACTIVE (each with a full sweep of every bank). |
| index_tracked | [uint64](#uint64) |  | Index accounts with exact thresholds. |
| index_fallback | [uint64](#uint64) |  | Index accounts with unknown thresholds, re-checked bank-wide. |
| index_bootstrap_pending | [uint64](#uint64) |  | Accounts waiting for their thresholds (bootstrap queue). |
| index_evaluations | [uint64](#uint64) |  | Exact threshold evaluations of the index. |
| crosscheck_samples | [uint64](#uint64) |  | ACTIVE cross-check samples evaluated off the owner. |
| crosscheck_skipped | [uint64](#uint64) |  | ACTIVE cross-check samples skipped (queue full). |
| crosscheck_stale | [uint64](#uint64) |  | ACTIVE cross-check samples whose state or generation changed (or failed). |
| crosscheck_misses | [uint64](#uint64) |  | ACTIVE cross-check misses (each an alarm and a fallback). |
| sweep_detect | [LatencyNs](#darkbot-project0-v1-LatencyNs) |  | t_rx → exact sweep evaluation of the liquidatable accounts of compared OBSERVE ticks. |
| threshold_detect | [LatencyNs](#darkbot-project0-v1-LatencyNs) |  | t_rx → threshold wake-up of the same accounts. |
| observe_owner | [LatencyNs](#darkbot-project0-v1-LatencyNs) |  | Owner time of one unit of threshold work beside the sweep (lookup, comparison, index upkeep, bootstrap batch). |
| observe_owner_ns_total | [uint64](#uint64) |  | The total owner time of that work. |
| observe_budget_ppm | [uint64](#uint64) |  | The budget it is held to, in ppm of the owner&#39;s wall time (B-9). |
| sweep_health_evaluations | [uint64](#uint64) |  | The sweep&#39;s exact health evaluations. |
| sweep_owner_ns_total | [uint64](#uint64) |  | The owner time of the sweep&#39;s batches. |






<a name="darkbot-project0-v1-OwnerTrigger"></a>

### OwnerTrigger
Candidate trigger and its lanes.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| mode | [string](#string) |  | sweep | threshold. |
| threshold_ready | [bool](#bool) |  | Threshold mode: every account&#39;s thresholds are known. |
| lane_l0 | [uint64](#uint64) |  | Pending candidates per lane. |
| lane_l1 | [uint64](#uint64) |  | Pending L1 candidates. |
| lane_l2 | [uint64](#uint64) |  | Pending L2 candidates (re-checks). |
| detect_queue | [uint64](#uint64) |  | Detection work queued behind the lanes. |
| indexed_accounts | [uint64](#uint64) |  | Accounts in the reverse index. |
| suppressed | [uint64](#uint64) |  | Repeats suppressed by the forward policy (no new cause). |
| softcap_suppressed | [uint64](#uint64) |  | Due repeats suppressed by the repeat soft cap. |
| blockhash_refused | [uint64](#uint64) |  | DEC-01: lane candidates refused before planning on a stale or missing blockhash (loss Build, cause BLOCKHASH_STALE) in this run. |
| blockhash_parked | [uint64](#uint64) |  | DEC-01: candidates parked now, waiting for a newer blockhash (at most one per account). |
| blockhash_rearmed | [uint64](#uint64) |  | DEC-01: parked candidates returned to their lane after a newer blockhash in this run. |
| blockhash_superseded | [uint64](#uint64) |  | DEC-01: parked candidates covered by a later evaluation of the account in this run. |
| blockhash_refresh_requests | [uint64](#uint64) |  | DEC-01: refresh wake-ups sent to the blockhash worker in this run (pending ones coalesced). |
| threshold | [OwnerThreshold](#darkbot-project0-v1-OwnerThreshold) |  | DEC-06: the threshold mode. |






<a name="darkbot-project0-v1-ProgramPin"></a>

### ProgramPin
The ProgramData the owner started on and the qualified deployment it is pinned against
(liquidation/src/protocol.rs; only REFERENCE may run unhalted, everything else is BLOCKED).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| verdict | [string](#string) |  | REFERENCE | NOT_REFERENCE | UNPINNED. |
| observed_hash | [string](#string) |  | Hash of the complete ProgramData account at startup. |
| deployment_slot | [uint64](#uint64) |  | ProgramData deployment slot. |
| upgrade_authority | [string](#string) | optional | Upgrade authority; absent when the program is immutable. |
| source_tag | [string](#string) |  | The protocol source this build decodes. |
| reference_source_tag | [string](#string) |  | The source the qualified reference deployment was qualified for. |
| reference_hash | [string](#string) |  | Hash of the qualified reference deployment. |






<a name="darkbot-project0-v1-RebalanceBlocked"></a>

### RebalanceBlocked
An asset the plan does not rebalance.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| asset | [string](#string) |  | Inventory key. |
| reason | [string](#string) |  | Stable reason (INVENTORY_BALANCE_UNKNOWN, REBALANCE_COST_UNKNOWN, …). |
| wallet | [string](#string) |  | DEC-17b: the wallet it is not rebalanced in, base58. |






<a name="darkbot-project0-v1-RebalancePlan"></a>

### RebalancePlan
B01 (P8-INV-01): the owner&#39;s rebalancing plan in OBSERVE — real balances, quotes and conservative
costs; planned, never traded (ACTIVE needs LIVE).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| mode | [string](#string) |  | OFF or OBSERVE. |
| at_unix_ms | [uint64](#uint64) | optional | Wall clock of the plan; unset before the first one. |
| slot | [uint64](#uint64) |  | Highest slot of the canonical state the plan read. |
| trades | [RebalanceTrade](#darkbot-project0-v1-RebalanceTrade) | repeated | Planned trades. |
| blocked | [RebalanceBlocked](#darkbot-project0-v1-RebalanceBlocked) | repeated | Assets not rebalanced, with the stable reason. |
| quotes_pending | [uint64](#uint64) |  | Quotes asked and not answered yet (the plan is then provisional). |
| budget_usd_micro | [uint64](#uint64) |  | The plan&#39;s cost budget (one for every wallet&#39;s trades). |
| transfers | [RebalanceTransfer](#darkbot-project0-v1-RebalanceTransfer) | repeated | DEC-17b: the planned moves between the owner&#39;s wallets — the only capital flow between them, never executed. |






<a name="darkbot-project0-v1-RebalanceTrade"></a>

### RebalanceTrade
One planned rebalance trade (ExactIn).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| sell | [string](#string) |  | Inventory key sold (SOL or a mint). |
| buy | [string](#string) |  | Inventory key bought. |
| amount_in | [uint64](#uint64) |  | Amount sold (base units). |
| minimum_out | [uint64](#uint64) |  | The on-chain enforced minimum output (base units). |
| cost_usd_micro | [uint64](#uint64) |  | Conservative cost: value given minus value received plus the planned network fee. |
| wallet | [string](#string) |  | DEC-17b: the wallet that would trade, base58. |






<a name="darkbot-project0-v1-RebalanceTransfer"></a>

### RebalanceTransfer
DEC-17b: one planned move of a band&#39;s asset from one wallet to another (OBSERVE).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| asset | [string](#string) |  | Inventory key (SOL or a mint). |
| from_wallet | [string](#string) |  | Giving wallet, base58 (above the band&#39;s max, never below its target). |
| to_wallet | [string](#string) |  | Receiving wallet, base58 (below the band&#39;s min, refilled to its target). |
| amount | [uint64](#uint64) |  | Amount moved (base units). |






<a name="darkbot-project0-v1-SourceControlCommand"></a>

### SourceControlCommand
Parameters (CommandRequest.params) of project0.source.set: the operands of
`p0ctl source set UNIT MODE`; the running owner applies or refuses the request in order.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| unit | [string](#string) |  | wss or geyser:&lt;provider&gt;. |
| mode | [string](#string) |  | off, observe or active (active needs a qualification record). |






<a name="darkbot-project0-v1-SourceDecision"></a>

### SourceDecision
The owner&#39;s decision on one source-control request.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| seq | [uint64](#uint64) |  | Request sequence number. |
| request | [string](#string) |  | `&lt;unit&gt; &lt;mode&gt;` (source) or `&lt;capability&gt; &lt;requested&gt;` (DEC-13 capability) as requested. |
| applied | [bool](#bool) |  | Applied, or refused (see detail). |
| detail | [string](#string) |  | The refusal reason, or the canonical handoff of an applied control. |






<a name="darkbot-project0-v1-SourceStats"></a>

### SourceStats
Source counters of the owner.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| observed | [uint64](#uint64) |  | Events observed by non-canonical units. |
| rejected | [uint64](#uint64) |  | Events refused (stale stamp, OFF) or not observable. |
| applied | [uint64](#uint64) |  | Control changes applied. |
| refused | [uint64](#uint64) |  | Control changes refused. |






<a name="darkbot-project0-v1-StateCounters"></a>

### StateCounters
StateCore write counters (solana/src/state::StateCounters).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| applied | [uint64](#uint64) |  | Accepted writes. |
| stale_slot | [uint64](#uint64) |  | Updates older than the stored slot. |
| stale_write_version | [uint64](#uint64) |  | Same-slot updates with an older write_version. |
| same_slot_no_wv | [uint64](#uint64) |  | Same-slot updates without write_version on both sides (later wins, counted). |
| same_slot_unversioned_refused | [uint64](#uint64) |  | F-05: unversioned same-slot updates refused against a versioned record. |
| below_root | [uint64](#uint64) |  | Updates below the rooted slot. |
| buffered | [uint64](#uint64) |  | Updates buffered before the first snapshot. |
| buffer_dropped_le_snapshot | [uint64](#uint64) |  | Buffered updates not newer than the snapshot. |
| reconcile_mismatch | [uint64](#uint64) |  | Reconcile: data differs from the snapshot. |
| reconcile_missing_local | [uint64](#uint64) |  | Reconcile: in the snapshot, not in the state. |
| reconcile_missing_remote | [uint64](#uint64) |  | Reconcile: in the state, not in the snapshot. |
| orphaned_writes | [uint64](#uint64) |  | Writes removed by fork invalidation. |






<a name="darkbot-project0-v1-WalletRow"></a>

### WalletRow
DEC-17b: one configured signer&#39;s wallet — its roles and switch, whether and why not it may run
a candidate now, its SOL, holds, held attempt, Safety and journal, and the candidates of this run
it ran. Unknown values are unset (never 0).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| wallet | [string](#string) |  | Wallet, base58. |
| roles | [string](#string) | repeated | LIQUIDATOR, FEE_PAYER, TREASURY. |
| enabled | [bool](#bool) |  | Its switch WALLET:&lt;pubkey&gt; is ENABLED. |
| runs_candidates | [bool](#bool) |  | It liquidates (as its own fee payer, or paid by a fee payer: DEC-17b B4). |
| block_reason | [string](#string) |  | Why it may not run a candidate now in the effective mode; empty: it may. |
| sol_lamports | [uint64](#uint64) | optional | Lamports in canonical state; unset when unknown. |
| free_sol_lamports | [uint64](#uint64) | optional | Lamports free after its holds (exit reserves included) and the policy reserve; unset when unknown. |
| holds | [AssetAmount](#darkbot-project0-v1-AssetAmount) | repeated | Holds of its attempts and open positions, per asset. |
| held_attempt | [string](#string) |  | State of its held attempt: NONE | RESERVED | UNKNOWN | FINALIZED_UNBOOKED | SETTLED. |
| halt_reason | [string](#string) | optional | Its Safety HALT; unset while not halted. |
| capital_in_use_usd_micro | [uint64](#uint64) |  | Capital its Safety holds reserved. |
| day_attempts | [uint32](#uint32) |  | Its attempts of the booking day. |
| journal_read | [bool](#bool) |  | Its FE-04 journals were read. |
| realized_net_usd_micro | [int64](#int64) | optional | Realized net of its settled steps; unset unless read and known. |
| selected | [uint64](#uint64) |  | Candidates of this run it ran. |
| fee_payer | [string](#string) | optional | DEC-17b B4: the wallet paying its candidates now (itself, or the fee payer it runs with); unset when it runs none or no fee payer may pay now. |
| sol_held_for_others | [uint64](#uint64) |  | DEC-17b B4: lamports held on it for the attempts of the liquidators it pays. |






<a name="darkbot-project0-v1-WalletsState"></a>

### WalletsState
FE-04 wallet state: STATE topic project0.wallets.state. Read from the wallets&#39; FE-04 journals
(execution/accounting.rs) and reservation books; fields 2–16 are the pilot wallet&#39;s (DEC-17b:
the first configured signer, its own fee payer), every wallet&#39;s in `rows`.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| finance_snapshot_id | [string](#string) |  | Same id as the FinanceSummary of this state. |
| wallet | [string](#string) |  | Liquidator wallet, base58. |
| journal_read | [bool](#bool) |  | An FE-04 journal directory is configured and was read. |
| verdict | [string](#string) |  | FE-07 verdict: NO_TRADE | FIRST_REALIZED_PROFIT_EVIDENCED | LOSS | UNKNOWN. |
| unknown_reasons | [string](#string) | repeated | Why the verdict is UNKNOWN (unsettled steps, open inventory) or the journal read error. |
| attempts | [uint64](#uint64) | optional | Own attempts in the journals (archived and active); unset unless the journal was read. |
| archived_attempts | [uint64](#uint64) | optional | Archived attempts; unset unless the journal was read. |
| landed_attempts | [uint64](#uint64) | optional | Settled attempts that landed; unset unless the journal was read. |
| settled_exits | [uint64](#uint64) | optional | Settled exits; unset unless the journal was read. |
| fee_lamports | [uint64](#uint64) | optional | Fees of the settled steps; unset on overflow. |
| open_inventory_mark_usd_micro | [int64](#int64) | optional | Mark of the open inventory; unset when a position is unvalued. |
| holds | [AssetAmount](#darkbot-project0-v1-AssetAmount) | repeated | Holds of every attempt in the reservation book, per asset (&#34;SOL&#34; lamports or mint units). |
| policy_reserves | [AssetAmount](#darkbot-project0-v1-AssetAmount) | repeated | Never-spent policy reserves of the inventory bands. |
| held_attempt | [string](#string) |  | State of the attempt held by this run: NONE | RESERVED | UNKNOWN | FINALIZED_UNBOOKED | SETTLED. |
| non_landing | [NonLanding](#darkbot-project0-v1-NonLanding) |  | DEC-03: the non-landing status of the held attempt or its latest exit; unset without a held FE-04 journal. |
| hold_b | [HoldB](#darkbot-project0-v1-HoldB) |  | DEC-04: the B hold mode and the held attempt&#39;s A, B and finalized flows; unset without an owner projection. |
| rows | [WalletRow](#darkbot-project0-v1-WalletRow) | repeated | DEC-17b: every configured signer&#39;s wallet. |
| selections | [uint64](#uint64) |  | DEC-17b: candidates this run chose a wallet for. |
| selections_none_eligible | [uint64](#uint64) |  | DEC-17b: of those, the ones no wallet could run (the pilot&#39;s refusal was theirs). |
| last_none_eligible | [string](#string) |  | DEC-17b: why no wallet could run the last of them (`&lt;wallet&gt;=&lt;reason&gt;, …`). |















<a name="darkbot_project0_v1_status-proto"></a>
<p align="right"><a href="#top">Top</a></p>

## darkbot/project0/v1/status.proto



<a name="darkbot-project0-v1-StatusProjection"></a>

### StatusProjection
First projection of project0.status. Superseded by SystemStatus (project0.status) and
CaptureState (project0.capture.state); kept unchanged for wire compatibility until package v2.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| run_id | [string](#string) |  | Run id of the capture. |
| last_observed_slot | [uint64](#uint64) | optional | Last slot of an admitted transaction. |
| last_frame_age_ns | [uint64](#uint64) | optional | Age of the last ingress application frame. |
| gap_count | [uint64](#uint64) | optional | Transport gaps counted inside the observation window. |
| reconnect_count | [uint64](#uint64) | optional | Reconnects counted inside the observation window. |
| liquidation_events | [uint64](#uint64) | optional | Counted liquidation events. |
| queue_bytes | [uint64](#uint64) | optional | Writer queue bytes. |
| lag_frames | [uint64](#uint64) | optional | Writer lag in frames. |
| market_verdict | [string](#string) |  | MARKET_OBSERVER_STREAM_NOT_READY | MARKET_DATA_INCOMPLETE | MARKET_DATA_SOURCE_COVERAGE_UNVERIFIED | MARKET_DATA_VALID |















<a name="darkbot_project0_v1_system-proto"></a>
<p align="right"><a href="#top">Top</a></p>

## darkbot/project0/v1/system.proto



<a name="darkbot-project0-v1-EffectiveRecord"></a>

### EffectiveRecord
Secret-free effective operational record (runtime_config::EffectiveConfig).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| schema | [string](#string) |  | Record schema. |
| profile | [string](#string) |  | Profile: smoke | standard | performance | custom. |
| rpc_endpoint_count | [uint64](#uint64) |  | Configured RPC endpoints (URLs never leave the engine). |
| wss_endpoint_count | [uint64](#uint64) |  | Configured WebSocket endpoints. |
| fallback | [bool](#bool) |  | Endpoint fallback enabled. |
| adaptive | [bool](#bool) |  | Adaptive limits enabled. |
| duration | [string](#string) |  | Configured duration (e.g. 4h). |
| verification | [string](#string) |  | Verification mode: off | index | strict | crosscheck. |
| diagnostics | [bool](#bool) |  | Diagnostics enabled. |
| fe_mode | [string](#string) |  | FE mode: smoke | full. |
| fe_targets | [string](#string) | repeated | FE target accounts, base58. |
| fe_group | [string](#string) |  | FE group account, base58. |
| sources | [EffectiveRecord.SourcesEntry](#darkbot-project0-v1-EffectiveRecord-SourcesEntry) | repeated | Origin of every configured value by field name; the profile&#39;s own origin is mode:&lt;mode&gt; (shadow|econ4h|market) when the operating mode selected it. |
| provider_capacity_qualified | [bool](#bool) |  | Provider capacity qualified (fixed false today). |
| free_only | [bool](#bool) |  | Free-plan-only operation (fixed true today). |
| live_authorized | [bool](#bool) |  | LIVE authorized (fixed false today). |
| submission_authorized | [bool](#bool) |  | Transaction submission authorized (fixed false today). |
| threshold_trigger | [bool](#bool) |  | Threshold trigger instead of the R127 sweep (P0_USE_THRESHOLD_TRIGGER). |
| c0_census | [bool](#bool) |  | C0 census on the startup sweep (P0_C0_CENSUS). |
| source_geyser | [string](#string) |  | Start mode of the Geyser source unit: off | observe | active. |
| source_geyser_provider | [string](#string) |  | Provider of the Geyser source unit. |
| providers | [RunProvider](#darkbot-project0-v1-RunProvider) | repeated | DEC-18: the providers the run is bound to (runtime_config::ProviderBinding), secret-free and without their endpoints; empty in a record of an earlier revision. |






<a name="darkbot-project0-v1-EffectiveRecord-SourcesEntry"></a>

### EffectiveRecord.SourcesEntry



| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| key | [string](#string) |  |  |
| value | [string](#string) |  |  |






<a name="darkbot-project0-v1-OperationalConfig"></a>

### OperationalConfig
Operational configuration of the process.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| parameters | [OperationalParameter](#darkbot-project0-v1-OperationalParameter) | repeated | Every numeric operational parameter with its active value. |
| effective | [EffectiveRecord](#darkbot-project0-v1-EffectiveRecord) |  | Effective record; unset when the process runs on built-in defaults. |






<a name="darkbot-project0-v1-OperationalParameter"></a>

### OperationalParameter
One numeric operational parameter of runtime_config::PARAMETERS.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| field | [string](#string) |  | Parameter field name (e.g. rpc_rps). |
| unit | [string](#string) |  | Unit as declared by the engine. |
| value | [uint64](#uint64) |  | Active value of this process. |
| default_value | [uint64](#uint64) |  | Built-in default. |
| env | [string](#string) |  | Environment variable that sets it. |
| source | [string](#string) |  | Origin of the value: profile:&lt;name&gt; | environment | config_file | cli | fe_mode:full | builtin_default. |
| minimum | [uint64](#uint64) |  | Smallest valid value. |
| maximum | [uint64](#uint64) |  | Largest valid value. |
| provisional | [string](#string) |  | Open measurement question (ENTSCHEIDUNGEN §3, e.g. B-2) that keeps the value provisional; empty for a value no measurement question covers. |
| change_rule | [string](#string) |  | When a change takes effect: restart (every numeric parameter today). |






<a name="darkbot-project0-v1-RunProvider"></a>

### RunProvider
DEC-18: one provider a run is bound to.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| kind | [string](#string) |  | rpc | wss | geyser | jupiter | jito. |
| id | [string](#string) |  | Provider id (local: a named loopback endpoint; jupiter-default: the contract&#39;s profile). |
| binding | [string](#string) |  | Its secret-free binding (hex SHA-256). |
| registry_version | [uint64](#uint64) |  | The registry version it was bound from; 0 for a local endpoint or a default profile. |
| credential_id | [string](#string) |  | The credential id it authenticates with; empty without. |
| credential_revision | [uint64](#uint64) |  | The credential revision bound. |
| plan | [string](#string) |  | free | paid | unknown. |






<a name="darkbot-project0-v1-SystemStatus"></a>

### SystemStatus
Process-level status of Project_0: STATE topic project0.status. Sent when a reporter session
starts and whenever the operational configuration changes (restart only today).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| operational | [OperationalConfig](#darkbot-project0-v1-OperationalConfig) |  | Operational parameters of the process (solana/src/runtime_config.rs). |















## Scalar Value Types

| .proto Type | Notes | C++ | Java | Python | Go | C# | PHP | Ruby |
| ----------- | ----- | --- | ---- | ------ | -- | -- | --- | ---- |
| <a name="double" /> double |  | double | double | float | float64 | double | float | Float |
| <a name="float" /> float |  | float | float | float | float32 | float | float | Float |
| <a name="int32" /> int32 | Uses variable-length encoding. Inefficient for encoding negative numbers – if your field is likely to have negative values, use sint32 instead. | int32 | int | int | int32 | int | integer | Bignum or Fixnum (as required) |
| <a name="int64" /> int64 | Uses variable-length encoding. Inefficient for encoding negative numbers – if your field is likely to have negative values, use sint64 instead. | int64 | long | int/long | int64 | long | integer/string | Bignum |
| <a name="uint32" /> uint32 | Uses variable-length encoding. | uint32 | int | int/long | uint32 | uint | integer | Bignum or Fixnum (as required) |
| <a name="uint64" /> uint64 | Uses variable-length encoding. | uint64 | long | int/long | uint64 | ulong | integer/string | Bignum or Fixnum (as required) |
| <a name="sint32" /> sint32 | Uses variable-length encoding. Signed int value. These more efficiently encode negative numbers than regular int32s. | int32 | int | int | int32 | int | integer | Bignum or Fixnum (as required) |
| <a name="sint64" /> sint64 | Uses variable-length encoding. Signed int value. These more efficiently encode negative numbers than regular int64s. | int64 | long | int/long | int64 | long | integer/string | Bignum |
| <a name="fixed32" /> fixed32 | Always four bytes. More efficient than uint32 if values are often greater than 2^28. | uint32 | int | int | uint32 | uint | integer | Bignum or Fixnum (as required) |
| <a name="fixed64" /> fixed64 | Always eight bytes. More efficient than uint64 if values are often greater than 2^56. | uint64 | long | int/long | uint64 | ulong | integer/string | Bignum |
| <a name="sfixed32" /> sfixed32 | Always four bytes. | int32 | int | int | int32 | int | integer | Bignum or Fixnum (as required) |
| <a name="sfixed64" /> sfixed64 | Always eight bytes. | int64 | long | int/long | int64 | long | integer/string | Bignum |
| <a name="bool" /> bool |  | bool | boolean | boolean | bool | bool | boolean | TrueClass/FalseClass |
| <a name="string" /> string | A string must always contain UTF-8 encoded or 7-bit ASCII text. | string | String | str/unicode | string | string | string | String (UTF-8) |
| <a name="bytes" /> bytes | May contain any arbitrary sequence of bytes. | string | ByteString | str | []byte | ByteString | string | String (ASCII-8BIT) |
