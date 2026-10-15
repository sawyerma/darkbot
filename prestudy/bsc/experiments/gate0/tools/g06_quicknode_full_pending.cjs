const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

const httpsUrl = process.env.BSC_QN_HTTPS || '';
const wssUrlEnv = process.env.BSC_QN_WSS || '';

let url = wssUrlEnv;
if (!url && httpsUrl.startsWith('https://')) {
  url = httpsUrl.replace(/^https:/, 'wss:');
}

if (!url.startsWith('wss://')) {
  console.error('Set BSC_QN_WSS or BSC_QN_HTTPS locally before running.');
  process.exit(2);
}

const OUT = path.resolve(__dirname, '../evidence/g06_quicknode_full_pending');
fs.mkdirSync(OUT, {recursive:true});

const MAX_MS = Number(process.env.G06_INGRESS_MS || 300000);
const MIN_FULL_PENDING = Number(process.env.G06_MIN_FULL_PENDING || 1000);

const PORTAL = '0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0';
const NEW_TOKEN_V6_SELECTOR = '0x8cb5772c';

const start = process.hrtime.bigint();
const elapsedNs = () => process.hrtime.bigint() - start;

const stats = {
  full_pending: 0,
  hash_only: 0,
  missing_required_fields: 0,
  portal_pending: 0,
  portal_new_token_v6: 0
};

const portal = [];
const errors = [];
let subscriptionId = null;
let stopping = false;

const ws = new WebSocket(url);

function redact(text) {
  if (!text) return text;
  let out = String(text);
  if (httpsUrl) out = out.split(httpsUrl).join('<REDACTED>');
  if (wssUrlEnv) out = out.split(wssUrlEnv).join('<REDACTED>');
  if (url) out = out.split(url).join('<REDACTED>');
  return out;
}

function persist(reason) {
  if (stopping) return;
  stopping = true;

  const exactFlapActionable = stats.portal_new_token_v6 > 0;
  const fullPendingCapability =
    stats.full_pending >= MIN_FULL_PENDING &&
    stats.hash_only === 0 &&
    stats.missing_required_fields === 0;

  const status =
    exactFlapActionable && fullPendingCapability
      ? 'G0.6_INGRESS_DRY_QUALIFICATION_PASS'
      : fullPendingCapability
        ? 'G0.6_INGRESS_SOURCE_CAPABILITY_PASS_NO_FLAP_SAMPLE'
        : 'G0.6_INGRESS_DRY_QUALIFICATION_PARTIAL';

  const data = {
    status,
    source: 'QuickNode BSC eth_subscribe newPendingTransactions flag=true',
    duration_ns: elapsedNs().toString(),
    planned_max_ms: MAX_MS,
    stop_reason: reason,
    subscription_established: !!subscriptionId,
    stats,
    field_contract: {
      required: ['hash','from','to','input','value','nonce','gas'],
      full_pending_capability: fullPendingCapability
    },
    exact_flap_scope: {
      portal: PORTAL,
      selector: NEW_TOKEN_V6_SELECTOR,
      observed_pending_launches: stats.portal_new_token_v6,
      actionable_sample_observed: exactFlapActionable
    },
    portal_pending_samples: portal,
    errors,
    safety: {
      read_only: true,
      transaction_submission: false,
      value_bearing_buy: false
    },
    non_claims: [
      'No canonical coverage denominator',
      'No T0 measurement',
      'No T_send or T_buy measurement',
      'No builder inclusion or ordering proof'
    ]
  };

  const body = JSON.stringify(data, null, 2) + '\n';

  if (
    (httpsUrl && body.includes(httpsUrl)) ||
    (wssUrlEnv && body.includes(wssUrlEnv)) ||
    (url && body.includes(url))
  ) {
    throw new Error('Credential/endpoint detected in evidence; refusing persistence');
  }

  const capture = path.join(OUT, 'summary.json');
  fs.writeFileSync(capture, body);

  const sha = crypto.createHash('sha256').update(body).digest('hex');
  fs.writeFileSync(
    path.join(OUT, 'sha256.json'),
    JSON.stringify({'summary.json': sha}, null, 2) + '\n'
  );

  console.log(body.trim());
  console.log('summary_sha256:', sha);

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
  try {
    msg = JSON.parse(String(event.data));
  } catch {
    errors.push({kind:'non_json_frame'});
    return;
  }

  if (msg.error) {
    errors.push({
      id: msg.id ?? null,
      code: msg.error.code ?? null,
      message: redact(msg.error.message ?? 'unknown')
    });
    persist('subscription_or_provider_error');
    return;
  }

  if (msg.id === 1) {
    subscriptionId = msg.result;
    return;
  }

  if (!msg.params || msg.params.subscription !== subscriptionId) return;

  const tx = msg.params.result;

  if (typeof tx === 'string') {
    stats.hash_only++;
    return;
  }

  if (!tx || typeof tx !== 'object') return;

  stats.full_pending++;

  const required = ['hash','from','to','input','value','nonce','gas'];
  if (!required.every(k => Object.prototype.hasOwnProperty.call(tx, k))) {
    stats.missing_required_fields++;
  }

  const to = typeof tx.to === 'string' ? tx.to.toLowerCase() : null;
  const input = typeof tx.input === 'string' ? tx.input.toLowerCase() : '';

  if (to === PORTAL) {
    stats.portal_pending++;

    const isNewTokenV6 = input.startsWith(NEW_TOKEN_V6_SELECTOR);
    if (isNewTokenV6) stats.portal_new_token_v6++;

    if (portal.length < 50) {
      portal.push({
        local_received_ns: elapsedNs().toString(),
        hash: tx.hash ?? null,
        from: tx.from ?? null,
        to: tx.to ?? null,
        nonce: tx.nonce ?? null,
        value: tx.value ?? null,
        gas: tx.gas ?? null,
        input_length_hex_chars: input.length,
        selector: input.slice(0, 10),
        is_new_token_v6: isNewTokenV6
      });
    }
  }

  if (
    stats.full_pending >= MIN_FULL_PENDING &&
    stats.portal_new_token_v6 > 0 &&
    stats.hash_only === 0 &&
    stats.missing_required_fields === 0
  ) {
    persist('exact_flap_actionable_sample_observed');
  }
});

ws.addEventListener('error', () => {
  errors.push({kind:'websocket_error'});
  persist('websocket_error');
});

ws.addEventListener('close', () => {
  if (!stopping) persist('server_closed');
});

setTimeout(() => persist('time_bound_reached'), MAX_MS);
