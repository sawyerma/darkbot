#!/usr/bin/env node
// G0.1 Flap economics reconfirmation (Phase D gate review HIGH-1): the
// independent local sequential EVM half.
//
// The witnessed G0.6a native launch (newTokenV6, native BNB, quoteAmt 0,
// TOKEN_TAXED_V3) and then a direct native swapExactInput BUY, executed by
// EthereumJS on BSC Mainnet state pinned to one block (EIP-1898 block hash).
// Same method, toolsets, callers and launch parameters as
// g06a_economics_evm_crosscheck.cjs; parameterized by block, salt and BUY
// value. The Rust caller (g01_flap_econ) compares this result with the node
// execution and the production quote and writes the binary record.
//
// Independence: encodes its own calldata with ethers (its Keccak-256 is
// printed); never reads a Rust quote; the BUY minimum output is 1.
//
// State: accounts from eth_getProof (balance, nonce; its codeHash must equal
// the Keccak-256 of eth_getCode), code from eth_getCode, storage from
// eth_getStorageAt -- every read at {blockHash, requireCanonical: true}.
//
// Safety: no key, no signing, no transaction construction, no
// eth_sendRawTransaction; read-only RPC state, code and header reads only.
// Caller balances exist only in the local state overlay.
//
// Usage: node g01_flap_econ_evm.cjs --block <number> --hash <0x..> --salt <0x..>
//          --launcher <0x..> --buyer <0x..> --buy-value <wei> --launch-gas <n>
//          --buy-gas <n> --balance-gas <n> --gas-price <wei> --caller-balance <wei>
//          --rpc-timeout-ms <ms> --rpc-attempts <n> --rpc-retry-pause-ms <ms>
// Environment: BSC_MAINNET_RPC_HTTPS (a read-only endpoint serving eth_getProof
// at a recent block; the free public dataseed refuses it).
// Output: KEY=value lines (no JSON); a non-zero exit on any failure.

'use strict';

const path = require('path');

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
const ethers = require(path.join(ABI_TOOLS, 'ethers'));
const { pin, pinState, requireBlock } = require('./g06a_state_pin.cjs');

const CHAIN_ID = 56n;
const PORTAL = '0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0';
const TOKEN_BOUGHT = ethers.id('TokenBought(uint256,address,address,uint256,uint256,uint256,uint256)');
const TAX_PAID = [
  ethers.id('TaxOnBondingCurvePaid(address,uint256)'),
  ethers.id('TaxV2OnBondingCurvePaid(address,uint256)'),
];

function args() {
  const out = {};
  const argv = process.argv.slice(2);
  for (let i = 0; i < argv.length; i += 2) {
    if (!argv[i].startsWith('--') || argv[i + 1] === undefined) throw new Error(`bad argument ${argv[i]}`);
    out[argv[i].slice(2)] = argv[i + 1];
  }
  for (const k of ['block', 'hash', 'salt', 'launcher', 'buyer', 'buy-value', 'launch-gas', 'buy-gas',
    'balance-gas', 'gas-price', 'caller-balance', 'rpc-timeout-ms', 'rpc-attempts', 'rpc-retry-pause-ms']) {
    if (out[k] === undefined) throw new Error(`--${k} is required`);
  }
  return out;
}

// The transport bounds (--rpc-timeout-ms, --rpc-attempts, --rpc-retry-pause-ms).
const transport = { timeoutMs: 0, attempts: 1, pauseMs: 0 };

async function rpc(url, method, params = []) {
  let lastError;
  for (let attempt = 0; attempt < transport.attempts; attempt++) {
    if (attempt) await new Promise(resolve => setTimeout(resolve, transport.pauseMs * attempt));
    try {
      const res = await fetch(url, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }),
        signal: AbortSignal.timeout(transport.timeoutMs),
      });
      if (!res.ok) throw new Error(`RPC HTTP ${res.status} for ${method}`);
      const body = await res.json();
      if (body.error) throw new Error(`RPC ${method}: ${body.error.message || body.error.code}`);
      return body.result;
    } catch (e) {
      lastError = e;
    }
  }
  throw new Error(`RPC transport failed after ${transport.attempts} attempts for ${method}: ${lastError?.message || lastError}`);
}

