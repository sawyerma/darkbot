// Part of the single Rust module `protocol_v2` (included by `mod.rs`):
// Owned and borrowed-by-copy V2 record views.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordV2<'a> {
    pub kind: RecordKind,
    pub flags: u16,
    pub timestamp_ns: u64,
    pub clock_id: u32,
    pub source: [u8; SOURCE_LEN],
    pub peer: [u8; PEER_LEN],
    pub hash: [u8; HASH_LEN],
    pub aux: [u8; AUX_LEN],
    pub payload: &'a [u8],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedRecordV2 {
    pub sequence: u64,
    pub lane_id: u16,
    pub lane_epoch: u64,
    pub kind: RecordKind,
    pub flags: u16,
    pub timestamp_ns: u64,
    pub clock_id: u32,
    pub source: [u8; SOURCE_LEN],
    pub peer: [u8; PEER_LEN],
    pub hash: [u8; HASH_LEN],
    pub aux: [u8; AUX_LEN],
    pub payload: Box<[u8]>,
}
