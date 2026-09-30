const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

const secret = fs.readFileSync(0, 'utf8').trim();
if (!secret.startsWith('https://')) throw Error('HTTPS endpoint required');

const url = secret.replace(/^https:/, 'wss:');
const OUT = path.resolve(__dirname, '../evidence/g04_quicknode_coverage_v2');
fs.mkdirSync(OUT, {recursive:true});

const TOTAL_MS = 600000;       // 10 min
const CENSUS_START_MS = 30000; // 30 s warmup
const CENSUS_END_MS = 570000;  // 30 s tail

const start = process.hrtime.bigint();
const elapsed = () => process.hrtime.bigint() - start;

const portal = '0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0';

const subscriptions = {};
const pendingHashes = new Set();
const portalPending = [];
const portalLogs = [];
const heads = [];
const canonicalPortalTxs = {};
const errors = [];

let reqId = 1000;
const txRequests = new Map();
let stopping = false;

const stats = {
  pending_full: 0,
  pending_hash_only: 0,
  pending_missing_fields: 0,
  portal_pending: 0,
  portal_logs: 0,
  heads: 0,
  canonical_portal_tx_fetches: 0
};

const ws = new WebSocket(url);

function sendRpc(method, params, meta=null) {
  const id = reqId++;
  if (meta) txRequests.set(id, meta);
  ws.send(JSON.stringify({jsonrpc:'2.0', id, method, params}));
  return id;
}

function finalize(reason) {
  if (stopping) return;
  stopping = true;

  setTimeout(() => {
    const data = {
      scope: 'QuickNode G0.4 canonical-baseline probe; not own-geth/builder qualification',
      total_duration_ns: elapsed().toString(),
      planned_total_ms: TOTAL_MS,
      census_start_ms: CENSUS_START_MS,
      census_end_ms: CENSUS_END_MS,
      stop_reason: reason,
      subscriptions,
      stats,
      errors,
      pending_hashes: [...pendingHashes],
      portal_pending: portalPending,
      portal_logs: portalLogs,
      heads,
      canonical_portal_txs: canonicalPortalTxs,
      claims: {
        T_send: null,
        T_buy: null,
        T0: null,
        buy_latency: null
      }
    };

    const body = JSON.stringify(data, null, 2) + '\n';

    if (
      body.includes(secret) ||
      body.includes(url) ||
      body.includes(secret.split('/').filter(Boolean).at(-1))
    ) throw Error('Credential detected; refusing persistence');

    const capture = path.join(OUT, 'capture.json');
    fs.writeFileSync(capture, body);

    const digest = crypto.createHash('sha256').update(body).digest('hex');
    fs.writeFileSync(
      path.join(OUT, 'sha256.json'),
      JSON.stringify({'capture.json':digest}, null, 2) + '\n'
    );

    console.log(JSON.stringify({
      stop_reason: reason,
      duration_s: Number(elapsed()) / 1e9,
      unique_pending_hashes: pendingHashes.size,
      ...stats,
      outstanding_rpc: txRequests.size,
      errors: errors.length
    }, null, 2));

    try { ws.close(); } catch {}
    setTimeout(() => process.exit(errors.length ? 2 : 0), 100);
  }, 3000);
}

ws.addEventListener('open', () => {
  ws.send(JSON.stringify({
    jsonrpc:'2.0', id:1, method:'eth_subscribe',
    params:['newPendingTransactions', true]
  }));

  ws.send(JSON.stringify({
    jsonrpc:'2.0', id:2, method:'eth_subscribe',
    params:['logs', {address:portal}]
  }));

  ws.send(JSON.stringify({
    jsonrpc:'2.0', id:3, method:'eth_subscribe',
    params:['newHeads']
  }));
});

ws.addEventListener('message', event => {
  let d;
  try {
    d = JSON.parse(String(event.data));
  } catch {
    errors.push({error:'non-JSON frame'});
    return;
  }

  if (d.error) {
    errors.push({id:d.id, code:d.error.code, message:d.error.message});
    txRequests.delete(d.id);
    return;
  }

  if (d.id === 1 || d.id === 2 || d.id === 3) {
    subscriptions[d.id] = d.result;
    return;
  }

  if (txRequests.has(d.id)) {
    const meta = txRequests.get(d.id);
    txRequests.delete(d.id);

    if (d.result) {
      canonicalPortalTxs[meta.hash] = {
        fetched_local_ns: elapsed().toString(),
        first_log_local_ns: meta.first_log_local_ns,
        tx: d.result
      };
      stats.canonical_portal_tx_fetches++;
    }
    return;
  }

  if (stopping || !d.params) return;

  const sub = d.params.subscription;
  const v = d.params.result;
  const now = elapsed().toString();

  if (sub === subscriptions[1]) {
    if (typeof v === 'string') {
      stats.pending_hash_only++;
      pendingHashes.add(v.toLowerCase());
      return;
    }

    stats.pending_full++;

    if (!['hash','from','to','input','value','nonce','gas'].every(k => k in v)) {
      stats.pending_missing_fields++;
    }

    if (v.hash) pendingHashes.add(v.hash.toLowerCase());

    if (v.to?.toLowerCase() === portal) {
      stats.portal_pending++;
      portalPending.push({
        local_received_ns: now,
        tx: v
      });
    }

    return;
  }

  if (sub === subscriptions[2]) {
    stats.portal_logs++;

    portalLogs.push({
      local_received_ns: now,
      log: v
    });

    const hash = v.transactionHash?.toLowerCase();

    if (
      hash &&
      !canonicalPortalTxs[hash] &&
      ![...txRequests.values()].some(x => x.hash === hash)
    ) {
      sendRpc('eth_getTransactionByHash', [hash], {
        hash,
        first_log_local_ns: now
      });
    }

    return;
  }

  if (sub === subscriptions[3]) {
    stats.heads++;
    heads.push({
      local_received_ns: now,
      number: v.number,
      hash: v.hash,
      parentHash: v.parentHash,
      timestamp: v.timestamp
    });
  }
});

ws.addEventListener('error', () => {
  errors.push({error:'WebSocket connection error; endpoint redacted'});
  finalize('connection error');
});

ws.addEventListener('close', () => {
  if (!stopping) finalize('server closed');
});

setTimeout(() => finalize('600-second bound'), TOTAL_MS);
