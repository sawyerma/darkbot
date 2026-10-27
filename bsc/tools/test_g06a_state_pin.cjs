// BSC-002 (BSC_Bug_V2 P2-D2): every local EVM run of the DRY reads its state
// from exactly the preflight's canonical block. A recording mock JSON-RPC
// server stands in for the archive endpoint; synthetic test keys only, no
// network, nothing sent. Run: node --test bsc/tools/test_g06a_state_pin.cjs
// The native switch checks run only against isolated owner states (the
// diagnostic `bsc_runtime control require --state`, or a foreign HOME the
// owner refuses); the account's own switches are never read or changed here.
// Needs the debug `bsc_runtime` and `g06a_dry_handoff`; cargo's `bsc_tools`
// test runs this file and checks both against the current build.
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const http = require('node:http');
const path = require('node:path');
const fs = require('node:fs');
const os = require('node:os');
const Module = require('node:module');
const { spawn, spawnSync } = require('node:child_process');
const { pin, pinState, requireBlock } = require('./g06a_state_pin.cjs');

const PREFLIGHT_HASH = '0x' + 'a1'.repeat(32);
const OTHER_HASH = '0x' + 'b2'.repeat(32);
const HEAD = 129392405;
// Synthetic test-only keys (secp256k1 scalars 1 and 2) and their addresses.
const KEY1 = '0x' + '0'.repeat(63) + '1';
const KEY2 = '0x' + '0'.repeat(63) + '2';
const ADDRESS1 = '0x7e5f4552091a69125d5dfcb7b8c2659029395bdf';
const ADDRESS2 = '0x2b5ad5c4795c026514f8317c7a215e218dccd6cf';
const SIMULATOR = path.join(__dirname, 'g06a_local_simulate.cjs');
const PIN_MODULE = path.join(__dirname, 'g06a_state_pin.cjs');
const BSC_RUNTIME = path.join(__dirname, '../engine/target/debug/bsc_runtime');
const SIGNING_SWITCHES = ['BSC_WALLET_LAUNCHER', 'BSC_WALLET_BUYER', 'BSC_ECON_SIGN'];
const STATE_METHODS = ['eth_getStorageAt', 'eth_getCode', 'eth_getBalance', 'eth_getTransactionCount', 'eth_getProof'];

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

/** The values main() prepares before it simulates (synthetic wallets, frozen candidate). */
function prepared(rpcUrl) {
  return {
    rpcUrl, parentNumber: BigInt(HEAD), parentHash: PREFLIGHT_HASH,
    portal: '0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0', token: '0x' + '7a'.repeat(20),
    launcherAddress: ADDRESS1, buyerAddress: ADDRESS2, launcherNonce: 0n, buyerNonce: 0n,
    gasPrice: 50000000n, launchGasLimit: 6603270n, buyGasLimit: 3000000n,
    launchValue: 0n, buyValue: 1000000000000n, launchData: '0x', buyData: '0x',
  };
}

/** One run of a simulation path against an endpoint whose block at HEAD is another one. */
async function atAnotherBlock(simulatePrepared) {
  const m = await mock(OTHER_HASH);
  try {
    let error = null;
    try { await simulatePrepared(prepared(m.url)); } catch (e) { error = e; }
    return { error, methods: m.seen.map((c) => c.method) };
  } finally { m.close(); }
}

/** BSC-002 holds: refused with STATE_BLOCK_MISMATCH only, before any state read. */
function pinHolds({ error, methods }) {
  return error instanceof Error && /^STATE_BLOCK_MISMATCH before the simulation: /.test(error.message)
    && !methods.some((x) => STATE_METHODS.includes(x));
}

/** A module compiled in memory from `source` under its own path (its requires resolve as usual). */
function compiled(file, source) {
  const m = new Module(file, module);
  m.filename = file;
  m.paths = Module._nodeModulePaths(path.dirname(file));
  m._compile(source, file);
  m.loaded = true;
  return m;
}

