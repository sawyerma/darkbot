'use strict';
// DBREC001 records for the DRY's JavaScript tools: a canonical encoder and a
// strict, fail-closed decoder mirroring bsc/engine/crates/record, which stays
// the authority (layout, checksum and every rule are defined there). The
// schema tables are generated from the Rust schemas
// (dbrec_schemas.generated.cjs). No JSON is read or written here.
//
// Values by kind: Bool boolean; U64/I64/U256 BigInt; Address and B256 0x-hex
// strings; Bytes 0x-hex; Text string; a Repeated field takes an array.

const crypto = require('crypto');
const SCHEMAS = require('./dbrec_schemas.generated.cjs');

const MAGIC = Buffer.from('DBREC001', 'ascii');
const FORMAT_VERSION = 1;
const MAX_RECORD_BYTES = 16 * 1024 * 1024;
const KIND = { Bool: 1, U64: 2, I64: 3, U256: 4, Address: 5, B256: 6, Bytes: 7, Text: 8 };
const FIXED = { Bool: 1, U64: 8, I64: 8, U256: 32, Address: 20, B256: 32 };

function schema(name) {
  const s = SCHEMAS[name];
  if (!s) throw new Error(`unknown DBREC schema ${name}`);
  return { name, ...s, fields: s.fields.map(([tag, field, kind, presence, maxLen]) => ({ tag, field, kind, presence, maxLen })) };
}

function hexBytes(value, len, what) {
  if (typeof value !== 'string' || !/^0x([0-9a-fA-F]{2})*$/.test(value)) throw new Error(`${what}: not 0x-hex`);
  const b = Buffer.from(value.slice(2), 'hex');
  if (len !== undefined && b.length !== len) throw new Error(`${what}: ${b.length} bytes, not ${len}`);
  return b;
}

function valueBytes(f, v) {
  const what = f.field;
  switch (f.kind) {
    case 'Bool':
      if (typeof v !== 'boolean') throw new Error(`${what}: not a boolean`);
      return Buffer.from([v ? 1 : 0]);
    case 'U64': {
      if (typeof v !== 'bigint' || v < 0n || v > 0xffffffffffffffffn) throw new Error(`${what}: not a u64 BigInt`);
      const b = Buffer.alloc(8); b.writeBigUInt64LE(v); return b;
    }
    case 'I64': {
      if (typeof v !== 'bigint' || v < -(1n << 63n) || v >= (1n << 63n)) throw new Error(`${what}: not an i64 BigInt`);
      const b = Buffer.alloc(8); b.writeBigInt64LE(v); return b;
    }
    case 'U256': {
      if (typeof v !== 'bigint' || v < 0n || v >= (1n << 256n)) throw new Error(`${what}: not a u256 BigInt`);
      return Buffer.from(v.toString(16).padStart(64, '0'), 'hex');
    }
    case 'Address': return hexBytes(v, 20, what);
    case 'B256': return hexBytes(v, 32, what);
    case 'Bytes': return hexBytes(v, undefined, what);
    case 'Text':
      if (typeof v !== 'string') throw new Error(`${what}: not a string`);
      return Buffer.from(v, 'utf8');
    default: throw new Error(`${what}: kind ${f.kind} is not supported`);
  }
}

// Encodes `values` (field name -> value) as the canonical record of `schemaName`.
function encode(schemaName, values) {
  const s = schema(schemaName);
  const known = new Set(s.fields.map(f => f.field));
  for (const name of Object.keys(values)) {
    if (!known.has(name)) throw new Error(`${s.name}: unknown field ${name}`);
  }
  const parts = [];
  let count = 0;
  for (const f of s.fields) {
    const raw = values[f.field];
    let list;
    if (f.presence === 'Repeated') {
      if (raw === undefined) list = [];
      else if (Array.isArray(raw)) list = raw;
      else throw new Error(`${s.name}: ${f.field} must be an array`);
    } else if (raw === undefined || raw === null) {
      if (f.presence === 'Required') throw new Error(`${s.name}: ${f.field} is required`);
      list = [];
    } else {
      list = [raw];
    }
    for (const v of list) {
      const body = valueBytes(f, v);
      if (FIXED[f.kind] === undefined && body.length > f.maxLen) throw new Error(`${s.name}: ${f.field} exceeds ${f.maxLen} bytes`);
      const head = Buffer.alloc(7);
      head.writeUInt16LE(f.tag, 0); head.writeUInt8(KIND[f.kind], 2); head.writeUInt32LE(body.length, 3);
      parts.push(head, body);
      count++;
    }
  }
  const header = Buffer.alloc(20);
  MAGIC.copy(header, 0);
  header.writeUInt16LE(FORMAT_VERSION, 8);
  header.writeUInt16LE(s.recordType, 10);
  header.writeUInt32LE(s.schemaVersion, 12);
  header.writeUInt32LE(count, 16);
  const body = Buffer.concat([header, ...parts]);
  const out = Buffer.concat([body, crypto.createHash('sha256').update(body).digest()]);
  if (out.length > MAX_RECORD_BYTES) throw new Error(`${s.name}: record too large`);
  return out;
}

