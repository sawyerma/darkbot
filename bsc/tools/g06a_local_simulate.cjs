#!/usr/bin/env node
// G0.6a independent local sequential simulation.
// Network use is READ-ONLY state/header retrieval through RPCStateManager.
// Valid signed Mainnet transaction bytes are created locally and NEVER submitted.
// Output evidence contains hashes/amounts/gas only, never raw signed tx bytes.
// The evidence is a binary DBREC001 record (./dbrec.cjs); stdout is a text
// summary. No JSON is written.

'use strict';

const path = require('path');

// Reproducible npm toolsets next to this script (package.json + package-lock.json;
// install with `npm ci` in each directory). Formerly loaded from the legacy archive.
const LEGACY_TOOLS = path.join(__dirname, 'evm-audit-tools/node_modules');
const ABI_TOOLS = path.join(__dirname, 'abi-tools/node_modules');

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
const { createLegacyTx } = require(path.join(LEGACY_TOOLS, '@ethereumjs/tx'));
const ethers = require(path.join(ABI_TOOLS, 'ethers'));
const dbrec = require('./dbrec.cjs');
const { guardEgress, localCreations } = require('./g06a_private_mode.cjs');
const { pin, pinState, requireBlock } = require('./g06a_state_pin.cjs');

const CHAIN_ID = 56n;
const PORTAL = '0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0';
const BUY_VALUE_DEFAULT = 1000000000000000n;
const SLIPPAGE_BPS = 100n;

function req(name) {
  const v = process.env[name];
  if (!v) throw new Error(`${name} is required`);
  return v;
}
function dec(name) { return BigInt(req(name)); }
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