/** A top-level function's text: from its header to its closing brace in column 0. */
function body(source, header) {
  const start = source.indexOf(header);
  assert.ok(start >= 0, header);
  return source.slice(start, source.indexOf('\n}\n', start) + 2);
}

function bscRuntime(args) {
  assert.ok(fs.existsSync(BSC_RUNTIME), 'build bsc_runtime (cargo, --features runtime) first');
  return spawnSync(BSC_RUNTIME, args, { encoding: 'utf8', env: { PATH: '/usr/bin:/bin' } });
}

/** An isolated owner state in `state` with `ids` ENABLED (this boot session); its generation. */
function isolatedOwner(state, ids) {
  const init = bscRuntime(['source', 'init', '--state', state]);
  assert.equal(init.status, 0, init.stdout + init.stderr);
  ids.forEach((id, g) => {
    const r = bscRuntime(['source', 'request', id, 'ENABLED', '--expect-generation', String(g), '--request-id', `t-${g}`, '--state', state]);
    assert.match(r.stdout, /^outcome=APPLIED /m, r.stdout + r.stderr);
  });
  return ids.length;
}

test('A: another block is refused with STATE_BLOCK_MISMATCH only, before any state read and without evidence', async () => {
  const { simulatePrepared } = require(SIMULATOR);
  const r = await atAnotherBlock(simulatePrepared);
  assert.ok(pinHolds(r), `${r.error && r.error.stack}\n${JSON.stringify(r.methods)}`);
  assert.deepEqual(r.methods, ['eth_chainId', 'eth_getBlockByNumber']);
  // Control: at the preflight's own block the same path does read state, so
  // the zero above is the pin's work, not an idle mock.
  const m = await mock(PREFLIGHT_HASH);
  try {
    await assert.rejects(simulatePrepared(prepared(m.url)), (e) => !/STATE_BLOCK_MISMATCH/.test(e.message));
    assert.ok(m.seen.some((c) => STATE_METHODS.includes(c.method)), JSON.stringify(m.seen.map((c) => c.method)));
  } finally { m.close(); }
  // No evidence: the simulation path has no writer at all (E).
});

test('A mutation: a removed or ineffective block check turns A red', async () => {
  const source = fs.readFileSync(SIMULATOR, 'utf8');
  const PIN = "const parent = await requireBlock(rpc, rpcUrl, parentNumber, parentHash, 'before the simulation');";
  const HEADER = "await rpc(rpcUrl, 'eth_getBlockByNumber', ['0x' + BigInt(parentNumber).toString(16), false])";
  assert.equal(source.split(PIN).length, 2, 'the pin line the mutants replace');
  const mutants = {
    'block check removed': source.replace(PIN, `const parent = ${HEADER};`),
    'block compared with itself': source.replace(PIN, `const parent = await requireBlock(rpc, rpcUrl, parentNumber, (${HEADER}).hash, 'before the simulation');`),
  };
  for (const [name, text] of Object.entries(mutants)) {
    const r = await atAnotherBlock(compiled(SIMULATOR, text).exports.simulatePrepared);
    assert.ok(!pinHolds(r), `${name}: A would stay green`);
    assert.ok(r.methods.some((x) => STATE_METHODS.includes(x)), `${name}: the mutant reads state at another block`);
  }
  // The shared check itself made ineffective (no hash comparison).
  const pinSource = fs.readFileSync(PIN_MODULE, 'utf8');
  const CHECK = 'if (seen !== String(hash).toLowerCase()) {';
  assert.equal(pinSource.split(CHECK).length, 2, 'the comparison the mutant removes');
  const saved = require.cache[PIN_MODULE];
  require.cache[PIN_MODULE] = compiled(PIN_MODULE, pinSource.replace(CHECK, 'if (false) {'));
  let simulatePrepared;
  try {
    simulatePrepared = compiled(SIMULATOR, source).exports.simulatePrepared;
  } finally {
    require.cache[PIN_MODULE] = saved;
  }
  const r = await atAnotherBlock(simulatePrepared);
  assert.ok(!pinHolds(r), 'ineffective requireBlock: A would stay green');
  assert.ok(r.methods.some((x) => STATE_METHODS.includes(x)));
});

