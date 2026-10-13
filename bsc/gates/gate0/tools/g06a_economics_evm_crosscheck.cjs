#!/usr/bin/env node
// G0.6a economics-only independent local sequential EVM simulation.
//
// Purpose:
//   Prove current deployed Mainnet semantics for:
//     newTokenV6(native BNB, quoteAmt=0, TOKEN_TAXED_V3)
//       -> direct swapExactInput(native BNB -> fresh token)
//
// Independence:
//   - Does NOT import or call the Rust quote implementation.
//   - Does NOT accept Rust expectedOutput/minOutput as input.
//   - BUY minOutput is deliberately 1, so the EVM determines the output.
//   - Uses the actual deployed Portal/TAX_V3 code and storage through read-only RPC state.
//
// Safety:
//   - No signing.
//   - No transaction/bundle construction.
//   - No eth_sendRawTransaction / builder call.
//   - Network activity is read-only RPC state/header/code access only.

'use strict';

const fs = require('fs');
const path = require('path');

const ROOT = path.resolve(__dirname, '../../..');
const LEGACY_TOOLS = path.join(ROOT, 'tests/legacy_gate0/gate0-evidence/evm-audit-tools/node_modules');
const ABI_TOOLS = path.join(ROOT, 'tests/legacy_gate0/gate0-evidence/abi-tools/node_modules');

const { createEVM } = require(path.join(LEGACY_TOOLS, '@ethereumjs/evm'));
const { createCustomCommon, Mainnet, Hardfork } = require(path.join(LEGACY_TOOLS, '@ethereumjs/common'));
const { RPCStateManager, RPCBlockChain } = require(path.join(LEGACY_TOOLS, '@ethereumjs/statemanager'));
const {
  Account,
  KECCAK256_RLP,
  createAccount,
  createAddressFromString,
  hexToBytes,
  bytesToHex,
} = require(path.join(LEGACY_TOOLS, '@ethereumjs/util'));
const ethers = require(path.join(ABI_TOOLS, 'ethers'));

const CHAIN_ID = 56n;
const PORTAL = '0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0';
const TAX_V3_IMPL = '0x024f18294970b5c76c0691b87f138a0317156422';
const DEFAULT_PREFLIGHT_PATH = path.join(__dirname, '../evidence/g06a_preflight_readonly.json');

const BUY_VALUE = BigInt(process.env.DARKBOT_G06A_BUY_VALUE_WEI || '1000000000000000');
// Optional bounded in-process repeat protocol for G07. One launch,
// checkpoint/revert every BUY against the identical post-launch state.
// The normal one-value G0.6 crosscheck remains unchanged.
const BATCH_RAW = process.env.DARKBOT_G06A_BUY_VALUES_WEI || '';
const BUY_VALUES = BATCH_RAW ? BATCH_RAW.split(',').map(v => BigInt(v)) : [BUY_VALUE];
if (BUY_VALUES.length > 8 || BUY_VALUES.length === 0 ||
    BUY_VALUES.some(v => v <= 0n || v > 1000000000000000n) || BUY_VALUES[0] !== BUY_VALUE) {
  throw new Error('invalid G07 BUY value batch');
}
const LAUNCH_VALUE = 0n;
const GAS_PRICE = BigInt(process.env.DARKBOT_G06A_GAS_PRICE_WEI || '50000000');
const LAUNCH_GAS_LIMIT = 5_000_000n;
const BUY_GAS_LIMIT = 3_000_000n;

// Deterministic local-only callers. They are overlaid with local balances below.
const LAUNCHER = '0x000000000000000000000000000000000000a11c';
const BUYER = '0x000000000000000000000000000000000000b0b0';

function req(name) {
  const v = process.env[name];
  if (!v) throw new Error(`${name} is required`);
  return v;
}

function clean(x) {
  return JSON.parse(JSON.stringify(x, (_, v) => typeof v === 'bigint' ? v.toString() : v));
}

async function rpc(url, method, params = []) {
  const delaysMs = [0, 150, 500, 1200];
  let lastError;
  for (let attempt = 0; attempt < delaysMs.length; attempt++) {
    if (delaysMs[attempt]) {
      await new Promise(resolve => setTimeout(resolve, delaysMs[attempt]));
    }
    try {
      const res = await fetch(url, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }),
        signal: AbortSignal.timeout(10000),
      });
      if (!res.ok) throw new Error(`RPC HTTP ${res.status} for ${method}`);
      const body = await res.json();
      if (body.error) throw new Error(`RPC ${method}: ${JSON.stringify(body.error)}`);
      return body.result;
    } catch (e) {
      lastError = e;
    }
  }
  throw new Error(`RPC transport failed after 4 attempts for ${method}: ${lastError?.message || lastError}`);
}

