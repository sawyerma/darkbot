//! Typed, versioned binary records (`DBREC001`): the canonical format of
//! DarkBot-owned approval contracts, imported legacy inputs, qualification
//! evidence and reports. The roadmap's data-path invariant forbids JSON as an
//! internal representation: JSON exists only on external JSON-RPC wires, and
//! a human-readable view is derived from a record with [`Record::render`],
//! never the other way round.
//!
//! A record is one self-checking byte string:
//!
//! ```text
//! "DBREC001" | u16 format version | u16 record type | u32 schema version | u32 field count
//! count x    u16 tag | u8 kind | u32 length | value
//! SHA-256 over every preceding byte
//! ```
//!
//! Integers are little-endian; a `U256` is a big-endian EVM word. A nested
//! value is `u32 field count` followed by its fields, checked against the
//! nested schema its field declares. The encoding is canonical: fields in
//! ascending tag order, the values of a repeated field consecutive in
//! insertion order, fixed-width kinds at their exact width, booleans 0 or 1.
//!
//! Decoding fails closed on anything else: another magic, format version,
//! record type or schema version; a tag out of order, unknown, or repeated
//! without being declared repeated; a kind or length the schema does not
//! declare; a missing required field; invalid UTF-8; trailing bytes; a wrong
//! checksum. The binding hash of a record is the SHA-256 of its whole file.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::{self, Write as _};

pub mod schema;

pub const MAGIC: [u8; 8] = *b"DBREC001";
pub const FORMAT_VERSION: u16 = 1;
/// Largest record the decoder accepts and the encoder produces.
pub const MAX_RECORD_BYTES: usize = 16 * 1024 * 1024;
/// Bound of a `Text` or `Bytes` value unless its field sets another.
pub const DEFAULT_MAX_LEN: u32 = 4096;
const HEADER_LEN: usize = 20;
const TRAILER_LEN: usize = 32;
const MAX_DEPTH: usize = 4;

/// The kind of a field value and its wire tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    Bool = 1,
    U64 = 2,
    I64 = 3,
    /// A 256-bit unsigned integer as a big-endian 32-byte word.
    U256 = 4,
    Address = 5,
    B256 = 6,
    Bytes = 7,
    /// UTF-8 text: identifiers, paths, prose and external error messages.
    Text = 8,
    /// The fields of a nested schema.
    Nested = 9,
}

impl Kind {
    fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            1 => Self::Bool,
            2 => Self::U64,
            3 => Self::I64,
            4 => Self::U256,
            5 => Self::Address,
            6 => Self::B256,
            7 => Self::Bytes,
            8 => Self::Text,
            9 => Self::Nested,
            _ => return None,
        })
    }

    fn fixed_len(self) -> Option<usize> {
        match self {
            Self::Bool => Some(1),
            Self::U64 | Self::I64 => Some(8),
            Self::Address => Some(20),
            Self::U256 | Self::B256 => Some(32),
            Self::Bytes | Self::Text | Self::Nested => None,
        }
    }
}

/// How often a field may occur in a record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presence {
    Required,
    Optional,
    /// Zero or more values, consecutive in insertion order.
    Repeated,
}

/// One field of a schema.
#[derive(Clone, Copy, Debug)]
pub struct Field {
    pub tag: u16,
    /// Dotted name, also the line label of the derived view.
    pub name: &'static str,
    pub kind: Kind,
    pub presence: Presence,
    /// Bound in bytes of a `Text`, `Bytes` or `Nested` value.
    pub max_len: u32,
    /// The schema of a `Nested` value.
    pub nested: Option<&'static Schema>,
}

impl Field {
    pub const fn req(tag: u16, name: &'static str, kind: Kind) -> Self {
        Self::new(tag, name, kind, Presence::Required)
    }

    pub const fn opt(tag: u16, name: &'static str, kind: Kind) -> Self {
        Self::new(tag, name, kind, Presence::Optional)
    }

    pub const fn rep(tag: u16, name: &'static str, kind: Kind) -> Self {
        Self::new(tag, name, kind, Presence::Repeated)
    }

    pub const fn nested(
        tag: u16,
        name: &'static str,
        presence: Presence,
        schema: &'static Schema,
    ) -> Self {
        Self {
            tag,
            name,
            kind: Kind::Nested,
            presence,
            max_len: 1024 * 1024,
            nested: Some(schema),
        }
    }

    /// The same field with another bound for its value.
    pub const fn max(self, max_len: u32) -> Self {
        Self { max_len, ..self }
    }

    const fn new(tag: u16, name: &'static str, kind: Kind, presence: Presence) -> Self {
        Self {
            tag,
            name,
            kind,
            presence,
            max_len: DEFAULT_MAX_LEN,
            nested: None,
        }
    }
}

/// A record layout: its name, type, version and fields in ascending tag order.
#[derive(Debug)]
pub struct Schema {
    pub name: &'static str,
    pub record_type: u16,
    pub schema_version: u32,
    pub fields: &'static [Field],
}

