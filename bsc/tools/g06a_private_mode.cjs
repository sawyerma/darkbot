// Fail-closed privacy of the local EVM runs (plan v8/v9.1 B2): the launch
// salt, the launch calldata and every address the run creates never reach an
// external RPC. Shared by g06a_local_simulate.cjs and g06a_launch_path_trace.cjs.
//
// - guardEgress: every JSON-RPC request leaves through globalThis.fetch; a
//   request carrying a private value, or a method that would execute calldata
//   remotely, is refused before it is sent. The refusal never names the value.
// - localCreations: the address of every CREATE/CREATE2 of the run (and the
//   predicted token, before it exists) is registered when the EVM derives it,
//   so its account, code and storage are served from the local state only.
//   The RPCStateManager caches are ordered maps (no eviction), so local reads
//   are exact.
'use strict';
const path = require('path');
const { createAccountFromRLP } = require(path.join(__dirname, 'evm-audit-tools/node_modules/@ethereumjs/util'));

const DENIED_METHODS = new Set([
  'eth_call', 'eth_estimateGas', 'eth_createAccessList', 'eth_sendRawTransaction',
  'eth_sendTransaction', 'eth_simulateV1', 'debug_traceCall',
]);

function bare(hex) {
  return String(hex).toLowerCase().replace(/^0x/, '');
}

/** Installs the egress guard over globalThis.fetch; returns its removal (tests only). */
function guardEgress(privateValues) {
  const needles = privateValues.map(bare);
  if (needles.length === 0 || needles.some((n) => !/^[0-9a-f]{40,}$/.test(n))) {
    throw new Error('private egress guard needs hex values of at least 20 bytes');
  }
  const send = globalThis.fetch;
  if (send.darkbotEgressGuard) throw new Error('private egress guard installed twice');
  const guarded = async (url, init = {}) => {
    if (init.body != null && typeof init.body !== 'string') {
      throw new Error('PRIVATE_EGRESS_REFUSED: non-text request body');
    }
    const body = init.body || '';
    if (needles.some((n) => body.toLowerCase().includes(n))) {
      throw new Error('PRIVATE_EGRESS_REFUSED: the request carries a private value');
    }
    let calls;
    try {
      calls = [].concat(JSON.parse(body || '{}'));
    } catch {
      throw new Error('PRIVATE_EGRESS_REFUSED: request is not JSON-RPC');
    }
    for (const call of calls) {
      if (DENIED_METHODS.has(call && call.method)) {
        throw new Error(`PRIVATE_EGRESS_REFUSED: ${call.method} would execute calldata remotely`);
      }
    }
    return send(url, init);
  };
  guarded.darkbotEgressGuard = true;
  globalThis.fetch = guarded;
  return () => { globalThis.fetch = send; };
}

/** Serves every address the run creates from the local state; returns that set. */
function localCreations(evm, state, predicted = []) {
  const created = new Set(predicted.map(bare));
  const derive = evm._generateAddress.bind(evm);
  evm._generateAddress = async (message) => {
    const address = await derive(message);
    created.add(bare(address.toString()));
    return address;
  };
  const local = (address) => created.has(bare(address.toString()));
  const getAccount = state.getAccount.bind(state);
  const getCode = state.getCode.bind(state);
  const getStorage = state.getStorage.bind(state);
  state.getAccount = async (address) => {
    if (!local(address)) return getAccount(address);
    const elem = state._caches.account?.get(address);
    return elem?.accountRLP !== undefined ? createAccountFromRLP(elem.accountRLP) : undefined;
  };
  state.getCode = async (address) => {
    if (!local(address)) return getCode(address);
    return state._caches.code?.get(address)?.code ?? new Uint8Array();
  };
  state.getStorage = async (address, key) => {
    if (!local(address)) return getStorage(address, key);
    return state._caches.storage?.get(address, key) ?? new Uint8Array();
  };
  // The SSTORE gas schedule reads original values through its own bound copy.
  if (state.originalStorageCache) state.originalStorageCache.getStorage = state.getStorage;
  return created;
}

module.exports = { guardEgress, localCreations, DENIED_METHODS };