test('B: the native owner refuses DISABLED signing switches in an isolated state (diagnostic interface)', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'bsc002-owner-'));
  const state = path.join(dir, 'bsc-sources');
  try {
    assert.equal(bscRuntime(['source', 'init', '--state', state]).status, 0);
    const off = bscRuntime(['control', 'require', '--state', state, ...SIGNING_SWITCHES]);
    assert.notEqual(off.status, 0, off.stdout + off.stderr);
    assert.match(off.stdout, /^controls=REFUSED CONTROL_DISABLED at generation 0: BSC_WALLET_LAUNCHER DISABLED, BSC_WALLET_BUYER DISABLED, BSC_ECON_SIGN DISABLED$/m);
    // The same check passes once they are ENABLED there, and names exactly the one switched back.
    fs.rmSync(state, { recursive: true, force: true });
    const g = isolatedOwner(state, SIGNING_SWITCHES);
    const on = bscRuntime(['control', 'require', '--state', state, ...SIGNING_SWITCHES]);
    assert.equal(on.status, 0, on.stdout + on.stderr);
    assert.match(on.stdout, new RegExp(`^controls=ENABLED generation=${g}$`, 'm'));
    const back = bscRuntime(['source', 'request', 'BSC_ECON_SIGN', 'DISABLED', '--expect-generation', String(g), '--request-id', 't-off', '--state', state]);
    assert.match(back.stdout, /^outcome=APPLIED /m, back.stdout + back.stderr);
    const one = bscRuntime(['control', 'require', '--state', state, ...SIGNING_SWITCHES]);
    assert.notEqual(one.status, 0);
    assert.match(one.stdout, new RegExp(`^controls=REFUSED CONTROL_DISABLED at generation ${g + 1}: BSC_ECON_SIGN DISABLED$`, 'm'));
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('C: a foreign HOME with ENABLED switches is refused before any signature, request or record', async () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'bsc002-home-'));
  const state = path.join(home, '.darkbot/run/bsc-sources');
  const out = path.join(home, 'sim.dbrec');
  const m = await mock(PREFLIGHT_HASH);
  try {
    fs.mkdirSync(path.dirname(state), { recursive: true });
    isolatedOwner(state, SIGNING_SWITCHES);
    assert.equal(bscRuntime(['control', 'require', '--state', state, ...SIGNING_SWITCHES]).status, 0, 'ENABLED under that HOME');
    const r = await run('g06a_local_simulate.cjs', [out], {
      HOME: home, BSC_MAINNET_RPC_HTTPS: m.url, DARKBOT_G06A_LAUNCHER_PRIVATE_KEY: KEY1, DARKBOT_G06A_BUYER_PRIVATE_KEY: KEY2,
      DARKBOT_G06A_TAX_V3_IMPL: '0x024f18294970b5c76c0691b87f138a0317156422', DARKBOT_G06A_SALT: '0x' + '5a'.repeat(32),
      DARKBOT_G06A_LAUNCHER_NONCE: '0', DARKBOT_G06A_BUYER_NONCE: '0', DARKBOT_G06A_GAS_PRICE_WEI: '50000000',
      DARKBOT_G06A_LAUNCH_GAS_LIMIT: '6603270', DARKBOT_G06A_BUY_GAS_LIMIT: '3000000', DARKBOT_G06A_LAUNCH_VALUE_WEI: '0',
      DARKBOT_G06A_BUY_VALUE_WEI: '1000000000000', DARKBOT_G06A_LOCAL_EXPECTED_OUTPUT: '176693134169499757962',
      DARKBOT_G06A_SEND_HEAD: String(HEAD), DARKBOT_G06A_SEND_HEAD_HASH: PREFLIGHT_HASH,
      DARKBOT_G06A_STRICTEST_BUILDER_FLOOR_WEI: '50000000', DARKBOT_G06A_BUILDER_FLOOR_EVIDENCE_SHA256: 'ab'.repeat(32),
    });
    assert.notEqual(r.code, 0, r.out);
    assert.match(r.out, /native switches refuse the signature: STOP: CONTROL_STATE_UNREADABLE: HOME is not the account home /);
    // Refused inside the check that precedes every signer (D).
    assert.match(r.out, /\n\s+at requireSigningSwitches \(/);
    assert.equal(m.seen.length, 0, JSON.stringify(m.seen.map((c) => c.method)));
    assert.ok(!fs.existsSync(out));
    assert.doesNotMatch(r.out, /LAUNCH_TX_HASH|LOCAL_SIMULATION=WRITTEN/);
  } finally {
    m.close();
    fs.rmSync(home, { recursive: true, force: true });
  }
});