function installBasicAccountLoader(state, rpcUrl, blockTag) {
  state.getAccountFromProvider = async function(address) {
    const a = address.toString();
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
      storageRoot: KECCAK256_RLP,
    });
  };
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
function predictedToken(portal, implAddr, salt) {
  const prefix = '0x3d602d80600a3d3981f3363d3d373d3d3d363d73';
  const suffix = '0x5af43d82803e903d91602b57fd5bf3';
  const init = ethers.concat([prefix, implAddr, suffix]);
  return ethers.getCreate2Address(portal, salt, ethers.keccak256(init));
}
function launchCalldata(launcher, salt) {
  const iface = new ethers.Interface([
    'function newTokenV6((string name,string symbol,string uri,uint8 dexThresh,bytes32 salt,uint8 migratorType,address quoteToken,uint256 quoteAmt,address beneficiary,bytes permit,bytes32 extensionId,bytes extensionData,uint8 dexId,uint8 lpFeeProfile,uint16 buyTax,uint16 sellTax,uint64 taxDuration,uint64 antiFarmerDuration,uint16 mktBps,uint16 deflationBps,uint16 dividendBps,uint16 lpBps,uint256 minimumShareBalance,address dividendToken,address commissionReceiver,uint8 tokenVersion) params) payable returns(address)'
  ]);
  return iface.encodeFunctionData('newTokenV6', [{
    name: 'DarkBot G06A Self Launch',
    symbol: 'DBG06A',
    uri: 'ipfs://darkbot-g06a-dry',
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
    taxDuration: 31536000,
    antiFarmerDuration: 86400,
    mktBps: 10000,
    deflationBps: 0,
    dividendBps: 0,
    lpBps: 0,
    minimumShareBalance: 0n,
    dividendToken: ethers.ZeroAddress,
    commissionReceiver: ethers.ZeroAddress,
    tokenVersion: 6,
  }]);
}
function buyCalldata(token, input, minOutput) {
  const iface = new ethers.Interface([
    'function swapExactInput((address inputToken,address outputToken,uint256 inputAmount,uint256 minOutputAmount,bytes permit) params) payable returns(uint256)'
  ]);
  return iface.encodeFunctionData('swapExactInput', [{
    inputToken: ethers.ZeroAddress,
    outputToken: token,
    inputAmount: input,
    minOutputAmount: minOutput,
    permit: '0x',
  }]);
}
function balanceOfCalldata(owner) {
  return new ethers.Interface([
    'function balanceOf(address owner) view returns(uint256)'
  ]).encodeFunctionData('balanceOf', [owner]);
}
async function main() {
  const outPath = process.argv[2];
  if (!outPath) throw new Error('usage: node g06a_local_simulate.cjs <local_simulation.dbrec>');

  const rpcUrl = req('BSC_MAINNET_RPC_HTTPS');
  const launcherWallet = new ethers.Wallet(req('DARKBOT_G06A_LAUNCHER_PRIVATE_KEY'));
  const buyerWallet = new ethers.Wallet(req('DARKBOT_G06A_BUYER_PRIVATE_KEY'));
  if (launcherWallet.address.toLowerCase() === buyerWallet.address.toLowerCase()) {
    throw new Error('launcher and buyer must be distinct');
  }

  const portal = ethers.getAddress(PORTAL);
  const implAddr = ethers.getAddress(req('DARKBOT_G06A_TAX_V3_IMPL'));
  const salt = req('DARKBOT_G06A_SALT');
  const token = predictedToken(portal, implAddr, salt);

  const launcherNonce = dec('DARKBOT_G06A_LAUNCHER_NONCE');
  const buyerNonce = dec('DARKBOT_G06A_BUYER_NONCE');
  const gasPrice = dec('DARKBOT_G06A_GAS_PRICE_WEI');
  const launchGasLimit = dec('DARKBOT_G06A_LAUNCH_GAS_LIMIT');
  const buyGasLimit = dec('DARKBOT_G06A_BUY_GAS_LIMIT');
  const launchValue = dec('DARKBOT_G06A_LAUNCH_VALUE_WEI');
  const buyValue = process.env.DARKBOT_G06A_BUY_VALUE_WEI
    ? BigInt(process.env.DARKBOT_G06A_BUY_VALUE_WEI)
    : BUY_VALUE_DEFAULT;
  const localExpected = dec('DARKBOT_G06A_LOCAL_EXPECTED_OUTPUT');
  if (localExpected <= 0n) throw new Error('local expected output must be >0');
  const minOutput = localExpected * (10000n - SLIPPAGE_BPS) / 10000n;
  if (minOutput <= 0n) throw new Error('minOutput must be >0');

  const launchData = launchCalldata(launcherWallet.address, salt);
  const buyData = buyCalldata(token, buyValue, minOutput);
  // The salt, the launch calldata and the token never leave the process.
  guardEgress([salt, launchData, token]);

  const launchReq = {
    chainId: CHAIN_ID, nonce: launcherNonce, gasPrice, gasLimit: launchGasLimit,
    to: portal, value: launchValue, data: launchData, type: 0,
  };
  const buyReq = {
    chainId: CHAIN_ID, nonce: buyerNonce, gasPrice, gasLimit: buyGasLimit,
    to: portal, value: buyValue, data: buyData, type: 0,
  };

  // Offline signing only. Raw bytes are intentionally never printed or persisted.
  const rawLaunch = await launcherWallet.signTransaction(launchReq);
  const rawBuy = await buyerWallet.signTransaction(buyReq);
  const launchHash = ethers.keccak256(rawLaunch);
  const buyHash = ethers.keccak256(rawBuy);

  const chain = BigInt(await rpc(rpcUrl, 'eth_chainId'));
  if (chain !== CHAIN_ID) throw new Error(`wrong chain: ${chain}`);
  const parentNumber = dec('DARKBOT_G06A_SEND_HEAD');
  // BSC-002: exactly the preflight's block, by hash (DARKBOT_G06A_SEND_HEAD_HASH).
  const parent = await requireBlock(rpc, rpcUrl, parentNumber, req('DARKBOT_G06A_SEND_HEAD_HASH'), 'before the simulation');

  const common = createCustomCommon(
    { chainId: Number(CHAIN_ID) },
    Mainnet,
    { hardfork: Hardfork.Osaka }
  );
  const state = new RPCStateManager({ provider: rpcUrl, blockTag: parentNumber });
  pinState(state, parent.hash);
  installBasicAccountLoader(state, rpcUrl, pin(parent.hash));
  const blockchain = new RPCBlockChain(rpcUrl);
  const evm = await createEVM({ common, stateManager: state, blockchain });
  localCreations(evm, state, [token]);
  const block = { header: blockHeaderFromRpc(parent) };

  const launcherAddr = createAddressFromString(launcherWallet.address);
  const buyerAddr = createAddressFromString(buyerWallet.address);
  const portalAddr = createAddressFromString(portal);

  // DRY-only local EOA overlay. Keep the real wallet identities and frozen
  // preflight nonces, but do not depend on historical balance/account reads
  // from a pruned public BSC RPC. Contract code/storage still comes from the
  // exact frozen Mainnet parent state through RPCStateManager.
  const dryBalance = 100_000_000_000_000_000_000n;
  await state.putAccount(launcherAddr, new Account(launcherNonce, dryBalance));
  await state.putAccount(buyerAddr, new Account(buyerNonce, dryBalance));

  const launchRes = await evm.runCall({
    caller: launcherAddr,
    origin: launcherAddr,
    to: portalAddr,
    value: launchValue,
    data: hexToBytes(launchData),
    gasLimit: launchGasLimit,
    gasPrice,
    block,
    // DRY-only local execution; G0.7 independently requires funding_pass=true.
    skipBalance: true,
  });
  const launchError = launchRes.execResult.exceptionError?.error || null;
  if (launchError) throw new Error(`local launch simulation reverted: ${launchError}`);
  // v9.1 R2/T2: newTokenV6 must return exactly the CREATE2-predicted token.
  const launchRet = bytesToHex(launchRes.execResult.returnValue);
  if (launchRet.length !== 66 || ('0x' + launchRet.slice(-40)).toLowerCase() !== token.toLowerCase()) {
    throw new Error(`newTokenV6 returned ${launchRet}, not the predicted CREATE2 token ${token}`);
  }

  const buyRes = await evm.runCall({
    caller: buyerAddr,
    origin: buyerAddr,
    to: portalAddr,
    value: buyValue,
    data: hexToBytes(buyData),
    gasLimit: buyGasLimit,
    gasPrice,
    block,
    // DRY-only ephemeral local EVM state.
    skipBalance: true,
  });
  const buyError = buyRes.execResult.exceptionError?.error || null;
  if (buyError) throw new Error(`local BUY simulation reverted: ${buyError}`);

  const retHex = bytesToHex(buyRes.execResult.returnValue);
  const returned = BigInt(retHex === '0x' ? '0x0' : retHex);

  const balanceRes = await evm.runCall({
    caller: buyerAddr,
    origin: buyerAddr,
    to: createAddressFromString(token),
    value: 0n,
    data: hexToBytes(balanceOfCalldata(buyerWallet.address)),
    gasLimit: 500000n,
    gasPrice: 0n,
    block,
    isStatic: true,
  });
  const balanceError = balanceRes.execResult.exceptionError?.error || null;
  if (balanceError) throw new Error(`local balanceOf simulation failed: ${balanceError}`);
  const balHex = bytesToHex(balanceRes.execResult.returnValue);
  const balance = BigInt(balHex === '0x' ? '0x0' : balHex);
  if (returned !== balance) {
    throw new Error(`BUY return ${returned} != buyer token balance ${balance}`);
  }

  const launchTxModel = createLegacyTx({
    nonce: launcherNonce, gasPrice, gasLimit: launchGasLimit, to: portal,
    value: launchValue, data: launchData,
  }, { common });
  const buyTxModel = createLegacyTx({
    nonce: buyerNonce, gasPrice, gasLimit: buyGasLimit, to: portal,
    value: buyValue, data: buyData,
  }, { common });

  const launchIntrinsic = launchTxModel.getIntrinsicGas();
  const buyIntrinsic = buyTxModel.getIntrinsicGas();
  const launchGasUpper = launchIntrinsic + launchRes.execResult.executionGasUsed;
  const buyGasUpper = buyIntrinsic + buyRes.execResult.executionGasUsed;
  if (launchGasUpper > launchGasLimit) {
    throw new Error(`launch gas upper bound ${launchGasUpper} > signed limit ${launchGasLimit}`);
  }
  if (buyGasUpper > buyGasLimit) {
    throw new Error(`buy gas upper bound ${buyGasUpper} > signed limit ${buyGasLimit}`);
  }

  const launcherPending = BigInt(await rpc(
    rpcUrl, 'eth_getTransactionCount', [launcherWallet.address, 'pending']
  ));
  const buyerPending = BigInt(await rpc(
    rpcUrl, 'eth_getTransactionCount', [buyerWallet.address, 'pending']
  ));

  const floorWei = dec('DARKBOT_G06A_STRICTEST_BUILDER_FLOOR_WEI');
  const floorEvidenceSha = req('DARKBOT_G06A_BUILDER_FLOOR_EVIDENCE_SHA256');
  if (!/^[0-9a-f]{64}$/i.test(floorEvidenceSha)) {
    throw new Error('DARKBOT_G06A_BUILDER_FLOOR_EVIDENCE_SHA256 must be 64 hex chars');
  }

  const record = dbrec.encode('darkbot.g06a.local-simulation.v2', {
    '_provenance.tool': 'bsc/tools/g06a_local_simulate.cjs',
    '_provenance.observed_unix_ms': BigInt(Date.now()),
    schema: 'darkbot.g06a.local-simulation.v1',
    backend: '@ethereumjs/evm + RPCStateManager 10.1.3',
    bsc_execution_profile: 'chainId 56; EthereumJS Osaka EVM semantics; BSC Pasteur consensus/builder rules outside local EVM scope',
    'state_source.block_number': parentNumber,
    'state_source.block_hash': parent.hash,
    'state_source.rpc_state_reads_only': true,
    'state_source.valid_signed_tx_bytes_submitted': false,
    launch_tx_hash: launchHash,
    buy_tx_hash: buyHash,
    token,
    launcher: launcherWallet.address,
    buyer: buyerWallet.address,
    launcher_pending_nonce: launcherPending,
    buyer_pending_nonce: buyerPending,
    launch_success: true,
    buy_success: true,
    simulated_received_amount: returned,
    simulated_buyer_balance_after: balance,
    local_expected_output_input: localExpected,
    min_output: minOutput,
    output_difference: (returned - localExpected).toString(),
    launch_execution_gas: launchRes.execResult.executionGasUsed,
    buy_execution_gas: buyRes.execResult.executionGasUsed,
    launch_intrinsic_gas: BigInt(launchIntrinsic),
    buy_intrinsic_gas: BigInt(buyIntrinsic),
    launch_gas_used: launchGasUpper,
    buy_gas_used: buyGasUpper,
    gas_accounting: 'conservative upper bound = intrinsic + runCall executionGasUsed; refunds not subtracted',
    strictest_verified_builder_floor_wei: floorWei,
    builder_floor_evidence_sha256: '0x' + floorEvidenceSha.toLowerCase(),
    raw_signed_transactions_persisted: false,
    raw_bundle_persisted: false,
    limitations: [
      'Local message/EVM semantics and conservative transaction gas upper bound, not BSC consensus validation',
      'RPCStateManager performs read-only state/code/storage requests against the frozen parent block',
      'Builder gas-floor evidence is a separately frozen input and is not established by this script',
      'No signed transaction or bundle is submitted to any RPC or builder'
    ],
  });
  await requireBlock(rpc, rpcUrl, parentNumber, parent.hash, 'before the evidence write');
  dbrec.writeNew(outPath, record);
  const sha = require('crypto').createHash('sha256').update(record).digest('hex');
  console.log(`LOCAL_SIMULATION=WRITTEN\nRECORD=${outPath}\nRECORD_SHA256=${sha}\nBLOCK=${parentNumber}\nLAUNCH_TX_HASH=${launchHash}\nBUY_TX_HASH=${buyHash}\nTOKEN=${token}\nSIMULATED_RECEIVED_AMOUNT=${returned}\nOUTPUT_DIFFERENCE=${returned - localExpected}\nLAUNCH_GAS_UPPER=${launchGasUpper}\nBUY_GAS_UPPER=${buyGasUpper}\nVALID_SIGNED_TX_BYTES_SUBMITTED=false`);
}

main().catch((e) => {
  console.error(e.stack || String(e));
  process.exitCode = 1;
});
