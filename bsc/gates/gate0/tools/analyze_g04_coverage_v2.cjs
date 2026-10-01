const fs = require('fs');
const path = require('path');
const assert = require('assert/strict');
const crypto = require('crypto');

const ROOT = path.resolve(__dirname, '../..');
const P = path.join(ROOT, 'gate0/evidence/g04_quicknode_coverage_v2');

const E = require(
  path.join(
    ROOT,
    'tests/DARKBOT 2/gate0-evidence/abi-tools/node_modules/ethers'
  )
);

const bytes = fs.readFileSync(path.join(P, 'capture.json'));
const manifest = JSON.parse(fs.readFileSync(path.join(P, 'sha256.json')));

assert.equal(
  crypto.createHash('sha256').update(bytes).digest('hex'),
  manifest['capture.json']
);

const d = JSON.parse(bytes);

const portal = '0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0';
const doge = '0xba2ae424d960c26247dd6c32edc70b295c744c43';
const extension =
  '0x3526f6a0e790306d64c896eeda5ec66052ea17889e20632ff0cf512923153dad';

const types = [
  'string','string','string','uint8','bytes32','uint8','address','uint256',
  'address','bytes','bytes32','bytes','uint8','uint8','uint16','uint16',
  'uint64','uint64','uint16','uint16','uint16','uint16','uint256',
  'address','address','uint8'
];

const it = new E.Interface([
  'function newTokenV6((' + types.join(',') + ')) payable returns(address)'
]);

const pending = new Set(d.pending_hashes.map(x => x.toLowerCase()));

const pendingPortal = new Map(
  d.portal_pending
    .filter(x => x.tx?.hash)
    .map(x => [x.tx.hash.toLowerCase(), x])
);

const startNs = BigInt(d.census_start_ms) * 1000000n;
const endNs = BigInt(d.census_end_ms) * 1000000n;

const launches = new Map();

for (const item of d.portal_logs) {
  const t = BigInt(item.local_received_ns);

  if (t < startNs || t > endNs) continue;
  if (item.log?.removed) continue;

  const hash = item.log?.transactionHash?.toLowerCase();
  if (!hash || launches.has(hash)) continue;

  const found = d.canonical_portal_txs[hash];
  if (!found?.tx) continue;

  const tx = found.tx;

  if (tx.to?.toLowerCase() !== portal) continue;
  if (!tx.input?.startsWith('0x8cb5772c')) continue;

  const lp = it.decodeFunctionData('newTokenV6', tx.input)[0];
  assert.equal(it.encodeFunctionData('newTokenV6', [lp]), tx.input);

  const scopeMatch =
    lp[25] === 6n &&
    lp[6].toLowerCase() === doge &&
    lp[10].toLowerCase() === extension;

  const pp = pendingPortal.get(hash);

  launches.set(hash, {
    tx_hash: hash,
    block_number: item.log.blockNumber,
    block_hash: item.log.blockHash,
    token_version: lp[25].toString(),
    quote_token: lp[6].toLowerCase(),
    quote_amount: lp[7].toString(),
    extension_id: lp[10].toLowerCase(),
    buy_tax: lp[14].toString(),
    sell_tax: lp[15].toString(),
    frozen_scope_match: scopeMatch,
    seen_in_pending: pending.has(hash),
    full_pending_object_seen: !!pp,
    pending_received_ns: pp?.local_received_ns ?? null,
    first_canonical_log_received_ns: item.local_received_ns,
    pending_to_log_delivery_ns: pp
      ? (BigInt(item.local_received_ns) -
         BigInt(pp.local_received_ns)).toString()
      : null
  });
}

const rows = [...launches.values()];
const v6Den = rows.length;
const v6Seen = rows.filter(x => x.seen_in_pending).length;

const scopeRows = rows.filter(x => x.frozen_scope_match);
const scopeDen = scopeRows.length;
const scopeSeen = scopeRows.filter(x => x.seen_in_pending).length;

const result = {
  status:
    scopeDen > 0
      ? 'G0.4_PARTIAL_CANONICAL_BASELINE'
      : 'G0.4_PARTIAL_NO_FROZEN_SCOPE_DENOMINATOR',
  capture_duration_ns: d.total_duration_ns,
  census_start_ms: d.census_start_ms,
  census_end_ms: d.census_end_ms,
  stats: d.stats,
  canonical_v6_launch_denominator: v6Den,
  canonical_v6_seen_pending: v6Seen,
  canonical_v6_pending_coverage:
    v6Den ? v6Seen / v6Den : null,
  frozen_scope_launch_denominator: scopeDen,
  frozen_scope_seen_pending: scopeSeen,
  frozen_scope_pending_coverage:
    scopeDen ? scopeSeen / scopeDen : null,
  launches: rows,
  errors: d.errors,
  T_send: null,
  T_buy: null,
  T0: null,
  limitations: [
    'QuickNode only; no own-geth/48Club/BlockRazor comparison',
    'Canonical denominator is successful Portal-emitting newTokenV6 transactions observed during the bounded census window',
    'Subscription delivery gaps are not BUY latency or T0',
    'No value-bearing transaction sent'
  ]
};

fs.writeFileSync(
  path.join(P, 'analysis.json'),
  JSON.stringify(result, null, 2) + '\n'
);

console.log(JSON.stringify({
  status: result.status,
  pending_full: d.stats.pending_full,
  portal_pending: d.stats.portal_pending,
  portal_logs: d.stats.portal_logs,
  heads: d.stats.heads,
  canonical_v6_launches: v6Den,
  canonical_v6_seen_pending: v6Seen,
  v6_coverage: result.canonical_v6_pending_coverage,
  frozen_scope_launches: scopeDen,
  frozen_scope_seen_pending: scopeSeen,
  frozen_scope_coverage: result.frozen_scope_pending_coverage,
  errors: d.errors.length
}, null, 2));