/// Schemas are identified by name, record type and version.
impl PartialEq for Schema {
    fn eq(&self, other: &Self) -> bool {
        (self.name, self.record_type, self.schema_version)
            == (other.name, other.record_type, other.schema_version)
    }
}

impl Eq for Schema {}

impl Schema {
    pub fn field(&self, tag: u16) -> Option<&'static Field> {
        // `fields` is 'static, so the found field is too.
        let fields: &'static [Field] = self.fields;
        fields.iter().find(|f| f.tag == tag)
    }

    /// Structural check of the schema itself: non-zero tags in strictly
    /// ascending order, unique non-empty names, a nested schema exactly for
    /// nested fields (each checked in turn), bounds within the record limit.
    pub fn check(&self) -> Result<(), RecordError> {
        self.check_at(0)
    }

    fn check_at(&self, depth: usize) -> Result<(), RecordError> {
        if depth > MAX_DEPTH {
            return fail(format!("{}: schemas nested too deep", self.name));
        }
        let mut previous = 0u16;
        for (i, field) in self.fields.iter().enumerate() {
            if field.tag == 0 || field.tag <= previous {
                return fail(format!(
                    "{}: tag {} of {} is zero or out of order",
                    self.name, field.tag, field.name
                ));
            }
            previous = field.tag;
            if field.name.is_empty() || self.fields[..i].iter().any(|f| f.name == field.name) {
                return fail(format!("{}: empty or duplicate name {}", self.name, field.name));
            }
            if field.max_len as usize > MAX_RECORD_BYTES {
                return fail(format!("{}: bound of {} too large", self.name, field.name));
            }
            match (field.kind, field.nested) {
                (Kind::Nested, Some(nested)) => nested.check_at(depth + 1)?,
                (Kind::Nested, None) | (_, Some(_)) => {
                    return fail(format!(
                        "{}: {} must declare a nested schema exactly when nested",
                        self.name, field.name
                    ));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// A field value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Bool(bool),
    U64(u64),
    I64(i64),
    U256([u8; 32]),
    Address([u8; 20]),
    B256([u8; 32]),
    Bytes(Vec<u8>),
    Text(String),
    Nested(Vec<(u16, Value)>),
}

impl Value {
    pub fn kind(&self) -> Kind {
        match self {
            Self::Bool(_) => Kind::Bool,
            Self::U64(_) => Kind::U64,
            Self::I64(_) => Kind::I64,
            Self::U256(_) => Kind::U256,
            Self::Address(_) => Kind::Address,
            Self::B256(_) => Kind::B256,
            Self::Bytes(_) => Kind::Bytes,
            Self::Text(_) => Kind::Text,
            Self::Nested(_) => Kind::Nested,
        }
    }

    /// A `U256` from a `u128`.
    pub fn u128(value: u128) -> Self {
        Self::U256(u256_from_u128(value))
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<u64> for Value {
    fn from(value: u64) -> Self {
        Self::U64(value)
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Self::I64(value)
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::Text(value.to_string())
    }
}

/// Why a record could not be encoded, decoded or read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordError(pub String);

impl fmt::Display for RecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for RecordError {}

fn fail<T>(message: impl Into<String>) -> Result<T, RecordError> {
    Err(RecordError(message.into()))
}

/// A record under construction. Setting a field never fails; the schema is
/// enforced when the draft is encoded.
#[derive(Clone, Debug)]
pub struct Draft {
    schema: &'static Schema,
    fields: BTreeMap<u16, Vec<Value>>,
}

impl Draft {
    pub fn new(schema: &'static Schema) -> Self {
        Self {
            schema,
            fields: BTreeMap::new(),
        }
    }

    pub fn schema(&self) -> &'static Schema {
        self.schema
    }

    /// Sets a field, replacing any earlier value.
    pub fn set(&mut self, tag: u16, value: impl Into<Value>) -> &mut Self {
        self.fields.insert(tag, vec![value.into()]);
        self
    }

    /// Sets a field to `Some` value, or clears it for `None`.
    pub fn set_opt<V: Into<Value>>(&mut self, tag: u16, value: Option<V>) -> &mut Self {
        match value {
            Some(value) => self.set(tag, value),
            None => self.clear(tag),
        }
    }

    /// Appends a value of a repeated field.
    pub fn push(&mut self, tag: u16, value: impl Into<Value>) -> &mut Self {
        self.fields.entry(tag).or_default().push(value.into());
        self
    }

    pub fn clear(&mut self, tag: u16) -> &mut Self {
        self.fields.remove(&tag);
        self
    }

    /// This draft as the value of a nested field; checked against that
    /// field's schema when the enclosing record is encoded.
    pub fn into_nested(self) -> Value {
        Value::Nested(self.flat())
    }

    fn flat(&self) -> Vec<(u16, Value)> {
        self.fields
            .iter()
            .flat_map(|(tag, values)| values.iter().map(|v| (*tag, v.clone())))
            .collect()
    }

    /// The canonical bytes of this record.
    pub fn encode(&self) -> Result<Vec<u8>, RecordError> {
        self.schema.check()?;
        let mut body = Vec::new();
        let count = encode_fields(self.schema, &self.flat(), &mut body, 0)?;
        let mut out = Vec::with_capacity(HEADER_LEN + body.len() + TRAILER_LEN);
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&self.schema.record_type.to_le_bytes());
        out.extend_from_slice(&self.schema.schema_version.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&body);
        let checksum = Sha256::digest(&out);
        out.extend_from_slice(&checksum);
        if out.len() > MAX_RECORD_BYTES {
            return fail(format!("{} exceeds {MAX_RECORD_BYTES} bytes", self.schema.name));
        }
        Ok(out)
    }
}

fn check_presence(schema: &Schema, fields: &[(u16, Value)]) -> Result<(), RecordError> {
    for spec in schema.fields {
        let n = fields.iter().filter(|(tag, _)| *tag == spec.tag).count();
        let ok = match spec.presence {
            Presence::Required => n == 1,
            Presence::Optional => n <= 1,
            Presence::Repeated => true,
        };
        if !ok {
            return fail(format!(
                "{}: {} occurs {n} times ({:?})",
                schema.name, spec.name, spec.presence
            ));
        }
    }
    Ok(())
}

/// The field spec for `tag`, checked to follow `previous` in canonical order.
fn spec_after(
    schema: &Schema,
    tag: u16,
    previous: Option<u16>,
) -> Result<&'static Field, RecordError> {
    let Some(spec) = schema.field(tag) else {
        return fail(format!("{}: unknown tag {tag}", schema.name));
    };
    if let Some(previous) = previous
        && (tag < previous || (tag == previous && spec.presence != Presence::Repeated))
    {
        return fail(format!(
            "{}: {} (tag {tag}) out of order or repeated",
            schema.name, spec.name
        ));
    }
    Ok(spec)
}

fn encode_fields(
    schema: &Schema,
    fields: &[(u16, Value)],
    out: &mut Vec<u8>,
    depth: usize,
) -> Result<u32, RecordError> {
    if depth > MAX_DEPTH {
        return fail(format!("{}: nested too deep", schema.name));
    }
    check_presence(schema, fields)?;
    let mut previous = None;
    for (tag, value) in fields {
        let spec = spec_after(schema, *tag, previous)?;
        previous = Some(*tag);
        if value.kind() != spec.kind {
            return fail(format!(
                "{}: {} is {:?}, not {:?}",
                schema.name,
                spec.name,
                value.kind(),
                spec.kind
            ));
        }
        out.extend_from_slice(&tag.to_le_bytes());
        out.push(spec.kind as u8);
        let len_at = out.len();
        out.extend_from_slice(&[0; 4]);
        let start = out.len();
        match value {
            Value::Bool(b) => out.push(u8::from(*b)),
            Value::U64(v) => out.extend_from_slice(&v.to_le_bytes()),
            Value::I64(v) => out.extend_from_slice(&v.to_le_bytes()),
            Value::U256(w) | Value::B256(w) => out.extend_from_slice(w),
            Value::Address(a) => out.extend_from_slice(a),
            Value::Bytes(b) => out.extend_from_slice(b),
            Value::Text(t) => out.extend_from_slice(t.as_bytes()),
            Value::Nested(inner) => {
                let Some(nested) = spec.nested else {
                    return fail(format!("{}: {} has no nested schema", schema.name, spec.name));
                };
                let count_at = out.len();
                out.extend_from_slice(&[0; 4]);
                let count = encode_fields(nested, inner, out, depth + 1)?;
                out[count_at..count_at + 4].copy_from_slice(&count.to_le_bytes());
            }
        }
        let len = out.len() - start;
        if spec.kind.fixed_len().is_none() && len > spec.max_len as usize {
            return fail(format!(
                "{}: {} is {len} bytes (limit {})",
                schema.name, spec.name, spec.max_len
            ));
        }
        let len = u32::try_from(len)
            .or_else(|_| fail(format!("{}: {} too long", schema.name, spec.name)))?;
        out[len_at..len_at + 4].copy_from_slice(&len.to_le_bytes());
    }
    u32::try_from(fields.len()).or_else(|_| fail(format!("{}: too many fields", schema.name)))
}

/// The record type and schema version of a record, after checking its
/// magic and format version. Nothing else is checked.
pub fn header(bytes: &[u8]) -> Result<(u16, u32), RecordError> {
    if bytes.len() < HEADER_LEN + TRAILER_LEN {
        return fail("record truncated");
    }
    if bytes[..8] != MAGIC {
        return fail("not a DBREC001 record");
    }
    let version = u16::from_le_bytes([bytes[8], bytes[9]]);
    if version != FORMAT_VERSION {
        return fail(format!("unsupported record format version {version}"));
    }
    Ok((
        u16::from_le_bytes([bytes[10], bytes[11]]),
        u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
    ))
}

struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], RecordError> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|end| *end <= self.buf.len())
            .ok_or_else(|| RecordError("record truncated".into()))?;
        let bytes = &self.buf[self.pos..end];
        self.pos = end;
        Ok(bytes)
    }

    fn u8(&mut self) -> Result<u8, RecordError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, RecordError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, RecordError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

