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

// D1 (v9.1): the real cross-check in --private mode against a recording
// mock JSON-RPC server. The Portal stand-in is zero code, so the local EVM
// launch returns nothing and the tool stops there; everything it sent up to
// that point is checked: no remote execution method and no salt, launch
// calldata or token in any request.
test('the private cross-check sends no remote execution and no private value', async () => {
  const http = require('node:http');
  const { spawn } = require('node:child_process');
  const fs = require('node:fs');
  const os = require('node:os');
  const ethers = require(path.join(__dirname, 'abi-tools/node_modules/ethers'));
  const dbrec = require('./dbrec.cjs');
  // Synthetic, test-only: keccak256("darkbot-d1-test-only-salt:130983").
  const salt = ethers.keccak256(ethers.toUtf8Bytes('darkbot-d1-test-only-salt:130983'));
  const init = ethers.concat(['0x3d602d80600a3d3981f3363d3d373d3d3d363d73', '0x024f18294970b5c76c0691b87f138a0317156422', '0x5af43d82803e903d91602b57fd5bf3']);
  const token = ethers.getCreate2Address('0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0', salt, ethers.keccak256(init)).toLowerCase();
  assert.ok(token.endsWith('7777'));
  const preflightPath = path.join(__dirname, '../native/evidence/g06a/g07_binary_import_20261016/preflight.dbrec');
  const pre = dbrec.decode('darkbot.g06a.preflight-readonly.v3', fs.readFileSync(preflightPath)).fields;
  const zeros = (n) => '0x' + '00'.repeat(Number(n));
  const seen = [];
  const server = http.createServer((req, res) => {
    let body = '';
    req.on('data', (c) => { body += c; });
    req.on('end', () => {
      seen.push(body);
      const { id, method, params } = JSON.parse(body);
      const word = '0x' + '00'.repeat(32);
      let result;
      if (method === 'eth_chainId') result = '0x38';
      else if (method === 'eth_getBlockByNumber') {
        result = { number: '0x' + pre.latest_block_number.toString(16), hash: pre.latest_block_hash, miner: '0x' + '00'.repeat(20), timestamp: '0x1', difficulty: '0x0', gasLimit: '0x8000000', baseFeePerGas: '0x0', mixHash: word };
      } else if (method === 'eth_getCode') {
        const a = String(params[0]).toLowerCase();
        result = a === pre.portal ? zeros(pre.portal_code_bytes) : a === pre.tax_v3_impl ? zeros(pre.tax_v3_impl_code_bytes) : '0x';
      } else if (method === 'eth_getBalance' || method === 'eth_getTransactionCount') result = '0x0';
      else if (method === 'eth_getStorageAt') result = word;
      else result = null;
      res.setHeader('content-type', 'application/json');
      res.end(JSON.stringify({ jsonrpc: '2.0', id, result }));
    });
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const out = path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'd1-private-')), 'econ.dbrec');
  try {
    const child = spawn(process.execPath, [path.join(__dirname, 'g06a_economics_evm_crosscheck.cjs'), preflightPath, out, '--private'], {
      env: {
        PATH: process.env.PATH, BSC_MAINNET_RPC_HTTPS: `http://127.0.0.1:${server.address().port}`,
        DARKBOT_G06A_SALT: salt, DARKBOT_G06A_LAUNCH_GAS_LIMIT: '6603270', DARKBOT_G06A_BUY_VALUE_WEI: '1000000000000',
      },
    });
    let stderr = '';
    let stdout = '';
    child.stderr.on('data', (c) => { stderr += c; });
    child.stdout.on('data', (c) => { stdout += c; });
    const code = await new Promise((r) => child.on('exit', r));
    assert.notEqual(code, 0);
    assert.match(stderr, /did not return the predicted CREATE2 token/);
    assert.doesNotMatch(stderr, /PRIVATE_EGRESS_REFUSED/);
    assert.ok(seen.some((b) => b.includes('eth_getStorageAt') || b.includes('eth_getBalance')), 'the local EVM read state');
    const calldataSalt = salt.slice(2).toLowerCase();
    for (const body of seen) {
      const { method } = JSON.parse(body);
      assert.ok(!['eth_call', 'eth_estimateGas', 'eth_createAccessList', 'eth_sendRawTransaction'].includes(method), method);
      assert.ok(!body.toLowerCase().includes(calldataSalt), 'salt or launch calldata sent');
      assert.ok(!body.toLowerCase().includes(token.slice(2)), 'token sent');
    }
    assert.ok(!(stdout + stderr).toLowerCase().includes(calldataSalt), 'salt printed');
    assert.ok(!fs.existsSync(out));
  } finally {
    server.close();
  }
});
