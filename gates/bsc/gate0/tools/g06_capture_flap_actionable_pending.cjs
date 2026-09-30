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

const OUT = path.resolve(__dirname, '../evidence/g06_flap_actionable_pending');
fs.mkdirSync(OUT, {recursive:true});

const MAX_MS = Number(process.env.G06_FLAP_MS || 300000);
const PORTAL = '0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0';
const SELECTOR = '0x8cb5772c';

const start = process.hrtime.bigint();
const elapsedNs = () => process.hrtime.bigint() - start;

let subscriptionId = null;
let stopping = false;
let fullPending = 0;
let hashOnly = 0;
let missingFields = 0;
const errors = [];

const ws = new WebSocket(url);

function persist(reason, tx) {
  if (stopping) return;
  stopping = true;

  const sample = tx ? {
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
  } : null;

  const data = {
    status: sample
      ? 'G0.6_FLAP_ACTIONABLE_PENDING_SAMPLE_CAPTURED'
      : 'G0.6_FLAP_ACTIONABLE_PENDING_SAMPLE_NOT_OBSERVED',
    source: 'QuickNode BSC eth_subscribe newPendingTransactions flag=true',
    duration_ns: elapsedNs().toString(),
    planned_max_ms: MAX_MS,
    stop_reason: reason,
    subscription_established: !!subscriptionId,
    stats: {
      full_pending: fullPending,
      hash_only: hashOnly,
      missing_required_fields: missingFields
    },
    exact_scope: {
      portal: PORTAL,
      selector: SELECTOR
    },
    pending_transaction: sample,
    errors,
    safety: {
      read_only: true,
      transaction_submission: false,
      value_bearing_buy: false
    },
    non_claims: [
      'Capture proves pending field availability only',
      'No canonical coverage denominator',
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

  const file = path.join(OUT, 'pending_sample.json');
  fs.writeFileSync(file, body);
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
    persist('provider_error', null);
    return;
  }

  if (msg.id === 1) {
    subscriptionId = msg.result;
    return;
  }

  if (!msg.params || msg.params.subscription !== subscriptionId) return;
  const tx = msg.params.result;

  if (typeof tx === 'string') {
    hashOnly++;
    return;
  }
  if (!tx || typeof tx !== 'object') return;

  fullPending++;
  const required = ['hash','from','to','input','value','nonce','gas'];
  if (!required.every(k => Object.prototype.hasOwnProperty.call(tx, k))) missingFields++;

  const to = typeof tx.to === 'string' ? tx.to.toLowerCase() : '';
  const input = typeof tx.input === 'string' ? tx.input.toLowerCase() : '';

  if (to === PORTAL && input.startsWith(SELECTOR)) {
    persist('exact_flap_pending_launch_observed', tx);
  }
});

ws.addEventListener('error', () => {
  errors.push({kind:'websocket_error'});
  persist('websocket_error', null);
});

ws.addEventListener('close', () => {
  if (!stopping) persist('server_closed', null);
});

setTimeout(() => persist('time_bound_reached', null), MAX_MS);
