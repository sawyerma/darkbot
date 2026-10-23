#!/usr/bin/env node
// Read-only launch-path trace (v9.1 C1/R1): which contracts the Flap
// newTokenV6 launch and the first swapExactInput BUY actually execute on the
// current Mainnet state, with the code hash of each at one pinned block.
// Local EthereumJS execution over RPCStateManager; no key, no signature, no
// send; a throwaway public vanity salt (never the A or B candidate salt) and
// dummy callers. Text output only (no JSON): one line per executed code.
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
const dbrec = null;

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
  const rpcUrl = req('BSC_MAINNET_RPC_HTTPS');
  const impl = '0x024f18294970b5c76c0691b87f138a0317156422';
  let salt;
  let token;
  for (let i = 0; ; i++) {
    salt = ethers.keccak256(ethers.toUtf8Bytes('darkbot-c1-trace-throwaway:' + i));
    token = predictedToken(PORTAL, impl, salt);
    if (token.toLowerCase().endsWith('7777')) break;
  }
  const head = await rpc(rpcUrl, 'eth_getBlockByNumber', ['latest', false]);
  const number = BigInt(head.number);
  const tag = '0x' + number.toString(16);
  const pin = { blockHash: head.hash, requireCanonical: true };
  const common = createCustomCommon({ chainId: 56 }, Mainnet, { hardfork: Hardfork.Osaka });
  const state = new RPCStateManager({ provider: rpcUrl, blockTag: number });
  installBasicAccountLoader(state, rpcUrl, tag);
  const evm = await createEVM({ common, stateManager: state, blockchain: new RPCBlockChain(rpcUrl) });
  const block = { header: blockHeaderFromRpc(head) };
  const seen = new Map();
  let phase = 'launch';
  evm.events.on('beforeMessage', (m) => {
    let code = null;
    try { code = m.codeAddress.toString(); } catch { code = m.to ? m.to.toString() : null; }
    if (!code) return;
    const kind = m.delegatecall ? 'DELEGATECALL' : (m.isStatic ? 'STATICCALL' : 'CALL');
    const context = m.to ? m.to.toString() : '-';
    const key = `${phase} ${kind} ${context} ${code}`;
    seen.set(key, (seen.get(key) || 0) + 1);
  });
  const caller = createAddressFromString('0x000000000000000000000000000000000000a11c');
  const buyer = createAddressFromString('0x000000000000000000000000000000000000b0b0');
  await state.putAccount(caller, new Account(0n, 10n ** 20n));
  await state.putAccount(buyer, new Account(0n, 10n ** 20n));
  const portal = createAddressFromString(PORTAL);
  const launch = await evm.runCall({ caller, origin: caller, to: portal, value: 0n,
    data: hexToBytes(launchCalldata(caller.toString(), salt)), gasLimit: 8000000n, gasPrice: 50000000n, block, skipBalance: true });
  const launchRet = bytesToHex(launch.execResult.returnValue);
  phase = 'buy';
  const buy = await evm.runCall({ caller: buyer, origin: buyer, to: portal, value: 1000000000000n,
    data: hexToBytes(buyCalldata(token, 1000000000000n, 1n)), gasLimit: 3000000n, gasPrice: 50000000n, block, skipBalance: true });
  const slot = '0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc';
  console.log(`LAUNCH_PATH_TRACE block=${number} hash=${head.hash} predicted_token=${token} launch_return=${launchRet} launch_ok=${!launch.execResult.exceptionError} buy_ok=${!buy.execResult.exceptionError}`);
  console.log('phase kind context code_address code_bytes code_sha256 eip1967_implementation count');
  for (const [key, count] of seen) {
    const code = key.split(' ')[3];
    const bytecode = await rpc(rpcUrl, 'eth_getCode', [code, pin]);
    const implSlot = await rpc(rpcUrl, 'eth_getStorageAt', [code, slot, pin]);
    const proxied = BigInt(implSlot) === 0n ? '-' : '0x' + implSlot.slice(-40);
    const sha = require('crypto').createHash('sha256').update(Buffer.from(bytecode.slice(2), 'hex')).digest('hex');
    console.log(`${key} ${(bytecode.length - 2) / 2} ${sha} ${proxied} ${count}`);
  }
}
main().catch((e) => { console.error(String(e && e.message || e)); process.exit(1); });