fn array<const N: usize>(raw: &[u8]) -> [u8; N] {
    let mut out = [0u8; N];
    out.copy_from_slice(raw);
    out
}

fn decode_fields(
    schema: &Schema,
    cursor: &mut Cursor<'_>,
    count: u32,
    depth: usize,
) -> Result<Vec<(u16, Value)>, RecordError> {
    if depth > MAX_DEPTH {
        return fail(format!("{}: nested too deep", schema.name));
    }
    let mut fields = Vec::new();
    let mut previous = None;
    for _ in 0..count {
        let tag = cursor.u16()?;
        let kind = cursor.u8()?;
        let len = cursor.u32()? as usize;
        let spec = spec_after(schema, tag, previous)?;
        previous = Some(tag);
        if Kind::from_u8(kind) != Some(spec.kind) {
            return fail(format!(
                "{}: {} has kind {kind}, not {:?}",
                schema.name, spec.name, spec.kind
            ));
        }
        let ok_len = match spec.kind.fixed_len() {
            Some(fixed) => len == fixed,
            None => len <= spec.max_len as usize,
        };
        if !ok_len {
            return fail(format!("{}: {} has length {len}", schema.name, spec.name));
        }
        let raw = cursor.take(len)?;
        let value = match spec.kind {
            Kind::Bool => match raw[0] {
                0 => Value::Bool(false),
                1 => Value::Bool(true),
                b => return fail(format!("{}: {} is bool byte {b}", schema.name, spec.name)),
            },
            Kind::U64 => Value::U64(u64::from_le_bytes(array(raw))),
            Kind::I64 => Value::I64(i64::from_le_bytes(array(raw))),
            Kind::U256 => Value::U256(array(raw)),
            Kind::Address => Value::Address(array(raw)),
            Kind::B256 => Value::B256(array(raw)),
            Kind::Bytes => Value::Bytes(raw.to_vec()),
            Kind::Text => match std::str::from_utf8(raw) {
                Ok(text) => Value::Text(text.to_string()),
                Err(_) => return fail(format!("{}: {} is not UTF-8", schema.name, spec.name)),
            },
            Kind::Nested => {
                let Some(nested) = spec.nested else {
                    return fail(format!("{}: {} has no nested schema", schema.name, spec.name));
                };
                let mut inner = Cursor { buf: raw, pos: 0 };
                let count = inner.u32()?;
                let fields = decode_fields(nested, &mut inner, count, depth + 1)?;
                if inner.pos != raw.len() {
                    return fail(format!("{}: {} has trailing bytes", schema.name, spec.name));
                }
                Value::Nested(fields)
            }
        };
        fields.push((tag, value));
    }
    check_presence(schema, &fields)?;
    Ok(fields)
}

