import { bigIntToHex } from '@ethereumjs/util';
/**
 * Converts a beacon block execution payload JSON object {@link BeaconPayloadJSON} to the {@link ExecutionPayload} data needed to construct a {@link Block}.
 * The JSON data can be retrieved from a consensus layer (CL) client on this Beacon API `/eth/v2/beacon/blocks/[block number]`
 */
export function executionPayloadFromBeaconPayload(payload) {
    const executionPayload = {
        parentHash: payload.parent_hash,
        feeRecipient: payload.fee_recipient,
        stateRoot: payload.state_root,
        receiptsRoot: payload.receipts_root,
        logsBloom: payload.logs_bloom,
        prevRandao: payload.prev_randao,
        blockNumber: bigIntToHex(BigInt(payload.block_number)),
        gasLimit: bigIntToHex(BigInt(payload.gas_limit)),
        gasUsed: bigIntToHex(BigInt(payload.gas_used)),
        timestamp: bigIntToHex(BigInt(payload.timestamp)),
        extraData: payload.extra_data,
        baseFeePerGas: bigIntToHex(BigInt(payload.base_fee_per_gas)),
        blockHash: payload.block_hash,
        transactions: payload.transactions,
    };
    if (payload.withdrawals !== undefined && payload.withdrawals !== null) {
        executionPayload.withdrawals = payload.withdrawals.map((wd) => ({
            index: bigIntToHex(BigInt(wd.index)),
            validatorIndex: bigIntToHex(BigInt(wd.validator_index)),
            address: wd.address,
            amount: bigIntToHex(BigInt(wd.amount)),
        }));
    }
    if (payload.blob_gas_used !== undefined && payload.blob_gas_used !== null) {
        executionPayload.blobGasUsed = bigIntToHex(BigInt(payload.blob_gas_used));
    }
    if (payload.excess_blob_gas !== undefined && payload.excess_blob_gas !== null) {
        executionPayload.excessBlobGas = bigIntToHex(BigInt(payload.excess_blob_gas));
    }
    if (payload.parent_beacon_block_root !== undefined && payload.parent_beacon_block_root !== null) {
        executionPayload.parentBeaconBlockRoot = payload.parent_beacon_block_root;
    }
    if (payload.requests_hash !== undefined && payload.requests_hash !== null) {
        executionPayload.requestsHash = payload.requests_hash;
    }
    return executionPayload;
}
//# sourceMappingURL=from-beacon-payload.js.map