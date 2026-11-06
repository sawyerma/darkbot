import { create, fromBinary, toBinary, type DescMessage, type MessageInitShape, type MessageShape } from "@bufbuild/protobuf";
import { BscQueryRequestSchema, BscQueryResponseSchema } from "@/core/generated/darkbot/bsc/v1/query_pb";
import { PerformanceSeriesSchema } from "@/core/generated/darkbot/bsc/v1/common_pb";
import { TradeDetailSchema } from "@/core/generated/darkbot/bsc/v1/history_pb";
import { LiveStrategySelectCommandSchema, StrategyCSettingsSetAllCommandSchema, StrategyCSettingsStateSchema, TrainingProfilesSchema, TrainingRunPageSchema, TrainingRunStartCommandSchema } from "@/core/generated/darkbot/bsc/v1/training_pb";
import { MainPayoutPolicySetCommandSchema, WalletDestinationSetCommandSchema, ProfitTransferPolicySetCommandSchema, WalletFundTradingCommandSchema, WalletHistoryPageSchema, WalletPayoutCommandSchema, WalletTreasuryStateSchema } from "@/core/generated/darkbot/bsc/v1/wallet_pb";
import { BscWatchStateSchema } from "@/core/generated/darkbot/bsc/v1/watch_pb";

// V8 (BSC-V8-09, FE-V8-17): the prepared V8 contracts of protocol/schemas/darkbot/bsc/v1/
// {common,training,history,wallet,watch,query}.proto, decoded in one place (the generated types
// stay behind adapters). No producer exists yet; the frontend shows these functions as
// NOT_IMPLEMENTED or UNAVAILABLE until the native owner answers (core/realtime/bscQuery.ts).

export { Availability, DataQuality, TimeSource, PerformanceMetric, PerformanceScope } from "@/core/generated/darkbot/bsc/v1/common_pb";
export type { PerformanceSeries } from "@/core/generated/darkbot/bsc/v1/common_pb";
export { TradeStatus, ExitReason, ExitReasonSource, TradePlacement, TradeSort } from "@/core/generated/darkbot/bsc/v1/history_pb";
export type { TradeRow, TradeDetail, TradeHistoryPage } from "@/core/generated/darkbot/bsc/v1/history_pb";
export { CreatorSoldBeforeFill, CreatorSellExit, SettingsApplyStatus, TrainingProfileStatus, TrainingRunMode, TrainingRunStatus, TrainingRunStopKind } from "@/core/generated/darkbot/bsc/v1/training_pb";
export type { TrainingProfile, TrainingProfiles, TrainingRun, TrainingRunPage } from "@/core/generated/darkbot/bsc/v1/training_pb";
export { ChainTxStatus, PolicyUnit, TransferStatus, WalletMovementKind, WalletRole } from "@/core/generated/darkbot/bsc/v1/wallet_pb";
export type { FiatQuote, MainPayoutPolicy, PolicyAmount, ProfitTransferPolicy, WalletAccount, WalletDestination, WalletHistoryPage, WalletMovement, WalletTransferState, WalletTreasuryState } from "@/core/generated/darkbot/bsc/v1/wallet_pb";

/** Every prepared V8 contract with a golden fixture (protocol/golden/bsc/v8/<name>.hex). */
export const V8_CONTRACTS = {
  query_trades_request: BscQueryRequestSchema,
  query_not_implemented: BscQueryResponseSchema,
  trade_page: BscQueryResponseSchema,
  trade_detail: TradeDetailSchema,
  strategy_settings_state: StrategyCSettingsStateSchema,
  strategy_set_all_command: StrategyCSettingsSetAllCommandSchema,
  training_profiles: TrainingProfilesSchema,
  training_run_page: TrainingRunPageSchema,
  training_run_start_command: TrainingRunStartCommandSchema,
  live_strategy_select_command: LiveStrategySelectCommandSchema,
  wallet_history_page: WalletHistoryPageSchema,
  treasury_state: WalletTreasuryStateSchema,
  destination_set_command: WalletDestinationSetCommandSchema,
  fund_trading_command: WalletFundTradingCommandSchema,
  payout_command: WalletPayoutCommandSchema,
  profit_policy_set_command: ProfitTransferPolicySetCommandSchema,
  payout_policy_set_command: MainPayoutPolicySetCommandSchema,
  performance_series: PerformanceSeriesSchema,
  watch_not_implemented: BscWatchStateSchema,
} as const satisfies Record<string, DescMessage>;
export type V8ContractName = keyof typeof V8_CONTRACTS;

/** Decodes one prepared V8 contract. */
export function decodeV8<N extends V8ContractName>(name: N, bytes: Uint8Array): MessageShape<(typeof V8_CONTRACTS)[N]> {
  return fromBinary(V8_CONTRACTS[name] as DescMessage, bytes) as MessageShape<(typeof V8_CONTRACTS)[N]>;
}
/** Encodes one prepared V8 contract (the golden round trip). */
export function encodeV8<N extends V8ContractName>(name: N, value: MessageShape<(typeof V8_CONTRACTS)[N]>): Uint8Array {
  return toBinary(V8_CONTRACTS[name] as DescMessage, value as never);
}

/** The params bytes of one prepared V8 command (a contract with a golden fixture). */
export function v8Params<N extends V8ContractName>(name: N, init: MessageInitShape<(typeof V8_CONTRACTS)[N]>): Uint8Array {
  const schema = V8_CONTRACTS[name] as DescMessage;
  return toBinary(schema, create(schema, init as never));
}
