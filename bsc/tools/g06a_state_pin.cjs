// One canonical state block for every local EVM run of the DRY (BSC-002 /
// BSC_Bug_V2 P2-D2). Shared by g06a_local_simulate.cjs,
// g06a_economics_evm_crosscheck.cjs and g06a_launch_path_trace.cjs.
//
// - pin: the EIP-1898 parameter {blockHash, requireCanonical: true}; a node
//   refuses it for a non-canonical block, so a load balancer or a reorg can
//   never substitute another state at the same height.
// - pinState: every RPCStateManager state read (code, storage, account
//   proof) uses that parameter instead of the block number.
// - requireBlock: the block at the preflight's height must be exactly the
//   preflight's block -- before the run and again right before its evidence
//   is written; otherwise the run stops without evidence.
'use strict';

function pin(blockHash) {
  if (!/^0x[0-9a-fA-F]{64}$/.test(String(blockHash))) throw new Error('state pin needs a 32-byte block hash');
  return { blockHash: String(blockHash).toLowerCase(), requireCanonical: true };
}

function pinState(state, blockHash) {
  state._blockTag = pin(blockHash);
}

/** The block at `number` is `hash` on this endpoint now; else an error. */
async function requireBlock(rpc, url, number, hash, when) {
  const block = await rpc(url, 'eth_getBlockByNumber', ['0x' + BigInt(number).toString(16), false]);
  const seen = block && typeof block.hash === 'string' ? block.hash.toLowerCase() : null;
  if (seen !== String(hash).toLowerCase()) {
    throw new Error(`STATE_BLOCK_MISMATCH ${when}: block ${number} is ${seen} on this endpoint, not the preflight block ${String(hash).toLowerCase()}`);
  }
  return block;
}

module.exports = { pin, pinState, requireBlock };
