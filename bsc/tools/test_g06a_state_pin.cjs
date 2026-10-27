// BSC-002 (BSC_Bug_V2 P2-D2): every local EVM run of the DRY reads its state
// from exactly the preflight's canonical block. A recording mock JSON-RPC
// server stands in for the archive endpoint; synthetic test keys only, no
// network, nothing sent. Run: node --test bsc/tools/test_g06a_state_pin.cjs
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const http = require('node:http');
const path = require('node:path');
const fs = require('node:fs');
const os = require('node:os');
const { spawn } = require('node:child_process');
const { pin, pinState, requireBlock } = require('./g06a_state_pin.cjs');

const PREFLIGHT_HASH = '0x' + 'a1'.repeat(32);
const OTHER_HASH = '0x' + 'b2'.repeat(32);
const HEAD = 129392405;
// Synthetic test-only keys (secp256k1 scalars 1 and 2).
const KEY1 = '0x' + '0'.repeat(63) + '1';
const KEY2 = '0x' + '0'.repeat(63) + '2';

/** A mock endpoint: the block at HEAD has `hash`; every request is kept. */
async function mock(hash) {
  const seen = [];
  const server = http.createServer((req, res) => {
    let body = '';
    req.on('data', (c) => { body += c; });
    req.on('end', () => {
      seen.push(JSON.parse(body));
      const { id, method } = JSON.parse(body);
      const word = '0x' + '00'.repeat(32);
      const result = {
        eth_chainId: '0x38',
        eth_getBlockByNumber: { number: '0x' + HEAD.toString(16), hash, miner: '0x' + '00'.repeat(20), timestamp: '0x1',
          difficulty: '0x0', gasLimit: '0x8000000', baseFeePerGas: '0x0', mixHash: word },
        eth_getBalance: '0x0', eth_getTransactionCount: '0x0', eth_getCode: '0x', eth_getStorageAt: word,
      }[method] ?? null;
      res.setHeader('content-type', 'application/json');
      res.end(JSON.stringify({ jsonrpc: '2.0', id, result }));
    });
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  return { url: `http://127.0.0.1:${server.address().port}`, seen, close: () => server.close() };
}

function run(script, args, env) {
  return new Promise((resolve) => {
    const child = spawn(process.execPath, [path.join(__dirname, script), ...args], { env: { PATH: process.env.PATH, ...env } });
    let out = '';
    child.stdout.on('data', (c) => { out += c; });
    child.stderr.on('data', (c) => { out += c; });
    child.on('exit', (code) => resolve({ code, out }));
  });
}

const STATE_METHODS = ['eth_getStorageAt', 'eth_getCode', 'eth_getBalance', 'eth_getTransactionCount', 'eth_getProof'];

test('local simulation refuses another block than the preflight before any state read', async () => {
  const m = await mock(OTHER_HASH);
  const out = path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'bsc002-')), 'sim.dbrec');
  try {
    const r = await run('g06a_local_simulate.cjs', [out], {
      BSC_MAINNET_RPC_HTTPS: m.url, DARKBOT_G06A_LAUNCHER_PRIVATE_KEY: KEY1, DARKBOT_G06A_BUYER_PRIVATE_KEY: KEY2,
      DARKBOT_G06A_TAX_V3_IMPL: '0x024f18294970b5c76c0691b87f138a0317156422', DARKBOT_G06A_SALT: '0x' + '5a'.repeat(32),
      DARKBOT_G06A_LAUNCHER_NONCE: '0', DARKBOT_G06A_BUYER_NONCE: '0', DARKBOT_G06A_GAS_PRICE_WEI: '50000000',
      DARKBOT_G06A_LAUNCH_GAS_LIMIT: '6603270', DARKBOT_G06A_BUY_GAS_LIMIT: '3000000', DARKBOT_G06A_LAUNCH_VALUE_WEI: '0',
      DARKBOT_G06A_BUY_VALUE_WEI: '1000000000000', DARKBOT_G06A_LOCAL_EXPECTED_OUTPUT: '176693134169499757962',
      DARKBOT_G06A_SEND_HEAD: String(HEAD), DARKBOT_G06A_SEND_HEAD_HASH: PREFLIGHT_HASH,
      DARKBOT_G06A_STRICTEST_BUILDER_FLOOR_WEI: '50000000', DARKBOT_G06A_BUILDER_FLOOR_EVIDENCE_SHA256: 'ab'.repeat(32),
    });
    assert.notEqual(r.code, 0, r.out);
    // Plan 27.10.: with the native signing switches DISABLED (their default,
    // and the owner state every test runs in) the simulation stops before any
    // signature and before any request; only with them ENABLED in this boot
    // does it reach the BSC-002 pin and refuse the other block there.
    if (/native switches refuse the signature: STOP: CONTROL_DISABLED/.test(r.out)) {
      assert.equal(m.seen.length, 0, JSON.stringify(m.seen.map((c) => c.method)));
    } else {
      assert.match(r.out, /STATE_BLOCK_MISMATCH/);
    }
    assert.equal(m.seen.filter((c) => STATE_METHODS.includes(c.method)).length, 0, JSON.stringify(m.seen.map((c) => c.method)));
    assert.ok(!fs.existsSync(out));
  } finally { m.close(); }
});