/// Decodes and checks a record of `schema`.
pub fn decode(schema: &'static Schema, bytes: &[u8]) -> Result<Record, RecordError> {
    if bytes.len() > MAX_RECORD_BYTES {
        return fail(format!("record exceeds {MAX_RECORD_BYTES} bytes"));
    }
    let (record_type, schema_version) = header(bytes)?;
    if (record_type, schema_version) != (schema.record_type, schema.schema_version) {
        return fail(format!(
            "record type {record_type} version {schema_version} is not {} (type {} version {})",
            schema.name, schema.record_type, schema.schema_version
        ));
    }
    let body_end = bytes.len() - TRAILER_LEN;
    if sha256(&bytes[..body_end]) != bytes[body_end..] {
        return fail(format!("{}: checksum mismatch", schema.name));
    }
    let count = u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let mut cursor = Cursor {
        buf: &bytes[..body_end],
        pos: HEADER_LEN,
    };
    let fields = decode_fields(schema, &mut cursor, count, 0)?;
    if cursor.pos != body_end {
        return fail(format!("{}: trailing bytes", schema.name));
    }
    Ok(Record {
        schema,
        fields,
        file_sha256: Some(sha256(bytes)),
    })
}

/// A decoded record, or the value of a nested field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    schema: &'static Schema,
    fields: Vec<(u16, Value)>,
    file_sha256: Option<[u8; 32]>,
}