function blockHeaderFromRpc(b) {
  return {
    number: BigInt(b.number) + 1n,
    coinbase: createAddressFromString(b.miner),
    timestamp: BigInt(b.timestamp) + 1n,
    difficulty: BigInt(b.difficulty || '0x0'),
    prevRandao: hexToBytes(b.mixHash || ('0x' + '00'.repeat(32))),
    gasLimit: BigInt(b.gasLimit),
    baseFeePerGas: BigInt(b.baseFeePerGas || '0x0'),
    getBlobGasPrice: () => 0n,
  };
}

// BSC's public dataseed endpoint does not provide the eth_getProof response
// shape required by EthereumJS RPCStateManager 10.1.3. The state manager only
// needs the account envelope here; code and storage are already fetched through
// eth_getCode and eth_getStorageAt respectively. Override this single provider
// hook with ordinary read-only RPC methods at the exact same frozen block tag.
function installBasicAccountLoader(state, rpcUrl, blockTag) {
  state.getAccountFromProvider = async function(address) {
    const a = address.toString();
    // Public BSC dataseed can drop short concurrent bursts; keep these
    // read-only account-envelope requests serial and retry transport failures.
    const balanceHex = await rpc(rpcUrl, 'eth_getBalance', [a, blockTag]);
    const nonceHex = await rpc(rpcUrl, 'eth_getTransactionCount', [a, blockTag]);
    const codeHex = await rpc(rpcUrl, 'eth_getCode', [a, blockTag]);

    if (balanceHex == null || nonceHex == null || codeHex == null) {
      throw new Error(`incomplete basic account RPC data for ${a}`);
    }

    return createAccount({
      balance: BigInt(balanceHex),
      nonce: BigInt(nonceHex),
      codeHash: hexToBytes(ethers.keccak256(codeHex)),
      // RPCStateManager reads actual storage slots independently via
      // eth_getStorageAt, so no account proof/storage root is required here.
      storageRoot: KECCAK256_RLP,
    });
  };
}

function predictedToken(portal, implAddr, salt) {
  const prefix = '0x3d602d80600a3d3981f3363d3d373d3d3d363d73';
  const suffix = '0x5af43d82803e903d91602b57fd5bf3';
  const init = ethers.concat([prefix, implAddr, suffix]);
  return ethers.getCreate2Address(portal, salt, ethers.keccak256(init));
}

// TAXED_V3 launch addresses are required to satisfy the frozen tax-token vanity suffix.
// Generate a deterministic unused candidate rather than reuse a historical/live salt.
async function chooseSalt(rpcUrl, blockTag) {
  const portal = ethers.getAddress(PORTAL);
  const impl = ethers.getAddress(TAX_V3_IMPL);
  for (let i = 0; i < 2_000_000; i++) {
    const salt = ethers.keccak256(ethers.toUtf8Bytes(`darkbot-g06a-economics-v1:${i}`));
    const token = predictedToken(portal, impl, salt);
    if (!token.toLowerCase().endsWith('7777')) continue;
    const code = await rpc(rpcUrl, 'eth_getCode', [token, blockTag]);
    if (code === '0x') return { salt, token, counter: i };
  }
  throw new Error('could not find unused deterministic TAXED_V3 vanity salt');
}

function launchCalldata(launcher, salt) {
  const iface = new ethers.Interface([
    'function newTokenV6((string name,string symbol,string uri,uint8 dexThresh,bytes32 salt,uint8 migratorType,address quoteToken,uint256 quoteAmt,address beneficiary,bytes permit,bytes32 extensionId,bytes extensionData,uint8 dexId,uint8 lpFeeProfile,uint16 buyTax,uint16 sellTax,uint64 taxDuration,uint64 antiFarmerDuration,uint16 mktBps,uint16 deflationBps,uint16 dividendBps,uint16 lpBps,uint256 minimumShareBalance,address dividendToken,address commissionReceiver,uint8 tokenVersion) params) payable returns(address)'
  ]);
  return iface.encodeFunctionData('newTokenV6', [{
    name: 'DarkBot G06A Economics',
    symbol: 'DBG06E',
    uri: 'ipfs://darkbot-g06a-economics',
    dexThresh: 1,
    salt,
    migratorType: 1,
    quoteToken: ethers.ZeroAddress,
    quoteAmt: 0n,
    beneficiary: launcher,
    permit: '0x',
    extensionId: ethers.ZeroHash,
    extensionData: '0x',
    dexId: 0,
    lpFeeProfile: 0,
    buyTax: 100,
    sellTax: 100,
    taxDuration: 31_536_000,
    antiFarmerDuration: 86_400,
    mktBps: 10_000,
    deflationBps: 0,
    dividendBps: 0,
    lpBps: 0,
    minimumShareBalance: 0n,
    dividendToken: ethers.ZeroAddress,
    commissionReceiver: ethers.ZeroAddress,
    tokenVersion: 6,
  }]);
}