// Decodes and checks a record of `schemaName`; fails on anything the Rust
// decoder refuses. Returns field name -> value and the file SHA-256.
function decode(schemaName, buf) {
  const s = schema(schemaName);
  if (!Buffer.isBuffer(buf) || buf.length < 52) throw new Error(`${s.name}: record truncated`);
  if (buf.length > MAX_RECORD_BYTES) throw new Error(`${s.name}: record too large`);
  if (!buf.subarray(0, 8).equals(MAGIC)) throw new Error(`${s.name}: not a DBREC001 record`);
  if (buf.readUInt16LE(8) !== FORMAT_VERSION) throw new Error(`${s.name}: unsupported format version`);
  if (buf.readUInt16LE(10) !== s.recordType || buf.readUInt32LE(12) !== s.schemaVersion) {
    throw new Error(`${s.name}: wrong record type or schema version`);
  }
  const end = buf.length - 32;
  if (!crypto.createHash('sha256').update(buf.subarray(0, end)).digest().equals(buf.subarray(end))) {
    throw new Error(`${s.name}: checksum mismatch`);
  }
  const byTag = new Map(s.fields.map(f => [f.tag, f]));
  const out = {};
  const seen = new Map();
  const count = buf.readUInt32LE(16);
  let pos = 20;
  let previous = null;
  const utf8 = new TextDecoder('utf-8', { fatal: true });
  for (let i = 0; i < count; i++) {
    if (pos + 7 > end) throw new Error(`${s.name}: record truncated`);
    const tag = buf.readUInt16LE(pos); const kind = buf.readUInt8(pos + 2); const len = buf.readUInt32LE(pos + 3);
    pos += 7;
    const f = byTag.get(tag);
    if (!f) throw new Error(`${s.name}: unknown tag ${tag}`);
    if (previous !== null && (tag < previous || (tag === previous && f.presence !== 'Repeated'))) {
      throw new Error(`${s.name}: ${f.field} out of order or repeated`);
    }
    previous = tag;
    if (KIND[f.kind] !== kind) throw new Error(`${s.name}: ${f.field} has kind ${kind}`);
    const fixed = FIXED[f.kind];
    if (fixed !== undefined ? len !== fixed : len > f.maxLen) throw new Error(`${s.name}: ${f.field} has length ${len}`);
    if (pos + len > end) throw new Error(`${s.name}: record truncated`);
    const raw = buf.subarray(pos, pos + len);
    pos += len;
    let v;
    switch (f.kind) {
      case 'Bool':
        if (raw[0] > 1) throw new Error(`${s.name}: ${f.field} is bool byte ${raw[0]}`);
        v = raw[0] === 1; break;
      case 'U64': v = raw.readBigUInt64LE(0); break;
      case 'I64': v = raw.readBigInt64LE(0); break;
      case 'U256': v = BigInt('0x' + raw.toString('hex')); break;
      case 'Address': case 'B256': case 'Bytes': v = '0x' + raw.toString('hex'); break;
      case 'Text':
        try { v = utf8.decode(raw); } catch (_) { throw new Error(`${s.name}: ${f.field} is not UTF-8`); }
        break;
      default: throw new Error(`${s.name}: kind ${f.kind} is not supported`);
    }
    seen.set(tag, (seen.get(tag) || 0) + 1);
    if (f.presence === 'Repeated') (out[f.field] ||= []).push(v);
    else out[f.field] = v;
  }
  if (pos !== end) throw new Error(`${s.name}: trailing bytes`);
  for (const f of s.fields) {
    const n = seen.get(f.tag) || 0;
    if (f.presence === 'Required' && n !== 1) throw new Error(`${s.name}: ${f.field} occurs ${n} times`);
    if (f.presence === 'Repeated' && n === 0) out[f.field] = [];
  }
  return { fields: out, sha256: crypto.createHash('sha256').update(buf).digest('hex') };
}

// Writes a record to a new file (0600); an existing file is never replaced.
function writeNew(path, bytes) {
  const fs = require('fs');
  const fd = fs.openSync(path, 'wx', 0o600);
  try {
    fs.writeSync(fd, bytes);
    fs.fsyncSync(fd);
  } finally {
    fs.closeSync(fd);
  }
}

module.exports = { encode, decode, writeNew };
