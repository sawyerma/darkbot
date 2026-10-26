# Protocol Documentation
<a name="top"></a>

## Table of Contents

- [darkbot/bsc/v1/status.proto](#darkbot_bsc_v1_status-proto)
    - [BscLaunchState](#darkbot-bsc-v1-BscLaunchState)
    - [BscMonitoringState](#darkbot-bsc-v1-BscMonitoringState)
    - [BscStatus](#darkbot-bsc-v1-BscStatus)
    - [BscWalletsState](#darkbot-bsc-v1-BscWalletsState)
    - [BuilderStatus](#darkbot-bsc-v1-BuilderStatus)
    - [DryStatus](#darkbot-bsc-v1-DryStatus)
    - [Evidence](#darkbot-bsc-v1-Evidence)
    - [GateStatus](#darkbot-bsc-v1-GateStatus)
    - [GuardStatus](#darkbot-bsc-v1-GuardStatus)
    - [JobStatus](#darkbot-bsc-v1-JobStatus)
    - [NativeField](#darkbot-bsc-v1-NativeField)
    - [NativeRecord](#darkbot-bsc-v1-NativeRecord)
    - [PublicField](#darkbot-bsc-v1-PublicField)
    - [ReadinessStatus](#darkbot-bsc-v1-ReadinessStatus)
    - [WitnessStatus](#darkbot-bsc-v1-WitnessStatus)

    - [NativeKind](#darkbot-bsc-v1-NativeKind)

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
    - [BacktestJobCommand](#darkbot-project0-v1-BacktestJobCommand)
    - [BacktestResult](#darkbot-project0-v1-BacktestResult)
    - [CapitalScenario](#darkbot-project0-v1-CapitalScenario)
    - [FundingSet](#darkbot-project0-v1-FundingSet)
    - [JobGrid](#darkbot-project0-v1-JobGrid)
    - [JobParams](#darkbot-project0-v1-JobParams)
    - [OptimizeJobCommand](#darkbot-project0-v1-OptimizeJobCommand)
    - [OptimizeResult](#darkbot-project0-v1-OptimizeResult)
    - [OptimizeScore](#darkbot-project0-v1-OptimizeScore)
    - [PostCaptureJobCommand](#darkbot-project0-v1-PostCaptureJobCommand)
    - [PostCaptureResult](#darkbot-project0-v1-PostCaptureResult)
    - [PostCaptureSpec](#darkbot-project0-v1-PostCaptureSpec)
    - [RunQualification](#darkbot-project0-v1-RunQualification)
    - [ShadowCounterfactual](#darkbot-project0-v1-ShadowCounterfactual)

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
    - [Fe04Approval](#darkbot-project0-v1-Fe04Approval)
    - [FinanceSummary](#darkbot-project0-v1-FinanceSummary)
    - [LiquidatorState](#darkbot-project0-v1-LiquidatorState)
    - [LossCount](#darkbot-project0-v1-LossCount)
    - [OwnerEngine](#darkbot-project0-v1-OwnerEngine)
    - [OwnerPipeline](#darkbot-project0-v1-OwnerPipeline)
    - [OwnerRun](#darkbot-project0-v1-OwnerRun)
    - [OwnerSafety](#darkbot-project0-v1-OwnerSafety)
    - [OwnerTrigger](#darkbot-project0-v1-OwnerTrigger)
    - [ProgramPin](#darkbot-project0-v1-ProgramPin)
    - [RebalanceBlocked](#darkbot-project0-v1-RebalanceBlocked)
    - [RebalancePlan](#darkbot-project0-v1-RebalancePlan)
    - [RebalanceTrade](#darkbot-project0-v1-RebalanceTrade)
    - [SourceControlCommand](#darkbot-project0-v1-SourceControlCommand)
    - [SourceDecision](#darkbot-project0-v1-SourceDecision)
    - [SourceStats](#darkbot-project0-v1-SourceStats)
    - [StateCounters](#darkbot-project0-v1-StateCounters)
    - [WalletsState](#darkbot-project0-v1-WalletsState)

- [darkbot/project0/v1/status.proto](#darkbot_project0_v1_status-proto)
    - [StatusProjection](#darkbot-project0-v1-StatusProjection)

- [darkbot/project0/v1/system.proto](#darkbot_project0_v1_system-proto)
    - [EffectiveRecord](#darkbot-project0-v1-EffectiveRecord)
    - [EffectiveRecord.SourcesEntry](#darkbot-project0-v1-EffectiveRecord-SourcesEntry)
    - [OperationalConfig](#darkbot-project0-v1-OperationalConfig)
    - [OperationalParameter](#darkbot-project0-v1-OperationalParameter)
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
Read-only BSC evidence projection.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| source_commit | [string](#string) |  | Source revision deployed with this reporter. |
| evaluated_unix_s | [uint64](#uint64) |  | Projection evaluation time, not qualification freshness. |
| live_state | [string](#string) |  | BLOCKED: the GUI never enables trading. |
| guard | [GuardStatus](#darkbot-bsc-v1-GuardStatus) |  | Compiled guard and current contract observation. |
| gates | [GateStatus](#darkbot-bsc-v1-GateStatus) | repeated | Separate qualification gates. |
| readiness | [ReadinessStatus](#darkbot-bsc-v1-ReadinessStatus) |  | Latest native readiness evidence. |
| builder | [BuilderStatus](#darkbot-bsc-v1-BuilderStatus) |  | Builder observations. |
| witness | [WitnessStatus](#darkbot-bsc-v1-WitnessStatus) |  | Producer witness status. |
| dry | [DryStatus](#darkbot-bsc-v1-DryStatus) |  | Historical DRY status and exact values. |
| jobs | [JobStatus](#darkbot-bsc-v1-JobStatus) | repeated | Read-only jobs observed by launchd. |
| warnings | [string](#string) | repeated | Missing or invalid data is made explicit. |
| data_as_of_unix_s | [uint64](#uint64) |  | Time of the explicit source snapshot sync, never evidence freshness. |






<a name="darkbot-bsc-v1-BscWalletsState"></a>

### BscWalletsState
Public wallet identity and timestamped evidence balances, never keys.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| expected_address | [string](#string) |  | Optional configured public identity. |
| wallet_status | [string](#string) |  | Public wallet configuration state. |
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
| TOPIC_ID_BSC_EXECUTION_EVENTS | 40 | bsc.execution.events EVENT |
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
| kind | [string](#string) |  | BACKTEST or OPTIMIZE. |
| run | [string](#string) |  | The run as requested (an id under the run roots). |
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
| post_capture_spec | [PostCaptureSpec](#darkbot-project0-v1-PostCaptureSpec) |  | Post-capture only: the verification request. |






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
| chain_verify | [string](#string) |  | off or strict (strict only with offline). |
| offline | [bool](#bool) |  | Strict from the verification cache only (no network). |
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
| chain_verify | [string](#string) |  | off (identities only, no chain read) or strict. |
| offline | [bool](#bool) |  | Strict from the verification cache only (no network). |
| resume | [bool](#bool) |  | Resume an incomplete verification. |
| provider_evidence | [bool](#bool) |  | An operator provider evidence was given (online strict; terminal only). |






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






<a name="darkbot-project0-v1-CapabilityControlCommand"></a>

### CapabilityControlCommand
DEC-13: parameters (CommandRequest.params) of project0.capability.set: the operands of
`p0ctl capability set CAPABILITY REQUESTED`; the running owner decides what is effective
(LIVE stays BLOCKED until a separate FE-06 authorization).


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| capability | [string](#string) |  | LIVE. |
| requested | [string](#string) |  | OFF or LIVE. |






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






<a name="darkbot-project0-v1-LossCount"></a>

### LossCount
One loss reason with its count.


| Field | Type | Label | Description |
| ----- | ---- | ----- | ----------- |
| reason | [string](#string) |  | Loss reason (record name). |
| count | [uint64](#uint64) |  | Count of the day. |






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
| budget_usd_micro | [uint64](#uint64) |  | The plan&#39;s cost budget. |






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






<a name="darkbot-project0-v1-WalletsState"></a>

### WalletsState
FE-04 wallet state of the liquidator: STATE topic project0.wallets.state. Read from the
wallet&#39;s FE-04 journals (execution/accounting.rs) and the reservation book.


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
