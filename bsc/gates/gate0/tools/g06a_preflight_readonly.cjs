#!/usr/bin/env node
'use strict';

const fs = require('fs');
const path = require('path');

const ROOT = path.resolve(__dirname, '../../..');
const ABI_TOOLS = path.join(ROOT, 'tests/legacy_gate0/gate0-evidence/abi-tools/node_modules');
const ethers = require(path.join(ABI_TOOLS, 'ethers'));

const CHAIN_ID = 56n;
const PORTAL = '0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0';
const TAX_V3_IMPL_EXPECTED = '0x024f18294970B5c76c0691b87f138A0317156422';

function req(name) {
  const v = process.env[name];
  if (!v) throw new Error(name + ' is required');
  return v;
}

async function rpc(url, method, params = []) {
  const res = await fetch(url, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }),
  });
  if (!res.ok) throw new Error('RPC HTTP ' + res.status + ' for ' + method);
  const body = await res.json();
  if (body.error) throw new Error('RPC ' + method + ': ' + JSON.stringify(body.error));
  return body.result;
}

function selector(sig) {
  return ethers.id(sig).slice(0, 10);
}

async function ethCall(url, to, data, block='latest') {
  return rpc(url, 'eth_call', [{to, data}, block]);
}

async function main() {
  const outPath = process.argv[2] || 'bsc/gates/gate0/evidence/g06a_preflight_readonly.json';
  const url = req('BSC_MAINNET_RPC_HTTPS');

  const chain = BigInt(await rpc(url, 'eth_chainId'));
  if (chain !== CHAIN_ID) throw new Error('wrong chain: ' + chain);

  const block = await rpc(url, 'eth_getBlockByNumber', ['latest', false]);
  const portalCode = await rpc(url, 'eth_getCode', [PORTAL, 'latest']);
  if (!portalCode || portalCode === '0x') throw new Error('portal has no code');

  const gasPrice = BigInt(await rpc(url, 'eth_gasPrice'));

  const launcher = process.env.DARKBOT_G06A_LAUNCHER_ADDRESS || null;
  const buyer = process.env.DARKBOT_G06A_BUYER_ADDRESS || null;
  let launcherNonce = null;
  let buyerNonce = null;
  let launcherBalance = null;
  let buyerBalance = null;

  if (launcher) {
    launcherNonce = BigInt(await rpc(url, 'eth_getTransactionCount', [launcher, 'pending']));
    launcherBalance = BigInt(await rpc(url, 'eth_getBalance', [launcher, 'latest']));
  }
  if (buyer) {
    buyerNonce = BigInt(await rpc(url, 'eth_getTransactionCount', [buyer, 'pending']));
    buyerBalance = BigInt(await rpc(url, 'eth_getBalance', [buyer, 'latest']));
  }

  let impl = null;
  const candidates = [
    'tokenImplTaxedV3()',
    'tokenImplTaxed()',
    'tokenImplTaxedV2()'
  ];
  for (const sig of candidates) {
    try {
      const ret = await ethCall(url, PORTAL, selector(sig));
      if (ret && ret.length >= 66) {
        const decoded = ethers.AbiCoder.defaultAbiCoder().decode(['address'], ret)[0];
        if (decoded && decoded !== ethers.ZeroAddress) {
          impl = ethers.getAddress(decoded);
          break;
        }
      }
    } catch {}
  }

  if (!impl) {
    impl = ethers.getAddress(TAX_V3_IMPL_EXPECTED);
  }

  const implCode = await rpc(url, 'eth_getCode', [impl, 'latest']);
  if (!implCode || implCode === '0x') throw new Error('tax-v3 impl has no code');

  const result = {
    schema: 'darkbot.g06a.preflight-readonly.v1',
    chain_id: chain.toString(),
    latest_block_number: BigInt(block.number).toString(),
    latest_block_hash: block.hash,
    portal: ethers.getAddress(PORTAL),
    portal_code_keccak256: ethers.keccak256(portalCode),
    tax_v3_impl: impl,
    tax_v3_impl_matches_expected: impl.toLowerCase() === TAX_V3_IMPL_EXPECTED.toLowerCase(),
    tax_v3_impl_code_keccak256: ethers.keccak256(implCode),
    gas_price_wei: gasPrice.toString(),
    launcher_address: launcher,
    launcher_pending_nonce: launcherNonce === null ? null : launcherNonce.toString(),
    launcher_balance_wei: launcherBalance === null ? null : launcherBalance.toString(),
    buyer_address: buyer,
    buyer_pending_nonce: buyerNonce === null ? null : buyerNonce.toString(),
    buyer_balance_wei: buyerBalance === null ? null : buyerBalance.toString(),
    network_actions: 'READ_ONLY_RPC_ONLY',
    signed_tx_submitted: false,
    bundle_submitted: false
  };

  fs.writeFileSync(outPath, JSON.stringify(result, null, 2) + '\n', { mode: 0o600 });
  console.log(JSON.stringify(result, null, 2));
}

main().catch((e) => {
  console.error(e.stack || String(e));
  process.exitCode = 1;
});