// As g06a_economics_evm_crosscheck.cjs: the next block on top of the pinned one.
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

const EMPTY_CODE_HASH = ethers.keccak256('0x');

// Account envelopes from eth_getProof at the pinned block. RPCStateManager's
// own loader takes the proof's codeHash as is, but a node reports the zero
// hash for an absent account; so the code hash is the Keccak-256 of
// eth_getCode, and the proof's must agree with it (zero only for no code).
// Storage stays block-exact via eth_getStorageAt (pinState).
function installProofAccountLoader(state, rpcUrl, blockTag, reads) {
  state.getAccountFromProvider = async function(address) {
    const a = address.toString();
    const proof = await rpc(rpcUrl, 'eth_getProof', [a, [], blockTag]);
    const codeHex = await rpc(rpcUrl, 'eth_getCode', [a, blockTag]);
    if (!proof || proof.balance == null || proof.nonce == null || codeHex == null) {
      throw new Error(`incomplete eth_getProof/eth_getCode data for ${a}`);
    }
    const codeHash = ethers.keccak256(codeHex);
    const proven = String(proof.codeHash || '').toLowerCase();
    const absent = codeHash === EMPTY_CODE_HASH && proven === ethers.ZeroHash;
    if (proven !== codeHash && !absent) {
      throw new Error(`eth_getProof codeHash ${proven} is not keccak256(eth_getCode) ${codeHash} for ${a}`);
    }
    reads.accounts += 1;
    return createAccount({
      balance: BigInt(proof.balance),
      nonce: BigInt(proof.nonce),
      codeHash: hexToBytes(codeHash),
      storageRoot: KECCAK256_RLP,
    });
  };
}

// The witnessed G0.6a launch: the same members as launchCalldata() of
// g06a_economics_evm_crosscheck.cjs.
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

function buyCalldata(token, value) {
  return new ethers.Interface([
    'function swapExactInput((address inputToken,address outputToken,uint256 inputAmount,uint256 minOutputAmount,bytes permit) params) payable returns(uint256)'
  ]).encodeFunctionData('swapExactInput', [{
    inputToken: ethers.ZeroAddress,
    outputToken: token,
    inputAmount: value,
    minOutputAmount: 1n,
    permit: '0x',
  }]);
}

function balanceOfCalldata(owner) {
  return new ethers.Interface(['function balanceOf(address owner) view returns(uint256)'])
    .encodeFunctionData('balanceOf', [owner]);
}

function word(bytes, i) {
  return BigInt('0x' + (Buffer.from(bytes).toString('hex').slice(i * 64, i * 64 + 64) || '0'));
}

