// Offline regression of g06a_private_mode.cjs: a fake JSON-RPC provider and a
// hand-assembled factory whose CREATE2 child reads unwritten storage in its
// constructor. Without localCreations the RPC state manager asks the provider
// about the created address (the B token leak); with it, nothing about it
// leaves the process. Run: node --test bsc/tools/test_g06a_private_mode.cjs
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const path = require('path');
const { guardEgress, localCreations } = require('./g06a_private_mode.cjs');

const LEGACY = path.join(__dirname, 'evm-audit-tools/node_modules');
const { createEVM } = require(path.join(LEGACY, '@ethereumjs/evm'));
const { createCustomCommon, Mainnet, Hardfork } = require(path.join(LEGACY, '@ethereumjs/common'));
const { RPCStateManager } = require(path.join(LEGACY, '@ethereumjs/statemanager'));
const util = require(path.join(LEGACY, '@ethereumjs/util'));

const PROVIDER = 'http://fake-rpc.invalid/';
const SALT = '77'.repeat(32);
const FACTORY = '0x' + 'f0'.repeat(20);
const CALLER = '0x' + 'c1'.repeat(20);
// Constructor: slot1 = 42; SLOAD slot0 (unwritten); return runtime 0x00.
const INIT = '602a600155' + '60005450' + '6001601560003960016000f3' + '00';
// Factory: CREATE2(0, calldata, SALT), EXTCODESIZE(child), return child.
const FACTORY_CODE = '366000600037' + '7f' + SALT + '3660006000f5' + '803b50' + '60005260206000f3';

/** A fake provider over globalThis.fetch that records every request body. */
function fakeProvider() {
  const seen = [];
  const original = globalThis.fetch;
  globalThis.fetch = async (url, init) => {
    assert.equal(url, PROVIDER);
    seen.push(init.body);
    const { id, method } = JSON.parse(init.body);
    const zero = '0x' + '00'.repeat(32);
    const result = {
      eth_getProof: { balance: '0x0', nonce: '0x0', codeHash: zero, storageHash: zero },
      eth_getStorageAt: zero,
      eth_getCode: '0x',
    }[method];
    if (result === undefined) throw new Error(`fake provider: unexpected ${method}`);
    return { ok: true, status: 200, json: async () => ({ jsonrpc: '2.0', id, result }), text: async () => '' };
  };
  return { seen, restore: () => { globalThis.fetch = original; } };
}

async function run(protect) {
  const common = createCustomCommon({ chainId: 56 }, Mainnet, { hardfork: Hardfork.Osaka });
  const state = new RPCStateManager({ provider: PROVIDER, blockTag: 1n });
  const factory = util.createAddressFromString(FACTORY);
  const caller = util.createAddressFromString(CALLER);
  await state.putAccount(caller, new util.Account(0n, 10n ** 18n));
  await state.putAccount(factory, new util.Account(1n, 0n));
  await state.putCode(factory, util.hexToBytes('0x' + FACTORY_CODE));
  const evm = await createEVM({ common, stateManager: state });
  const created = protect ? localCreations(evm, state) : null;
  const result = await evm.runCall({
    caller, origin: caller, to: factory, data: util.hexToBytes('0x' + INIT), gasLimit: 1_000_000n,
  });
  assert.equal(result.execResult.exceptionError, undefined);
  const child = util.bytesToHex(result.execResult.returnValue).slice(-40);
  const expected = util.bytesToHex(util.generateAddress2(factory.bytes, util.hexToBytes('0x' + SALT), util.hexToBytes('0x' + INIT))).slice(2);
  assert.equal(child, expected);
  // The constructor's SSTORE is visible locally.
  const slot1 = new Uint8Array(32); slot1[31] = 1;
  assert.deepEqual(await state.getStorage(util.createAddressFromString('0x' + child), slot1), Uint8Array.from([42]));
  return { child, created };
}

test('without local creations the created address reaches the provider (the leak)', async () => {
  const fake = fakeProvider();
  try {
    const { child } = await run(false);
    assert.ok(fake.seen.some((body) => body.toLowerCase().includes(child)), 'control run must leak');
  } finally { fake.restore(); }
});

test('with local creations nothing about the created address leaves the process', async () => {
  const fake = fakeProvider();
  try {
    const { child, created } = await run(true);
    assert.ok(created.has(child));
    assert.equal(fake.seen.filter((body) => body.toLowerCase().includes(child)).length, 0, fake.seen.join('\n'));
  } finally { fake.restore(); }
});

test('the egress guard refuses private values and remote execution before sending', async () => {
  const fake = fakeProvider();
  const secret = 'ab'.repeat(32);
  const restore = guardEgress(['0x' + secret, FACTORY]);
  try {
    const post = (method, params) => globalThis.fetch(PROVIDER, {
      method: 'POST', body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }),
    });
    for (const [method, params] of [
      ['eth_getStorageAt', [FACTORY.toUpperCase().replace('0X', '0x'), '0x0', 'latest']],
      ['eth_getCode', ['0x' + 'c1'.repeat(20), '0x' + secret]],
      ['eth_call', [{ to: '0x' + 'c1'.repeat(20), data: '0x00' }, 'latest']],
      ['eth_estimateGas', [{ to: '0x' + 'c1'.repeat(20) }]],
    ]) {
      await assert.rejects(post(method, params), (e) => {
        assert.match(e.message, /^PRIVATE_EGRESS_REFUSED/);
        assert.ok(!e.message.toLowerCase().includes(secret));
        return true;
      });
    }
    await assert.rejects(globalThis.fetch(PROVIDER, { method: 'POST', body: Buffer.from('{}') }), /non-text/);
    assert.equal(fake.seen.length, 0);
    await post('eth_getCode', ['0x' + 'c1'.repeat(20), 'latest']);
    assert.equal(fake.seen.length, 1);
    assert.throws(() => guardEgress(['0x' + secret]), /twice/);
  } finally { restore(); fake.restore(); }
  assert.throws(() => guardEgress(['0x1234']), /at least 20 bytes/);
  assert.throws(() => guardEgress([]), /at least 20 bytes/);
});