impl Record {
    pub fn schema(&self) -> &'static Schema {
        self.schema
    }

    /// SHA-256 of the whole record file it was decoded from (its binding
    /// hash); `None` for a nested record.
    pub fn file_sha256(&self) -> Option<[u8; 32]> {
        self.file_sha256
    }

    pub fn fields(&self) -> &[(u16, Value)] {
        &self.fields
    }

    pub fn get(&self, tag: u16) -> Option<&Value> {
        self.fields.iter().find(|(t, _)| *t == tag).map(|(_, v)| v)
    }

    pub fn all(&self, tag: u16) -> impl Iterator<Item = &Value> {
        self.fields
            .iter()
            .filter(move |(t, _)| *t == tag)
            .map(|(_, v)| v)
    }

    fn name(&self, tag: u16) -> String {
        self.schema
            .field(tag)
            .map_or_else(|| format!("tag {tag}"), |f| f.name.to_string())
    }

    fn maybe<'r, T>(
        &'r self,
        tag: u16,
        want: Kind,
        pick: impl Fn(&'r Value) -> Option<T>,
    ) -> Result<Option<T>, RecordError> {
        match self.get(tag) {
            None => Ok(None),
            Some(value) => pick(value).map(Some).ok_or_else(|| {
                RecordError(format!(
                    "{}: {} is {:?}, not {want:?}",
                    self.schema.name,
                    self.name(tag),
                    value.kind()
                ))
            }),
        }
    }

    fn one<'r, T>(
        &'r self,
        tag: u16,
        want: Kind,
        pick: impl Fn(&'r Value) -> Option<T>,
    ) -> Result<T, RecordError> {
        self.maybe(tag, want, pick)?.ok_or_else(|| {
            RecordError(format!("{}: {} is missing", self.schema.name, self.name(tag)))
        })
    }

    pub fn opt_bool(&self, tag: u16) -> Result<Option<bool>, RecordError> {
        self.maybe(tag, Kind::Bool, |v| match v {
            Value::Bool(b) => Some(*b),
            _ => None,
        })
    }

    pub fn opt_u64(&self, tag: u16) -> Result<Option<u64>, RecordError> {
        self.maybe(tag, Kind::U64, |v| match v {
            Value::U64(n) => Some(*n),
            _ => None,
        })
    }

    pub fn opt_i64(&self, tag: u16) -> Result<Option<i64>, RecordError> {
        self.maybe(tag, Kind::I64, |v| match v {
            Value::I64(n) => Some(*n),
            _ => None,
        })
    }

    pub fn opt_u256(&self, tag: u16) -> Result<Option<[u8; 32]>, RecordError> {
        self.maybe(tag, Kind::U256, |v| match v {
            Value::U256(w) => Some(*w),
            _ => None,
        })
    }

    pub fn opt_address(&self, tag: u16) -> Result<Option<[u8; 20]>, RecordError> {
        self.maybe(tag, Kind::Address, |v| match v {
            Value::Address(a) => Some(*a),
            _ => None,
        })
    }

    pub fn opt_b256(&self, tag: u16) -> Result<Option<[u8; 32]>, RecordError> {
        self.maybe(tag, Kind::B256, |v| match v {
            Value::B256(w) => Some(*w),
            _ => None,
        })
    }

    pub fn opt_bytes(&self, tag: u16) -> Result<Option<&[u8]>, RecordError> {
        self.maybe(tag, Kind::Bytes, |v| match v {
            Value::Bytes(b) => Some(b.as_slice()),
            _ => None,
        })
    }

    pub fn opt_text(&self, tag: u16) -> Result<Option<&str>, RecordError> {
        self.maybe(tag, Kind::Text, |v| match v {
            Value::Text(t) => Some(t.as_str()),
            _ => None,
        })
    }

    pub fn opt_nested(&self, tag: u16) -> Result<Option<Record>, RecordError> {
        let Some(spec) = self.schema.field(tag) else {
            return fail(format!("{}: unknown tag {tag}", self.schema.name));
        };
        self.maybe(tag, Kind::Nested, |v| match (v, spec.nested) {
            (Value::Nested(fields), Some(schema)) => Some(Record {
                schema,
                fields: fields.clone(),
                file_sha256: None,
            }),
            _ => None,
        })
    }

    pub fn bool(&self, tag: u16) -> Result<bool, RecordError> {
        self.one(tag, Kind::Bool, |v| match v {
            Value::Bool(b) => Some(*b),
            _ => None,
        })
    }

    pub fn u64(&self, tag: u16) -> Result<u64, RecordError> {
        self.opt_u64(tag)?.ok_or_else(|| self.missing(tag))
    }

    pub fn i64(&self, tag: u16) -> Result<i64, RecordError> {
        self.opt_i64(tag)?.ok_or_else(|| self.missing(tag))
    }

    pub fn u256(&self, tag: u16) -> Result<[u8; 32], RecordError> {
        self.opt_u256(tag)?.ok_or_else(|| self.missing(tag))
    }

    pub fn address(&self, tag: u16) -> Result<[u8; 20], RecordError> {
        self.opt_address(tag)?.ok_or_else(|| self.missing(tag))
    }

    pub fn b256(&self, tag: u16) -> Result<[u8; 32], RecordError> {
        self.opt_b256(tag)?.ok_or_else(|| self.missing(tag))
    }

    pub fn bytes(&self, tag: u16) -> Result<&[u8], RecordError> {
        self.opt_bytes(tag)?.ok_or_else(|| self.missing(tag))
    }

    pub fn text(&self, tag: u16) -> Result<&str, RecordError> {
        self.opt_text(tag)?.ok_or_else(|| self.missing(tag))
    }

    pub fn nested(&self, tag: u16) -> Result<Record, RecordError> {
        self.opt_nested(tag)?.ok_or_else(|| self.missing(tag))
    }

    /// The values of a repeated text field.
    pub fn texts(&self, tag: u16) -> Result<Vec<&str>, RecordError> {
        self.all(tag)
            .map(|v| match v {
                Value::Text(t) => Ok(t.as_str()),
                other => fail(format!(
                    "{}: {} is {:?}, not Text",
                    self.schema.name,
                    self.name(tag),
                    other.kind()
                )),
            })
            .collect()
    }

    /// The records of a repeated nested field.
    pub fn nested_all(&self, tag: u16) -> Result<Vec<Record>, RecordError> {
        let Some(schema) = self.schema.field(tag).and_then(|f| f.nested) else {
            return fail(format!("{}: tag {tag} is not nested", self.schema.name));
        };
        self.all(tag)
            .map(|v| match v {
                Value::Nested(fields) => Ok(Record {
                    schema,
                    fields: fields.clone(),
                    file_sha256: None,
                }),
                other => fail(format!(
                    "{}: {} is {:?}, not Nested",
                    self.schema.name,
                    self.name(tag),
                    other.kind()
                )),
            })
            .collect()
    }

    fn missing(&self, tag: u16) -> RecordError {
        RecordError(format!("{}: {} is missing", self.schema.name, self.name(tag)))
    }

    /// The derived human-readable view: one `name = value` line per field.
    /// It is never a source of truth and never parsed back.
    pub fn render(&self) -> String {
        let mut out = format!(
            "# {} (record type {}, schema version {})\n",
            self.schema.name, self.schema.record_type, self.schema.schema_version
        );
        if let Some(sha) = self.file_sha256 {
            let _ = writeln!(out, "file_sha256 = {}", hex(&sha));
        }
        render_fields(self.schema, &self.fields, "", &mut out);
        out
    }
}