test('the simulation pins the preflight block before its first state read', () => {
  const source = fs.readFileSync(path.join(__dirname, 'g06a_local_simulate.cjs'), 'utf8');
  const main = source.slice(source.indexOf('async function main()'));
  const pinned = main.indexOf("await requireBlock(rpc, rpcUrl, parentNumber, req('DARKBOT_G06A_SEND_HEAD_HASH'), 'before the simulation');");
  assert.ok(pinned > 0, 'the BSC-002 pin before the simulation');
  assert.ok(pinned < main.indexOf('new RPCStateManager('));
  assert.ok(pinned < main.indexOf('installBasicAccountLoader('));
  assert.ok(main.indexOf('requireSigningSwitches();') < main.indexOf('.signTransaction('));
});

test('the cross-check reads every state word pinned to the preflight block hash', async () => {
  const dbrec = require('./dbrec.cjs');
  const ethers = require(path.join(__dirname, 'abi-tools/node_modules/ethers'));
  const preflightPath = path.join(__dirname, '../native/evidence/g06a/g07_binary_import_20261016/preflight.dbrec');
  const pre = dbrec.decode('darkbot.g06a.preflight-readonly.v3', fs.readFileSync(preflightPath)).fields;
  // The real cross-check needs the real Portal: zero-code stand-ins make the
  // local launch fail after the state reads, which is all this test needs.
  const seen = [];
  const server = http.createServer((req, res) => {
    let body = '';
    req.on('data', (c) => { body += c; });
    req.on('end', () => {
      const call = JSON.parse(body);
      seen.push(call);
      const word = '0x' + '00'.repeat(32);
      const zeros = (n) => '0x' + '00'.repeat(Number(n));
      let result = null;
      if (call.method === 'eth_chainId') result = '0x38';
      else if (call.method === 'eth_getBlockByNumber') result = { number: '0x' + pre.latest_block_number.toString(16), hash: pre.latest_block_hash, miner: '0x' + '00'.repeat(20), timestamp: '0x1', difficulty: '0x0', gasLimit: '0x8000000', baseFeePerGas: '0x0', mixHash: word };
      else if (call.method === 'eth_getCode') { const a = String(call.params[0]).toLowerCase(); result = a === pre.portal ? zeros(pre.portal_code_bytes) : a === pre.tax_v3_impl ? zeros(pre.tax_v3_impl_code_bytes) : '0x'; }
      else if (['eth_getBalance', 'eth_getTransactionCount'].includes(call.method)) result = '0x0';
      else if (call.method === 'eth_getStorageAt') result = word;
      res.setHeader('content-type', 'application/json');
      res.end(JSON.stringify({ jsonrpc: '2.0', id: call.id, result }));
    });
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const salt = ethers.keccak256(ethers.toUtf8Bytes('darkbot-d1-test-only-salt:130983'));
  const out = path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'bsc002-')), 'econ.dbrec');
  try {
    const r = await run('g06a_economics_evm_crosscheck.cjs', [preflightPath, out, '--private'], {
      BSC_MAINNET_RPC_HTTPS: `http://127.0.0.1:${server.address().port}`, DARKBOT_G06A_SALT: salt,
      DARKBOT_G06A_LAUNCH_GAS_LIMIT: '6603270', DARKBOT_G06A_BUY_VALUE_WEI: '1000000000000',
    });
    assert.notEqual(r.code, 0);
    const state = seen.filter((c) => STATE_METHODS.includes(c.method));
    assert.ok(state.length > 0);
    for (const call of state) {
      const tag = call.params[call.params.length - 1];
      assert.deepEqual(tag, { blockHash: pre.latest_block_hash.toLowerCase(), requireCanonical: true }, JSON.stringify(call));
    }
  } finally { server.close(); }
});

test('the state pin and the canonical re-check refuse another block', async () => {
  assert.deepEqual(pin(PREFLIGHT_HASH.toUpperCase().replace('0X', '0x')), { blockHash: PREFLIGHT_HASH, requireCanonical: true });
  assert.throws(() => pin('latest'));
  assert.throws(() => pin(HEAD));
  const state = {};
  pinState(state, PREFLIGHT_HASH);
  assert.deepEqual(state._blockTag, { blockHash: PREFLIGHT_HASH, requireCanonical: true });
  const rpc = (hash) => async () => ({ hash });
  await requireBlock(rpc(PREFLIGHT_HASH), 'u', HEAD, PREFLIGHT_HASH, 'test');
  await assert.rejects(requireBlock(rpc(OTHER_HASH), 'u', HEAD, PREFLIGHT_HASH, 'after'), /STATE_BLOCK_MISMATCH after/);
  await assert.rejects(requireBlock(async () => null, 'u', HEAD, PREFLIGHT_HASH, 'gone'), /STATE_BLOCK_MISMATCH/);
});

test('every local EVM tool re-checks its block right before writing evidence', () => {
  for (const tool of ['g06a_local_simulate.cjs', 'g06a_economics_evm_crosscheck.cjs', 'g06a_launch_path_trace.cjs']) {
    const source = fs.readFileSync(path.join(__dirname, tool), 'utf8');
    const writes = source.split('dbrec.writeNew(').length - 1;
    assert.ok(writes >= 1, tool);
    // Each evidence write is immediately preceded by the canonical re-check.
    for (const chunk of source.split('dbrec.writeNew(').slice(0, -1)) {
      const tail = chunk.slice(-400);
      assert.match(tail, /await requireBlock\([^;]*'before the evidence write'\);/, tool);
    }
    assert.match(source, /pinState\(state, /, tool);
    assert.doesNotMatch(source, /installBasicAccountLoader\(state, rpcUrl, (parentTag|tag|'0x')/, tool);
  }
});