test('D: main asks the native switches after taking the keys and before any signer; the pin precedes every state read', () => {
  const source = fs.readFileSync(SIMULATOR, 'utf8');
  const main = body(source, 'async function main()');
  const simulation = body(source, 'async function simulatePrepared(p)');
  const at = (text, needle) => { const i = text.indexOf(needle); assert.ok(i >= 0, needle); return i; };
  assert.ok(at(main, 'takeKeys(process.env)') < at(main, 'requireSigningSwitches();'));
  assert.ok(at(main, 'requireSigningSwitches();') < at(main, 'new ethers.Wallet('));
  assert.ok(at(main, 'new ethers.Wallet(') < at(main, '.signTransaction('));
  const pinned = at(simulation, "await requireBlock(rpc, rpcUrl, parentNumber, parentHash, 'before the simulation');");
  assert.ok(at(simulation, "rpc(rpcUrl, 'eth_chainId')") < pinned);
  for (const read of ['new RPCStateManager(', 'installBasicAccountLoader(', 'createEVM(', '.runCall(', "'eth_getTransactionCount'"]) {
    assert.ok(pinned < at(simulation, read), read);
  }
});

test('E: main() simulates only through simulatePrepared, which never signs, sends, switches or writes; evidence comes last', () => {
  const source = fs.readFileSync(SIMULATOR, 'utf8');
  const main = body(source, 'async function main()');
  const simulation = body(source, 'async function simulatePrepared(p)');
  // One simulation path: defined once, called once, by main().
  assert.equal(source.split('simulatePrepared(').length - 1, 2);
  assert.equal(main.split('await simulatePrepared(').length - 1, 1);
  for (const own of ['new RPCStateManager(', 'new RPCBlockChain(', 'createEVM(', '.runCall(', 'installBasicAccountLoader(', 'pinState(', "'before the simulation'"]) {
    assert.ok(!main.includes(own), own);
  }
  for (const never of ['signTransaction', 'Wallet(', 'writeNew', 'dbrec.', 'child_process', 'execFile', 'spawn', 'requireSigningSwitches',
    'takeKeys', 'process.env', 'PRIVATE_KEY', 'eth_sendRawTransaction', 'eth_sendBundle', 'sendTransaction']) {
    assert.ok(!simulation.includes(never), never);
  }
  // Evidence: written once, by main(), after the switch check, the signature, the simulation and the closing pin.
  assert.equal(source.split('dbrec.writeNew(').length - 1, 1);
  const write = main.indexOf('dbrec.writeNew(');
  assert.ok(write > 0);
  for (const before of ['requireSigningSwitches();', '.signTransaction(', 'await simulatePrepared(', "'before the evidence write');"]) {
    const i = main.indexOf(before);
    assert.ok(i >= 0 && i < write, before);
  }
  // The native check is this checkout's debug owner binary, never one the environment names.
  assert.match(source, /function requireSigningSwitches\(handoff = path\.join\(__dirname, '\.\.\/engine\/target\/debug\/g06a_dry_handoff'\), home = process\.env\.HOME\)/);
  assert.equal(main.split('requireSigningSwitches(').length - 1, 1);
  assert.ok(main.includes('requireSigningSwitches();'));
  assert.doesNotMatch(source, /process\.env\.[A-Z0-9_]*(HANDOFF|BIN|BINARY|EXE)\b/);
  assert.match(source, /\nif \(require\.main === module\) \{\n  main\(\)/);
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