fn render_fields(schema: &Schema, fields: &[(u16, Value)], prefix: &str, out: &mut String) {
    let mut seen: BTreeMap<u16, usize> = BTreeMap::new();
    for (tag, value) in fields {
        let Some(spec) = schema.field(*tag) else {
            continue;
        };
        let name = if spec.presence == Presence::Repeated {
            let index = seen.entry(*tag).or_default();
            let name = format!("{prefix}{}[{index}]", spec.name);
            *index += 1;
            name
        } else {
            format!("{prefix}{}", spec.name)
        };
        match (value, spec.nested) {
            (Value::Nested(inner), Some(nested)) => {
                render_fields(nested, inner, &format!("{name}."), out);
            }
            _ => {
                let _ = writeln!(out, "{name} = {}", display(value));
            }
        }
    }
}

fn display(value: &Value) -> String {
    match value {
        Value::Bool(b) => b.to_string(),
        Value::U64(n) => n.to_string(),
        Value::I64(n) => n.to_string(),
        Value::U256(w) => u256_to_decimal(w),
        Value::Address(a) => format!("0x{}", hex(a)),
        Value::B256(w) => format!("0x{}", hex(w)),
        Value::Bytes(b) => format!("0x{}", hex(b)),
        Value::Text(t) => format!("{t:?}"),
        Value::Nested(fields) => format!("<{} nested fields>", fields.len()),
    }
}

pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// Lowercase hex without a prefix.
pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

pub fn u256_from_u128(value: u128) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[16..].copy_from_slice(&value.to_be_bytes());
    out
}

/// The decimal digits of a big-endian 256-bit word.
pub fn u256_to_decimal(word: &[u8; 32]) -> String {
    let mut limbs = [0u64; 4];
    for (i, limb) in limbs.iter_mut().enumerate() {
        *limb = u64::from_be_bytes(array(&word[i * 8..i * 8 + 8]));
    }
    if limbs == [0; 4] {
        return "0".into();
    }
    let mut digits = Vec::new();
    while limbs != [0; 4] {
        let mut rem = 0u128;
        for limb in limbs.iter_mut() {
            let current = (rem << 64) | u128::from(*limb);
            *limb = (current / 10) as u64;
            rem = current % 10;
        }
        digits.push(b'0' + rem as u8);
    }
    digits.iter().rev().map(|d| char::from(*d)).collect()
}

