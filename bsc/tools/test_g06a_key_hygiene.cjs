// BSC-007 (BSC_Bug_V2 P2-D5): the local simulation reads both wallet keys
// once, removes every key variable from its process environment before it
// parses them, and keeps the signers only until both transactions are
// signed. Synthetic test keys only, no network.
// Run: node --test bsc/tools/test_g06a_key_hygiene.cjs
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { takeKeys, requireSigningSwitches } = require('./g06a_local_simulate.cjs');

const KEY1 = '0x' + '0'.repeat(63) + '1';
const KEY2 = '0x' + '0'.repeat(63) + '2';
const NAMES = ['DARKBOT_G06A_LAUNCHER_PRIVATE_KEY', 'DARKBOT_G06A_BUYER_PRIVATE_KEY', 'BSC_LAUNCHER_PRIVATE_KEY', 'BSC_BUYER_PRIVATE_KEY'];

test('both keys are taken once and every key variable is gone, also on a missing key', () => {
  const env = { DARKBOT_G06A_LAUNCHER_PRIVATE_KEY: KEY1, DARKBOT_G06A_BUYER_PRIVATE_KEY: KEY2, BSC_LAUNCHER_PRIVATE_KEY: 'x', BSC_BUYER_PRIVATE_KEY: 'y', OTHER: 'kept' };
  assert.deepEqual(takeKeys(env), { launcher: KEY1, buyer: KEY2 });
  assert.deepEqual(Object.keys(env), ['OTHER']);
  const partial = { DARKBOT_G06A_LAUNCHER_PRIVATE_KEY: KEY1 };
  assert.throws(() => takeKeys(partial), /DARKBOT_G06A_BUYER_PRIVATE_KEY is required/);
  assert.deepEqual(Object.keys(partial), []);
  for (const name of NAMES) assert.ok(!(name in partial));
});

test('the simulation never reads a key elsewhere and drops its signers after signing', () => {
  const source = fs.readFileSync(path.join(__dirname, 'g06a_local_simulate.cjs'), 'utf8');
  const main = source.slice(source.indexOf('async function main()'));
  // The keys are taken before the first await (no network yet).
  assert.ok(main.indexOf('takeKeys(process.env)') < main.indexOf('await '));
  const take = source.slice(source.indexOf('function takeKeys('), source.indexOf('\n}\n', source.indexOf('function takeKeys(')));
  assert.ok(take.includes('delete env[name]'));
  assert.doesNotMatch(source.replace(take, ''), /PRIVATE_KEY/, 'key names only inside takeKeys');
  const signed = main.lastIndexOf('.signTransaction(');
  const dropped = main.indexOf('launcherWallet = buyerWallet = null;');
  assert.ok(signed > 0 && dropped > signed);
  assert.doesNotMatch(main.slice(dropped + 'launcherWallet = buyerWallet = null;'.length), /launcherWallet|buyerWallet/);
});

test('plan 27.10.: the native switches are asked after the keys left the environment, before any signer', () => {
  const source = fs.readFileSync(path.join(__dirname, 'g06a_local_simulate.cjs'), 'utf8');
  const main = source.slice(source.indexOf('async function main()'));
  const take = main.indexOf('takeKeys(process.env)');
  const ask = main.indexOf('requireSigningSwitches();');
  const wallet = main.indexOf('new ethers.Wallet(');
  assert.ok(take > 0 && take < ask && ask < wallet);
  const os = require('node:os');
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'g06a-switch-'));
  const fake = (code) => {
    const p = path.join(dir, `handoff-${code}`);
    // The stand-in records its argv and environment, then answers like the owner.
    fs.writeFileSync(p, `#!/bin/sh\nprintf '%s\\n' "$@" > ${dir}/argv-${code}\nenv > ${dir}/env-${code}\n[ ${code} = 0 ] && echo controls=ENABLED || echo "STOP: CONTROL_DISABLED at generation 0: BSC_ECON_SIGN DISABLED" >&2\nexit ${code}\n`);
    fs.chmodSync(p, 0o755);
    return p;
  };
  requireSigningSwitches(fake(0), '/home/x');
  assert.equal(fs.readFileSync(path.join(dir, 'argv-0'), 'utf8'), 'controls\n--require\nBSC_WALLET_LAUNCHER\nBSC_WALLET_BUYER\nBSC_ECON_SIGN\n');
  const env = fs.readFileSync(path.join(dir, 'env-0'), 'utf8');
  assert.match(env, /^HOME=\/home\/x$/m);
  assert.doesNotMatch(env, /PRIVATE_KEY|SALT/);
  assert.throws(() => requireSigningSwitches(fake(2), '/home/x'), /native switches refuse the signature: STOP: CONTROL_DISABLED at generation 0: BSC_ECON_SIGN DISABLED/);
  assert.throws(() => requireSigningSwitches(path.join(dir, 'missing'), '/home/x'), /native switches refuse the signature/);
  fs.rmSync(dir, { recursive: true });
});