function buyCalldata(token, value = BUY_VALUE) {
  const iface = new ethers.Interface([
    'function swapExactInput((address inputToken,address outputToken,uint256 inputAmount,uint256 minOutputAmount,bytes permit) params) payable returns(uint256)'
  ]);
  return iface.encodeFunctionData('swapExactInput', [{
    inputToken: ethers.ZeroAddress,
    outputToken: token,
    inputAmount: value,
    // Deliberately independent of the Rust quote. We only require positive output.
    minOutputAmount: 1n,
    permit: '0x',
  }]);
}

function balanceOfCalldata(owner) {
  return new ethers.Interface([
    'function balanceOf(address owner) view returns(uint256)'
  ]).encodeFunctionData('balanceOf', [owner]);
}

function revertHex(res) {
  const x = bytesToHex(res.execResult.returnValue || new Uint8Array());
  return x === '0x' ? null : x;
}

async function main() {
  const rpcUrl = req('BSC_MAINNET_RPC_HTTPS');
  const preflightPath = process.argv[2] ? path.resolve(process.argv[2]) : DEFAULT_PREFLIGHT_PATH;
  const preflight = JSON.parse(fs.readFileSync(preflightPath));

  if (preflight.schema !== 'darkbot.g06a.preflight-readonly.v2') throw new Error('wrong preflight schema');
  if (preflight.chain_id !== '56') throw new Error('preflight is not BSC Mainnet');
  if (preflight.portal.toLowerCase() !== PORTAL) throw new Error('Portal mismatch');
  if (preflight.tax_v3_impl.toLowerCase() !== TAX_V3_IMPL) throw new Error('TAX_V3 implementation mismatch');
  if (preflight.native_quote_configuration.enabled !== '1') throw new Error('native BNB disabled');
  if (preflight.native_quote_configuration.default_curve_id !== '20') throw new Error('default native curve is not 20');
  if (preflight.native_quote_configuration.alternative_curve_id !== '20') throw new Error('alternative native curve is not 20');
  if (preflight.native_quote_configuration.native_to_quote_swap_type !== '0') throw new Error('unexpected native swap type');
  if (preflight.native_quote_configuration.dex_id !== '0') throw new Error('unexpected dex id');
  if (preflight.protocol_fee_bps.buy !== '100') throw new Error('unexpected buy fee');
  if (preflight.tax_on_bonding_curve_enabled !== true) throw new Error('bonding-curve tax disabled');

  const chain = BigInt(await rpc(rpcUrl, 'eth_chainId'));
  if (chain !== CHAIN_ID) throw new Error(`wrong chain: ${chain}`);

  // Freeze execution state to the exact block recorded by the committed preflight.
  const parentNum = BigInt(preflight.latest_block_number);
  const parentTag = '0x' + parentNum.toString(16);
  const parent = await rpc(rpcUrl, 'eth_getBlockByNumber', [parentTag, false]);
  if (!parent) throw new Error('preflight parent block unavailable from RPC');
  if (parent.hash.toLowerCase() !== preflight.latest_block_hash.toLowerCase()) {
    throw new Error('preflight block hash mismatch');
  }

  let portalCode;
  let taxCode;
  try {
    portalCode = await rpc(rpcUrl, 'eth_getCode', [PORTAL, parentTag]);
    taxCode = await rpc(rpcUrl, 'eth_getCode', [TAX_V3_IMPL, parentTag]);
  } catch (e) {
    if (String(e).includes('missing trie node')) {
      throw new Error(
        `RPC has pruned state for preflight block ${preflight.latest_block_number}. ` +
        'Generate a fresh read-only preflight to a temporary file and run this crosscheck immediately against that file. ' +
        `Original error: ${e.message}`
      );
    }
    throw e;
  }
  if (ethers.getBytes(portalCode).length !== preflight.portal_code_bytes) throw new Error('Portal code length mismatch');
  if (ethers.getBytes(taxCode).length !== preflight.tax_v3_impl_code_bytes) throw new Error('TAX_V3 code length mismatch');

  const saltChoice = await chooseSalt(rpcUrl, parentTag);
  const launchData = launchCalldata(LAUNCHER, saltChoice.salt);
  const buyData = buyCalldata(saltChoice.token);

  // Differential authority check: ask the real BSC node to execute exactly the same
  // launch call at the exact frozen parent state. This is eth_call only: no transaction
  // is signed, submitted, or persisted. If this succeeds but EthereumJS below reverts,
  // the divergence belongs to the local EVM backend rather than the launch parameters.
  let rpcLaunchGasEstimate;
  try {
    rpcLaunchGasEstimate = BigInt(await rpc(rpcUrl, 'eth_estimateGas', [{
      from: LAUNCHER,
      to: PORTAL,
      value: '0x0',
      data: launchData
    }, parentTag]));
  } catch (e) {
    throw new Error(
      'MAINNET_ESTIMATE_GAS_LAUNCH_REVERT: exact launch parameters are rejected before gas estimation at ' +
      `block ${preflight.latest_block_number}; ${e.message}`
    );
  }

  const rpcLaunchGasLimit = rpcLaunchGasEstimate * 12n / 10n;
  let rpcLaunchReturn;
  try {
    rpcLaunchReturn = await rpc(rpcUrl, 'eth_call', [{
      from: LAUNCHER,
      to: PORTAL,
      value: '0x0',
      data: launchData,
      gas: '0x' + rpcLaunchGasLimit.toString(16)
    }, parentTag]);
  } catch (e) {
    throw new Error(
      'MAINNET_ETH_CALL_LAUNCH_REVERT_AFTER_ESTIMATE: estimateGas succeeded but eth_call reverted at ' +
      `block ${preflight.latest_block_number}; estimate=${rpcLaunchGasEstimate}; limit=${rpcLaunchGasLimit}; ${e.message}`
    );
  }
  const rpcLaunchDecoded = rpcLaunchReturn === '0x'
    ? null
    : ethers.AbiCoder.defaultAbiCoder().decode(['address'], rpcLaunchReturn)[0];
  if (rpcLaunchDecoded && rpcLaunchDecoded.toLowerCase() !== saltChoice.token.toLowerCase()) {
    throw new Error(
      `MAINNET_ETH_CALL_TOKEN_MISMATCH: returned ${rpcLaunchDecoded}, predicted ${saltChoice.token}`
    );
  }

  const common = createCustomCommon(
    { chainId: Number(CHAIN_ID) },
    Mainnet,
    { hardfork: Hardfork.Osaka }
  );
  const state = new RPCStateManager({ provider: rpcUrl, blockTag: parentNum });
  installBasicAccountLoader(state, rpcUrl, parentTag);
  const blockchain = new RPCBlockChain(rpcUrl);

  const launcherAddr = createAddressFromString(LAUNCHER);
  const buyerAddr = createAddressFromString(BUYER);
  const portalAddr = createAddressFromString(PORTAL);

  // Local overlay only; this is never written to Mainnet.
  await state.putAccount(launcherAddr, new Account(0n, 100_000_000_000_000_000_000n));
  await state.putAccount(buyerAddr, new Account(0n, 100_000_000_000_000_000_000n));

  const evm = await createEVM({ common, stateManager: state, blockchain });
  const block = { header: blockHeaderFromRpc(parent) };

  const launchRes = await evm.runCall({
    caller: launcherAddr,
    origin: launcherAddr,
    to: portalAddr,
    value: LAUNCH_VALUE,
    data: hexToBytes(launchData),
    gasLimit: rpcLaunchGasLimit,
    gasPrice: GAS_PRICE,
    block,
  });

  const launchError = launchRes.execResult.exceptionError?.error || null;
  if (launchError) {
    throw new Error(
      `local launch simulation reverted: ${launchError}; returndata=${revertHex(launchRes)}; ` +
      `mainnet_estimate=${rpcLaunchGasEstimate}; local_gas_limit=${rpcLaunchGasLimit}`
    );
  }

  const launchReturn = bytesToHex(launchRes.execResult.returnValue);
  if (launchReturn !== '0x') {
    const decoded = ethers.AbiCoder.defaultAbiCoder().decode(['address'], launchReturn)[0];
    if (decoded.toLowerCase() !== saltChoice.token.toLowerCase()) {
      throw new Error(`launch returned ${decoded}, predicted ${saltChoice.token}`);
    }
  }

  const trialResults = [];
  for (const [trialIndex, value] of BUY_VALUES.entries()) {
    if (BATCH_RAW) await state.checkpoint();
    try {
      const buyRes = await evm.runCall({
        caller: buyerAddr,
        origin: buyerAddr,
        to: portalAddr,
        value,
        data: hexToBytes(buyCalldata(saltChoice.token, value)),
        gasLimit: BUY_GAS_LIMIT,
        gasPrice: GAS_PRICE,
        block,
      });
      const buyError = buyRes.execResult.exceptionError?.error || null;
      if (buyError) {
        throw new Error(`local BUY simulation reverted: ${buyError}; trial=${trialIndex}; value=${value}; returndata=${revertHex(buyRes)}`);
      }
      const retHex = bytesToHex(buyRes.execResult.returnValue);
      const returned = BigInt(retHex === '0x' ? '0x0' : retHex);
      const balanceRes = await evm.runCall({
        caller: buyerAddr,
        origin: buyerAddr,
        to: createAddressFromString(saltChoice.token),
        value: 0n,
        data: hexToBytes(balanceOfCalldata(BUYER)),
        gasLimit: 500_000n,
        gasPrice: 0n,
        block,
        isStatic: true,
      });
      const balanceError = balanceRes.execResult.exceptionError?.error || null;
      if (balanceError) throw new Error(`local balanceOf failed: ${balanceError}; trial=${trialIndex}`);
      const balHex = bytesToHex(balanceRes.execResult.returnValue);
      const balance = BigInt(balHex === '0x' ? '0x0' : balHex);
      if (returned <= 0n) throw new Error(`independent EVM BUY returned zero; trial=${trialIndex}; value=${value}`);
      if (balance !== returned) {
        throw new Error(`BUY return ${returned} != buyer token balance ${balance}; trial=${trialIndex}`);
      }
      trialResults.push({ buy_value_wei: value, received_raw: returned, buyer_balance_raw: balance, execution_gas: buyRes.execResult.executionGasUsed });
    } finally {
      if (BATCH_RAW) await state.revert();
    }
  }
  const returned = trialResults[0].received_raw;
  const balance = trialResults[0].buyer_balance_raw;
  const buyExecutionGas = trialResults[0].execution_gas;

  console.log(JSON.stringify(clean({
    schema: 'darkbot.g06a.economics-evm-crosscheck.v1',
    preflight_path: preflightPath,
    status: 'EVM_SEQUENCE_PASS',
    state_source: {
      block_number: parentNum,
      block_hash: parent.hash,
      rpc_state_reads_only: true,
    },
    mainnet_estimate_gas_launch_success: true,
    mainnet_launch_gas_estimate: rpcLaunchGasEstimate,
    mainnet_launch_gas_limit_120pct: rpcLaunchGasLimit,
    mainnet_eth_call_launch_success: true,
    mainnet_eth_call_launch_return: rpcLaunchReturn,
    mainnet_eth_call_predicted_token_match: rpcLaunchDecoded === null
      ? null
      : rpcLaunchDecoded.toLowerCase() === saltChoice.token.toLowerCase(),
    portal: PORTAL,
    tax_v3_impl: TAX_V3_IMPL,
    launcher: LAUNCHER,
    buyer: BUYER,
    local_only_funded_callers: true,
    salt: saltChoice.salt,
    salt_counter: saltChoice.counter,
    token: saltChoice.token,
    launch_value_wei: LAUNCH_VALUE,
    buy_value_wei: BUY_VALUE,
    buy_min_output_used_for_execution: '1',
    launch_success: true,
    buy_success: true,
    simulated_received_amount: returned,
    simulated_buyer_balance_after: balance,
    launch_execution_gas: launchRes.execResult.executionGasUsed,
    buy_execution_gas: buyExecutionGas,
    batch_buy_trials: BATCH_RAW ? trialResults : undefined,
    post_launch_checkpoint_revert: Boolean(BATCH_RAW),
    rust_expected_output_was_execution_input: false,
    signed_transactions_created: false,
    signed_transactions_submitted: false,
    bundle_submitted: false,
    network_actions: 'READ_ONLY_RPC_STATE_AND_HEADER_ONLY',
    limitations: [
      'EthereumJS local message execution against frozen BSC Mainnet state; not BSC consensus/block validation',
      'Dummy caller balances exist only in the local StateManager overlay',
      'Account envelopes are reconstructed from eth_getBalance + eth_getTransactionCount + eth_getCode because the public BSC dataseed is incompatible with EthereumJS eth_getProof account loading; contract storage still comes block-exactly from eth_getStorageAt',
      'This probe proves economics/contract semantics only; wallet nonces, builder floors and signed bundle validity belong to later DRY checks'
    ]
  }), null, 2));
}

main().catch((e) => {
  console.error(e.stack || String(e));
  process.exitCode = 1;
});