/// A big-endian 256-bit word from canonical decimal digits: no sign, no
/// leading zero (except "0" itself), no overflow.
pub fn u256_from_decimal(text: &str) -> Option<[u8; 32]> {
    if text.is_empty()
        || !text.bytes().all(|b| b.is_ascii_digit())
        || (text.len() > 1 && text.starts_with('0'))
    {
        return None;
    }
    let mut limbs = [0u64; 4];
    for digit in text.bytes() {
        let mut carry = u128::from(digit - b'0');
        for limb in limbs.iter_mut().rev() {
            let current = u128::from(*limb) * 10 + carry;
            *limb = current as u64;
            carry = current >> 64;
        }
        if carry != 0 {
            return None;
        }
    }
    let mut out = [0u8; 32];
    for (i, limb) in limbs.iter().enumerate() {
        out[i * 8..i * 8 + 8].copy_from_slice(&limb.to_be_bytes());
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const INNER: Schema = Schema {
        name: "test.inner",
        record_type: 0,
        schema_version: 1,
        fields: &[
            Field::req(1, "label", Kind::Text).max(8),
            Field::opt(2, "count", Kind::U64),
        ],
    };

    const OUTER: Schema = Schema {
        name: "test.outer",
        record_type: 60_000,
        schema_version: 3,
        fields: &[
            Field::req(1, "flag", Kind::Bool),
            Field::req(2, "number", Kind::U64),
            Field::opt(3, "signed", Kind::I64),
            Field::req(4, "amount", Kind::U256),
            Field::opt(5, "who", Kind::Address),
            Field::opt(6, "hash", Kind::B256),
            Field::opt(7, "blob", Kind::Bytes).max(4),
            Field::rep(8, "notes", Kind::Text),
            Field::nested(9, "inner", Presence::Repeated, &INNER),
        ],
    };

    fn sample() -> Draft {
        let mut inner = Draft::new(&INNER);
        inner.set(1, "a").set(2, 7u64);
        let mut d = Draft::new(&OUTER);
        d.set(1, true)
            .set(2, 42u64)
            .set(3, -5i64)
            .set(4, Value::u128(1_000_000_000_000))
            .set(5, Value::Address([0x11; 20]))
            .set(6, Value::B256([0x22; 32]))
            .set(7, Value::Bytes(vec![1, 2]))
            .push(8, "x")
            .push(8, "y")
            .push(9, inner.into_nested());
        d
    }

    /// Recomputes the checksum of hand-edited bytes, so the decoder reaches
    /// the structural check under test.
    fn reseal(mut bytes: Vec<u8>) -> Vec<u8> {
        let end = bytes.len() - TRAILER_LEN;
        let digest = sha256(&bytes[..end]);
        bytes[end..].copy_from_slice(&digest);
        bytes
    }

    fn expect_err(bytes: &[u8], needle: &str) {
        let e = decode(&OUTER, bytes).unwrap_err().0;
        assert!(e.contains(needle), "{e}");
    }

    #[test]
    fn every_schema_is_well_formed() {
        OUTER.check().unwrap();
        for schema in schema::all() {
            schema.check().unwrap();
        }
        let mut seen = std::collections::BTreeSet::new();
        for schema in schema::all() {
            assert!(
                seen.insert((schema.record_type, schema.schema_version)),
                "{}",
                schema.name
            );
        }
    }

    #[test]
    fn round_trip_is_canonical_and_golden() {
        let bytes = sample().encode().unwrap();
        // Golden: header, the first field (flag) and the total length.
        assert_eq!(&bytes[..8], b"DBREC001");
        assert_eq!(&bytes[8..20], &[1, 0, 0x60, 0xea, 3, 0, 0, 0, 10, 0, 0, 0]);
        assert_eq!(&bytes[20..28], &[1, 0, 1, 1, 0, 0, 0, 1]);
        assert_eq!(bytes.len(), 254);
        let record = decode(&OUTER, &bytes).unwrap();
        assert!(record.bool(1).unwrap());
        assert_eq!(record.u64(2).unwrap(), 42);
        assert_eq!(record.i64(3).unwrap(), -5);
        assert_eq!(u256_to_decimal(&record.u256(4).unwrap()), "1000000000000");
        assert_eq!(record.texts(8).unwrap(), ["x", "y"]);
        let inner = record.nested_all(9).unwrap();
        assert_eq!(inner[0].text(1).unwrap(), "a");
        assert_eq!(record.file_sha256(), Some(sha256(&bytes)));
        // Insertion order of the draft does not change the bytes.
        let mut reordered = Draft::new(&OUTER);
        reordered
            .push(9, {
                let mut i = Draft::new(&INNER);
                i.set(2, 7u64).set(1, "a");
                i.into_nested()
            })
            .push(8, "x")
            .push(8, "y")
            .set(7, Value::Bytes(vec![1, 2]))
            .set(6, Value::B256([0x22; 32]))
            .set(5, Value::Address([0x11; 20]))
            .set(4, Value::u128(1_000_000_000_000))
            .set(3, -5i64)
            .set(2, 42u64)
            .set(1, true);
        assert_eq!(reordered.encode().unwrap(), bytes);
        let view = record.render();
        assert!(view.contains("amount = 1000000000000"), "{view}");
        assert!(view.contains("notes[1] = \"y\""), "{view}");
        assert!(view.contains("inner[0].count = 7"), "{view}");
        assert!(view.contains(&format!("who = 0x{}", "11".repeat(20))), "{view}");
    }

    #[test]
    fn encoding_enforces_the_schema() {
        let mut d = sample();
        d.clear(2);
        assert!(d.encode().unwrap_err().0.contains("number occurs 0 times"));
        let mut d = sample();
        d.set(2, true);
        assert!(d.encode().unwrap_err().0.contains("not U64"));
        let mut d = sample();
        d.set(7, Value::Bytes(vec![0; 5]));
        assert!(d.encode().unwrap_err().0.contains("limit 4"));
        let mut d = sample();
        d.set(99, 1u64);
        assert!(d.encode().unwrap_err().0.contains("unknown tag 99"));
        let mut d = sample();
        d.push(1, false);
        assert!(d.encode().unwrap_err().0.contains("flag occurs 2 times"));
        let mut bad_inner = Draft::new(&INNER);
        bad_inner.set(1, "too long label");
        let mut d = sample();
        d.push(9, bad_inner.into_nested());
        assert!(d.encode().unwrap_err().0.contains("label"));
    }

    #[test]
    fn decoding_fails_closed() {
        let good = sample().encode().unwrap();
        assert!(decode(&OUTER, &good).is_ok());
        expect_err(&good[..good.len() - 1], "checksum");
        expect_err(&good[..60], "checksum");
        expect_err(&good[..10], "truncated");
        let mut flipped = good.clone();
        flipped[25] ^= 1;
        expect_err(&flipped, "checksum");
        let mut magic = good.clone();
        magic[0] = b'X';
        expect_err(&magic, "not a DBREC001");
        let mut version = good.clone();
        version[8] = 2;
        expect_err(&reseal(version), "format version 2");
        let mut kind = good.clone();
        kind[10] = 1;
        expect_err(&reseal(kind), "is not test.outer");
        let mut schema_version = good.clone();
        schema_version[12] = 4;
        expect_err(&reseal(schema_version), "is not test.outer");
        // The first field (flag, tag 1) starts at byte 20: tag, kind, length, value.
        let mut bool_byte = good.clone();
        bool_byte[27] = 2;
        expect_err(&reseal(bool_byte), "bool byte 2");
        let mut wrong_kind = good.clone();
        wrong_kind[22] = 2;
        expect_err(&reseal(wrong_kind), "has kind 2");
        let mut wrong_len = good.clone();
        wrong_len[23] = 2;
        expect_err(&reseal(wrong_len), "has length 2");
        let mut unknown = good.clone();
        unknown[20] = 77;
        expect_err(&reseal(unknown), "unknown tag 77");
        let mut count = good.clone();
        count[16] = 9;
        expect_err(&reseal(count), "trailing bytes");
        let mut more = good.clone();
        more[16] = 11;
        expect_err(&reseal(more), "truncated");
        // Out of order and a repeated non-repeated field, built by hand.
        let flag = |tag: u16| {
            let mut f = tag.to_le_bytes().to_vec();
            f.extend_from_slice(&[1, 1, 0, 0, 0, 1]);
            f
        };
        let number = {
            let mut f = 2u16.to_le_bytes().to_vec();
            f.extend_from_slice(&[2, 8, 0, 0, 0]);
            f.extend_from_slice(&1u64.to_le_bytes());
            f
        };
        let amount = {
            let mut f = 4u16.to_le_bytes().to_vec();
            f.extend_from_slice(&[4, 32, 0, 0, 0]);
            f.extend_from_slice(&[0; 32]);
            f
        };
        let raw = |fields: &[&Vec<u8>]| {
            let mut out = good[..16].to_vec();
            out.extend_from_slice(&(fields.len() as u32).to_le_bytes());
            for f in fields {
                out.extend_from_slice(f);
            }
            out.extend_from_slice(&[0; 32]);
            reseal(out)
        };
        assert!(decode(&OUTER, &raw(&[&flag(1), &number, &amount])).is_ok());
        expect_err(&raw(&[&number, &flag(1), &amount]), "out of order");
        expect_err(&raw(&[&flag(1), &flag(1), &number, &amount]), "out of order");
        expect_err(&raw(&[&flag(1), &amount]), "number occurs 0 times");
    }

    #[test]
    fn text_must_be_utf8_and_required_fields_present() {
        let mut d = Draft::new(&OUTER);
        d.set(1, false)
            .set(2, 1u64)
            .set(4, Value::u128(0))
            .push(8, "ab");
        let good = d.encode().unwrap();
        let record = decode(&OUTER, &good).unwrap();
        assert_eq!(record.opt_u64(3).unwrap(), None);
        assert!(record.i64(3).unwrap_err().0.contains("signed is missing"));
        assert!(record.u64(1).unwrap_err().0.contains("not U64"));
        let at = good.windows(2).position(|w| w == b"ab").unwrap();
        let mut invalid = good.clone();
        invalid[at] = 0xff;
        expect_err(&reseal(invalid), "not UTF-8");
        // Remove the required `number` (tag 2, 15 bytes) and fix the count.
        let start = HEADER_LEN + 8;
        let mut missing = good[..start].to_vec();
        missing.extend_from_slice(&good[start + 15..]);
        missing[16] -= 1;
        expect_err(&reseal(missing), "number occurs 0 times");
    }

    #[test]
    fn decimal_conversion_is_exact_and_canonical() {
        for text in [
            "0",
            "1",
            "176693134169499757962",
            "115792089237316195423570985008687907853269984665640564039457584007913129639935",
        ] {
            assert_eq!(u256_to_decimal(&u256_from_decimal(text).unwrap()), text);
        }
        for bad in [
            "",
            "01",
            "-1",
            "1e3",
            " 1",
            "115792089237316195423570985008687907853269984665640564039457584007913129639936",
        ] {
            assert!(u256_from_decimal(bad).is_none(), "{bad}");
        }
        assert_eq!(u256_from_u128(u128::MAX)[..16], [0; 16]);
    }
}
