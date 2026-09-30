const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

const httpsUrl = process.env.BSC_QN_HTTPS || '';
const wssUrlEnv = process.env.BSC_QN_WSS || '';
let url = wssUrlEnv;
if (!url && httpsUrl.startsWith('https://')) url = httpsUrl.replace(/^https:/, 'wss:');

if (!url.startsWith('wss://')) {
  console.error('Set BSC_QN_WSS or BSC_QN_HTTPS locally before running.');
  process.exit(2);
}

const OUT = path.resolve(__dirname, '../evidence/g06_flap_exact_scope_pending');
fs.mkdirSync(OUT, {recursive:true});

const MAX_MS = Number(process.env.G06_FLAP_EXACT_MS || 600000);
const PORTAL = '0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0';
const SELECTOR = '0x8cb5772c';
const DOGE = 'ba2ae424d960c26247dd6c32edc70b295c744c43';
const EXT = '3526f6a0e790306d64c896eeda5ec66052ea17889e20632ff0cf512923153dad';

const start = process.hrtime.bigint();
const elapsedNs = () => process.hrtime.bigint() - start;

let subscriptionId = null;
let stopping = false;
let fullPending = 0;
let flapV6 = 0;
let exactScope = 0;
const mismatches = [];
const errors = [];

const ws = new WebSocket(url);

function word(inputNo0x, tupleStartHex, index) {
  const start = tupleStartHex + index * 64;
  return inputNo0x.slice(start, start + 64).toLowerCase();
}

function decodeScopeFields(input) {
  const hex = input.slice(2);
  if (!hex.startsWith(SELECTOR.slice(2))) return null;

  const args = hex.slice(8);
  if (args.length < 64) return null;

  const tupleOffsetBytes = Number(BigInt('0x' + args.slice(0, 64)));
  const tupleStartHex = 64 + tupleOffsetBytes * 2;

  const quoteWord = word(args, tupleStartHex, 6);
  const extensionWord = word(args, tupleStartHex, 10);
  const tokenVersionWord = word(args, tupleStartHex, 25);

  if ([quoteWord, extensionWord, tokenVersionWord].some(x => x.length !== 64)) return null;

  return {
    quote_token: '0x' + quoteWord.slice(24),
    extension_id: '0x' + extensionWord,
    token_version: Number(BigInt('0x' + tokenVersionWord))
  };
}

function persist(reason, tx, fields) {
  if (stopping) return;
  stopping = true;

  const data = {
    status: tx
      ? 'G0.6_FLAP_EXACT_FROZEN_SCOPE_PENDING_CAPTURED'
      : 'G0.6_FLAP_EXACT_FROZEN_SCOPE_PENDING_NOT_OBSERVED',
    source: 'QuickNode BSC eth_subscribe newPendingTransactions flag=true',
    duration_ns: elapsedNs().toString(),
    planned_max_ms: MAX_MS,
    stop_reason: reason,
    subscription_established: !!subscriptionId,
    stats: {
      full_pending: fullPending,
      flap_new_token_v6: flapV6,
      exact_frozen_scope_matches: exactScope,
      stored_scope_mismatches: mismatches.length
    },
    frozen_scope: {
      portal: PORTAL,
      selector: SELECTOR,
      token_version: 6,
      quote_token: '0x' + DOGE,
      extension_id: '0x' + EXT
    },
    matched_scope_fields: fields ?? null,
    pending_transaction: tx ?? null,
    recent_scope_mismatches: mismatches,
    errors,
    safety: {
      read_only: true,
      transaction_submission: false,
      value_bearing_buy: false
    },
    non_claims: [
      'Exact pending-scope capture does not prove canonical coverage',
      'No builder inclusion or ordering proof',
      'No T0, T_send or T_buy measurement'
    ]
  };

  const body = JSON.stringify(data, null, 2) + '\n';
  if ((httpsUrl && body.includes(httpsUrl)) ||
      (wssUrlEnv && body.includes(wssUrlEnv)) ||
      (url && body.includes(url))) {
    throw new Error('Endpoint detected in evidence; refusing persistence');
  }

  fs.writeFileSync(path.join(OUT, 'pending_sample.json'), body);
  const sha = crypto.createHash('sha256').update(body).digest('hex');
  fs.writeFileSync(
    path.join(OUT, 'sha256.json'),
    JSON.stringify({'pending_sample.json': sha}, null, 2) + '\n'
  );

  console.log(body.trim());
  console.log('pending_sample_sha256:', sha);
  try { ws.close(); } catch {}
  setTimeout(() => process.exit(errors.length ? 3 : 0), 50);
}

ws.addEventListener('open', () => {
  ws.send(JSON.stringify({
    jsonrpc: '2.0',
    id: 1,
    method: 'eth_subscribe',
    params: ['newPendingTransactions', true]
  }));
});

ws.addEventListener('message', event => {
  let msg;
  try { msg = JSON.parse(String(event.data)); }
  catch {
    errors.push({kind:'non_json_frame'});
    return;
  }

  if (msg.error) {
    errors.push({id:msg.id ?? null, code:msg.error.code ?? null, message:msg.error.message ?? 'unknown'});
    persist('provider_error', null, null);
    return;
  }

  if (msg.id === 1) {
    subscriptionId = msg.result;
    return;
  }

  if (!msg.params || msg.params.subscription !== subscriptionId) return;
  const tx = msg.params.result;
  if (!tx || typeof tx !== 'object') return;

  fullPending++;

  const to = typeof tx.to === 'string' ? tx.to.toLowerCase() : '';
  const input = typeof tx.input === 'string' ? tx.input.toLowerCase() : '';

  if (to !== PORTAL || !input.startsWith(SELECTOR)) return;

  flapV6++;

  const fields = decodeScopeFields(input);
  if (!fields) return;

  const isExact =
    fields.token_version === 6 &&
    fields.quote_token === '0x' + DOGE &&
    fields.extension_id === '0x' + EXT;

  if (isExact) {
    exactScope++;
    persist('exact_frozen_scope_pending_launch_observed', {
      local_received_ns: elapsedNs().toString(),
      hash: tx.hash,
      from: tx.from,
      to: tx.to,
      nonce: tx.nonce,
      value: tx.value,
      gas: tx.gas,
      gasPrice: tx.gasPrice ?? null,
      maxFeePerGas: tx.maxFeePerGas ?? null,
      maxPriorityFeePerGas: tx.maxPriorityFeePerGas ?? null,
      type: tx.type ?? null,
      input: tx.input
    }, fields);
    return;
  }

  if (mismatches.length < 20) {
    mismatches.push({
      local_received_ns: elapsedNs().toString(),
      hash: tx.hash ?? null,
      ...fields
    });
  }
});

ws.addEventListener('error', () => {
  errors.push({kind:'websocket_error'});
  persist('websocket_error', null, null);
});

ws.addEventListener('close', () => {
  if (!stopping) persist('server_closed', null, null);
});

setTimeout(() => persist('time_bound_reached', null, null), MAX_MS);
