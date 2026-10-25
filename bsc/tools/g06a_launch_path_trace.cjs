#!/usr/bin/env node
// Launch-path code identity snapshot (v9.1 C1/R1), read-only. Runs the
// candidate's newTokenV6 launch and first swapExactInput BUY in a local
// EthereumJS EVM over RPCStateManager at exactly the preflight's block and
// records, as one DBREC launch-path record: every contract with code at that
// block that executed (code SHA-256), and every storage word of such a
// contract that selects an executed code address (proxy implementation
// slots, storage-selected clone targets). No key, no signature, no send; the
// salt comes only from this process's environment and is never printed.
// Usage: node g06a_launch_path_trace.cjs <preflight.dbrec> <launch_path.dbrec>
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
const { pin: statePin, pinState, requireBlock } = require('./g06a_state_pin.cjs');

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

const PREFLIGHT_SCHEMA = 'darkbot.g06a.preflight-readonly.v3';
const LAUNCH_PATH_SCHEMA = 'darkbot.g06a.launch-path';
const TRACE_METHOD = 'local-evm-message+sload-trace:newTokenV6+swapExactInput';

async function main() {
  const [preflightPath, outPath] = process.argv.slice(2);
  if (!preflightPath || !outPath) throw new Error('usage: node g06a_launch_path_trace.cjs <preflight.dbrec> <launch_path.dbrec>');
  const rpcUrl = req('BSC_MAINNET_RPC_HTTPS');
  const salt = req('DARKBOT_G06A_SALT');
  const pre = dbrec.decode(PREFLIGHT_SCHEMA, require('fs').readFileSync(preflightPath)).fields;
  if (pre.chain_id !== CHAIN_ID) throw new Error('preflight is not BSC Mainnet');
  const portal = pre.portal;
  if (portal !== PORTAL) throw new Error('Portal mismatch');
  const number = pre.latest_block_number;
  const tag = '0x' + number.toString(16);
  const parent = await rpc(rpcUrl, 'eth_getBlockByNumber', [tag, false]);
  if (!parent || parent.hash.toLowerCase() !== pre.latest_block_hash.toLowerCase()) {
    throw new Error('the preflight block is no longer canonical on this endpoint');
  }
  const pin = { blockHash: parent.hash, requireCanonical: true };
  const token = predictedToken(portal, pre.tax_v3_impl, salt);
  // The salt, the launch calldata and the token never leave the process.
  guardEgress([salt, launchCalldata(pre.launcher_address, salt), token]);
  const common = createCustomCommon({ chainId: Number(CHAIN_ID) }, Mainnet, { hardfork: Hardfork.Osaka });
  const state = new RPCStateManager({ provider: rpcUrl, blockTag: number });
  pinState(state, parent.hash);
  installBasicAccountLoader(state, rpcUrl, statePin(parent.hash));
  const evm = await createEVM({ common, stateManager: state, blockchain: new RPCBlockChain(rpcUrl) });
  const created = localCreations(evm, state, [token]);
  const block = { header: blockHeaderFromRpc(parent) };
  const roles = new Map();
  const loads = new Map();
  let phase = 'launch';
  evm.events.on('beforeMessage', (m) => {
    let code = null;
    try { code = m.codeAddress.toString(); } catch { code = m.to ? m.to.toString() : null; }
    if (!code) return;
    const kind = m.delegatecall ? 'DELEGATECALL' : (m.isStatic ? 'STATICCALL' : 'CALL');
    const set = roles.get(code) || new Set();
    set.add(`${phase}:${kind}`);
    roles.set(code, set);
  });
  evm.events.on('step', (s) => {
    if (s.opcode.name !== 'SLOAD') return;
    const slot = '0x' + s.stack[s.stack.length - 1].toString(16).padStart(64, '0');
    loads.set(`${s.address.toString()} ${slot}`, [s.address.toString(), slot]);
  });
  const launcher = createAddressFromString(pre.launcher_address);
  const buyer = createAddressFromString(pre.buyer_address);
  const dryBalance = 100_000_000_000_000_000_000n;
  await state.putAccount(launcher, new Account(pre.launcher_pending_nonce, dryBalance));
  await state.putAccount(buyer, new Account(pre.buyer_pending_nonce, dryBalance));
  const launch = await evm.runCall({ caller: launcher, origin: launcher, to: createAddressFromString(portal), value: pre['wallet_qualification.funding_profile.launch_value_wei'],
    data: hexToBytes(launchCalldata(pre.launcher_address, salt)), gasLimit: pre['wallet_qualification.funding_profile.launch_gas_limit'],
    gasPrice: pre.gas_price_wei, block, skipBalance: true });
  const launchRet = bytesToHex(launch.execResult.returnValue);
  phase = 'buy';
  const buyValue = pre['wallet_qualification.funding_profile.buy_value_wei'];
  // minOutput 1: the code path, not the economics (those are DRY-2/3).
  const buy = await evm.runCall({ caller: buyer, origin: buyer, to: createAddressFromString(portal), value: buyValue,
    data: hexToBytes(buyCalldata(token, buyValue, 1n)), gasLimit: pre['wallet_qualification.funding_profile.buy_gas_limit'],
    gasPrice: pre.gas_price_wei, block, skipBalance: true });
  const code = { address: [], sha: [], bytes: [], roles: [] };
  const executed = new Set();
  for (const [address, set] of [...roles].sort(([a], [b]) => a.localeCompare(b))) {
    // Created during the run (never asked of the RPC): its creator's code is bound.
    if (created.has(address.toLowerCase().replace(/^0x/, ''))) continue;
    const bytecode = await rpc(rpcUrl, 'eth_getCode', [address, pin]);
    if (bytecode === '0x') continue;
    executed.add(address.toLowerCase());
    code.address.push(address);
    code.sha.push('0x' + require('crypto').createHash('sha256').update(Buffer.from(bytecode.slice(2), 'hex')).digest('hex'));
    code.bytes.push(BigInt((bytecode.length - 2) / 2));
    code.roles.push([...set].sort().join(' '));
  }
  const storage = { address: [], slot: [], value: [], selects: [] };
  for (const [address, slot] of [...loads.values()].sort((a, b) => (a[0] + a[1]).localeCompare(b[0] + b[1]))) {
    if (!executed.has(address.toLowerCase())) continue;
    const value = await rpc(rpcUrl, 'eth_getStorageAt', [address, slot, pin]);
    const word = value.toLowerCase().replace(/^0x/, '').padStart(64, '0');
    const selects = '0x' + word.slice(24);
    if (word.slice(0, 24) !== '0'.repeat(24) || !executed.has(selects)) continue;
    storage.address.push(address);
    storage.slot.push(slot);
    storage.value.push('0x' + word);
    storage.selects.push(selects);
  }
  const record = dbrec.encode(LAUNCH_PATH_SCHEMA, {
    '_provenance.tool': 'bsc/tools/g06a_launch_path_trace.cjs',
    '_provenance.observed_unix_ms': BigInt(Date.now()),
    block_number: number,
    block_hash: parent.hash,
    portal,
    method: TRACE_METHOD,
    token,
    launch_success: !launch.execResult.exceptionError,
    buy_success: !buy.execResult.exceptionError,
    launch_return_token: '0x' + launchRet.slice(-40),
    'code.address': code.address,
    'code.code_sha256': code.sha,
    'code.code_bytes': code.bytes,
    'code.roles': code.roles,
    'storage.address': storage.address,
    'storage.slot': storage.slot,
    'storage.value': storage.value,
    'storage.selects': storage.selects,
  });
  await requireBlock(rpc, rpcUrl, number, parent.hash, 'before the evidence write');
  dbrec.writeNew(outPath, record);
  console.log(`LAUNCH_PATH=WRITTEN\nRECORD=${outPath}\nBLOCK=${number}\nCODE_ENTRIES=${code.address.length}\nSTORAGE_SELECTORS=${storage.address.length}\nLAUNCH_OK=${!launch.execResult.exceptionError}\nBUY_OK=${!buy.execResult.exceptionError}`);
}
main().catch((e) => { console.error(String(e && e.message || e)); process.exit(1); });
