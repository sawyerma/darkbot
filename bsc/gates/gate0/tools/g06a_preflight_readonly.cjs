#!/usr/bin/env node
'use strict';

const fs = require('fs');
const crypto = require('crypto');

const CHAIN_ID = 56n;
const PORTAL = '0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0';
const TAX_V3_IMPL_EXPECTED = '0x024f18294970b5c76c0691b87f138a0317156422';

// keccak256 selectors, independently frozen from the canonical signatures.
const GET_QUOTE_CONFIG_SELECTOR = '0x26ef20d5'; // getQuoteTokenConfiguration(address)
const GET_FEE_RATE_SELECTOR = '0x84e5eed0';    // getFeeRate()
const TAX_ON_BC_SELECTOR = '0x1d31e153';       // enableTaxOnBondingCurve()

function req(name) {
  const v = process.env[name];
  if (!v) throw new Error(name + ' is required');
  return v.trim();
}

function requireRpcUrl() {
  const url = req('BSC_MAINNET_RPC_HTTPS');
  if (url === 'DEIN_RPC' || url.includes('DEIN_RPC')) {
    throw new Error('BSC_MAINNET_RPC_HTTPS still contains the placeholder DEIN_RPC; export your real BSC Mainnet HTTPS RPC URL');
  }
  if (!/^https:\/\//i.test(url)) {
    throw new Error('BSC_MAINNET_RPC_HTTPS must be a real https:// URL');
  }
  return url;
}

function normalizeAddress(value, label) {
  if (value == null || value === '') return null;
  const v = value.trim().toLowerCase();
  if (!/^0x[0-9a-f]{40}$/.test(v)) throw new Error(label + ' is not a valid 20-byte hex address');
  return v;
}

function sha256HexBytes(hex) {
  const raw = hex.startsWith('0x') ? hex.slice(2) : hex;
  if (raw.length % 2 !== 0 || !/^[0-9a-f]*$/i.test(raw)) throw new Error('invalid hex byte string');
  return crypto.createHash('sha256').update(Buffer.from(raw, 'hex')).digest('hex');
}

function decodeWords(hex, count, label) {
  if (typeof hex !== 'string' || !hex.startsWith('0x')) throw new Error(label + ': invalid hex result');
  const raw = hex.slice(2);
  if (raw.length < count * 64) throw new Error(label + ': short ABI result');
  const out = [];
  for (let i = 0; i < count; i++) out.push(BigInt('0x' + raw.slice(i * 64, (i + 1) * 64)));
  return out;
}

async function ethCall(url, to, data, block='latest') {
  return rpc(url, 'eth_call', [{ to, data }, block]);
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

async function main() {
  const outPath = process.argv[2] || 'bsc/gates/gate0/evidence/g06a_preflight_readonly.json';
  const url = requireRpcUrl();

  const chain = BigInt(await rpc(url, 'eth_chainId'));
  if (chain !== CHAIN_ID) throw new Error('wrong chain: expected 56, got ' + chain);

  const block = await rpc(url, 'eth_getBlockByNumber', ['latest', false]);
  if (!block || !block.number || !block.hash) throw new Error('latest block response incomplete');

  const portal = normalizeAddress(PORTAL, 'PORTAL');
  const impl = normalizeAddress(TAX_V3_IMPL_EXPECTED, 'TAX_V3_IMPL_EXPECTED');

  const portalCode = await rpc(url, 'eth_getCode', [portal, 'latest']);
  if (!portalCode || portalCode === '0x') throw new Error('portal has no code at latest');

  const implCode = await rpc(url, 'eth_getCode', [impl, 'latest']);
  if (!implCode || implCode === '0x') throw new Error('tax-v3 implementation has no code at latest');

  const gasPrice = BigInt(await rpc(url, 'eth_gasPrice'));

  // Bind the exact current native-BNB launch economics from the Portal itself.
  const quoteConfigRaw = await ethCall(
    url,
    portal,
    GET_QUOTE_CONFIG_SELECTOR + '0'.repeat(64)
  );
  const [quoteEnabled, defaultCurve, alternativeCurve, nativeToQuoteSwapType, quoteDexId] =
    decodeWords(quoteConfigRaw, 5, 'getQuoteTokenConfiguration(address(0))');

  const feeRaw = await ethCall(url, portal, GET_FEE_RATE_SELECTOR);
  const [buyFeeBps, sellFeeBps] = decodeWords(feeRaw, 2, 'getFeeRate()');

  const taxRaw = await ethCall(url, portal, TAX_ON_BC_SELECTOR);
  const [taxOnBondingCurveWord] = decodeWords(taxRaw, 1, 'enableTaxOnBondingCurve()');
  const taxOnBondingCurve = taxOnBondingCurveWord !== 0n;

  if (quoteEnabled !== 1n) throw new Error('native BNB quote token is not enabled');
  if (buyFeeBps >= 10000n || sellFeeBps >= 10000n) throw new Error('invalid protocol fee bps');

  const launcher = normalizeAddress(process.env.DARKBOT_G06A_LAUNCHER_ADDRESS || '', 'DARKBOT_G06A_LAUNCHER_ADDRESS');
  const buyer = normalizeAddress(process.env.DARKBOT_G06A_BUYER_ADDRESS || '', 'DARKBOT_G06A_BUYER_ADDRESS');
  if (launcher && buyer && launcher === buyer) throw new Error('launcher and buyer addresses must be distinct');

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

  const result = {
    schema: 'darkbot.g06a.preflight-readonly.v2',
    chain_id: chain.toString(),
    latest_block_number: BigInt(block.number).toString(),
    latest_block_hash: block.hash,
    portal,
    portal_code_bytes: (portalCode.length - 2) / 2,
    portal_code_sha256: sha256HexBytes(portalCode),
    tax_v3_impl: impl,
    tax_v3_impl_source: 'frozen BSC-mainnet deployment evidence; runtime code existence rechecked here',
    tax_v3_impl_code_bytes: (implCode.length - 2) / 2,
    tax_v3_impl_code_sha256: sha256HexBytes(implCode),
    gas_price_wei: gasPrice.toString(),
    native_quote_configuration: {
      enabled: quoteEnabled.toString(),
      default_curve_id: defaultCurve.toString(),
      alternative_curve_id: alternativeCurve.toString(),
      native_to_quote_swap_type: nativeToQuoteSwapType.toString(),
      dex_id: quoteDexId.toString()
    },
    protocol_fee_bps: {
      buy: buyFeeBps.toString(),
      sell: sellFeeBps.toString()
    },
    tax_on_bonding_curve_enabled: taxOnBondingCurve,
    launcher_address: launcher,
    launcher_pending_nonce: launcherNonce === null ? null : launcherNonce.toString(),
    launcher_balance_wei: launcherBalance === null ? null : launcherBalance.toString(),
    buyer_address: buyer,
    buyer_pending_nonce: buyerNonce === null ? null : buyerNonce.toString(),
    buyer_balance_wei: buyerBalance === null ? null : buyerBalance.toString(),
    network_actions: 'READ_ONLY_RPC_ONLY',
    signed_tx_submitted: false,
    bundle_submitted: false,
    dependencies: 'Node.js standard library only'
  };

  fs.writeFileSync(outPath, JSON.stringify(result, null, 2) + '\n', { mode: 0o600 });
  console.log(JSON.stringify(result, null, 2));
}

main().catch((e) => {
  console.error(e.stack || String(e));
  process.exitCode = 1;
});