async function main() {
  const a = args();
  transport.timeoutMs = Number(a['rpc-timeout-ms']);
  transport.attempts = Number(a['rpc-attempts']);
  transport.pauseMs = Number(a['rpc-retry-pause-ms']);
  if (!(transport.timeoutMs > 0 && transport.attempts >= 1 && transport.pauseMs >= 0)) {
    throw new Error('bad RPC transport bounds');
  }
  const rpcUrl = process.env.BSC_MAINNET_RPC_HTTPS;
  if (!rpcUrl) throw new Error('BSC_MAINNET_RPC_HTTPS is required');
  const chain = BigInt(await rpc(rpcUrl, 'eth_chainId'));
  if (chain !== CHAIN_ID) throw new Error(`wrong chain: ${chain}`);
  const blockNumber = BigInt(a.block);
  const parent = await requireBlock(rpc, rpcUrl, blockNumber, a.hash, 'before the run');

  const launcher = ethers.getAddress(a.launcher);
  const buyer = ethers.getAddress(a.buyer);
  const buyValue = BigInt(a['buy-value']);
  const launchData = launchCalldata(launcher, a.salt);

  const common = createCustomCommon({ chainId: Number(CHAIN_ID) }, Mainnet, { hardfork: Hardfork.Osaka });
  const state = new RPCStateManager({ provider: rpcUrl, blockTag: blockNumber });
  pinState(state, parent.hash);
  const reads = { accounts: 0 };
  installProofAccountLoader(state, rpcUrl, pin(parent.hash), reads);
  const evm = await createEVM({ common, stateManager: state, blockchain: new RPCBlockChain(rpcUrl) });
  const launcherAddr = createAddressFromString(launcher);
  const buyerAddr = createAddressFromString(buyer);
  const portalAddr = createAddressFromString(PORTAL);
  const balance = BigInt(a['caller-balance']);
  await state.putAccount(launcherAddr, new Account(0n, balance));
  await state.putAccount(buyerAddr, new Account(0n, balance));
  const block = { header: blockHeaderFromRpc(parent) };
  const gasPrice = BigInt(a['gas-price']);

  console.log(`LAUNCH_CALLDATA_KECCAK=${ethers.keccak256(launchData)}`);
  const launchRes = await evm.runCall({
    caller: launcherAddr, origin: launcherAddr, to: portalAddr, value: 0n,
    data: hexToBytes(launchData), gasLimit: BigInt(a['launch-gas']), gasPrice, block,
  });
  if (launchRes.execResult.exceptionError) {
    console.log(`STATUS=LAUNCH_REVERTED ${launchRes.execResult.exceptionError.error}`);
    return;
  }
  const launchReturn = bytesToHex(launchRes.execResult.returnValue);
  if (launchReturn.length !== 66) throw new Error('newTokenV6 returned no address');
  const token = ethers.getAddress('0x' + launchReturn.slice(-40));
  console.log(`TOKEN=${token.toLowerCase()}`);
  console.log(`LAUNCH_GAS=${launchRes.execResult.executionGasUsed}`);

  const buyData = buyCalldata(token, buyValue);
  console.log(`BUY_CALLDATA_KECCAK=${ethers.keccak256(buyData)}`);
  const buyRes = await evm.runCall({
    caller: buyerAddr, origin: buyerAddr, to: portalAddr, value: buyValue,
    data: hexToBytes(buyData), gasLimit: BigInt(a['buy-gas']), gasPrice, block,
  });
  if (buyRes.execResult.exceptionError) {
    console.log(`STATUS=BUY_REVERTED ${buyRes.execResult.exceptionError.error}`);
    return;
  }
  const retHex = bytesToHex(buyRes.execResult.returnValue);
  console.log(`BUY_RETURN=${BigInt(retHex === '0x' ? '0x0' : retHex)}`);
  console.log(`BUY_GAS=${buyRes.execResult.executionGasUsed}`);
  for (const [address, topics, data] of buyRes.execResult.logs || []) {
    if (bytesToHex(address).toLowerCase() !== PORTAL) continue;
    const t0 = bytesToHex(topics[0]);
    if (t0 === TOKEN_BOUGHT) {
      console.log(`BOUGHT_AMOUNT=${word(data, 3)}`);
      console.log(`BOUGHT_ETH=${word(data, 4)}`);
      console.log(`BOUGHT_FEE=${word(data, 5)}`);
    } else if (TAX_PAID.includes(t0)) {
      console.log(`TAX_PAID=${word(data, 0)}`);
    }
  }
  const balRes = await evm.runCall({
    caller: buyerAddr, origin: buyerAddr, to: createAddressFromString(token), value: 0n,
    data: hexToBytes(balanceOfCalldata(buyer)), gasLimit: BigInt(a['balance-gas']), gasPrice: 0n, block, isStatic: true,
  });
  if (balRes.execResult.exceptionError) throw new Error('local balanceOf failed');
  const balHex = bytesToHex(balRes.execResult.returnValue);
  console.log(`BUYER_BALANCE=${BigInt(balHex === '0x' ? '0x0' : balHex)}`);
  console.log(`PROVEN_ACCOUNTS=${reads.accounts}`);
  await requireBlock(rpc, rpcUrl, blockNumber, a.hash, 'after the run');
  console.log('STATUS=SUCCESS');
}

main().catch((e) => {
  console.error(String(e && e.message ? e.message : e));
  process.exitCode = 1;
});
