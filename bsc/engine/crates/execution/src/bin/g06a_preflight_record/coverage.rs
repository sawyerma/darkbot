//! R0 coverage without `eth_getLogs` (the free endpoints refuse or meter
//! it): every header of a block range is hash-verified and linked by its
//! parent hash, its `logsBloom` decides whether the block can hold the
//! Portal's `Upgraded` event, and every block that can is read in full and
//! its receipts verified against the header's `receiptsRoot`. Nothing here
//! trusts the endpoint beyond what the hashes bind: a header whose hash is
//! not keccak256(rlp(header)), a broken parent link or receipts that do not
//! rebuild `receiptsRoot` stop the scan.
//!
//! JSON exists only on the RPC wire (`wire`); its values are decoded into
//! the typed `Header` and `Receipt` there and never leave it.

use super::*;

/// Headers per JSON-RPC batch (the public BSC dataseed allows 100).
const HEADER_BATCH: u64 = 100;
const HEADER_WORKERS: u64 = 4;
const RECEIPT_WORKERS: usize = 6;

/// The header fields the coverage needs, from a hash-verified header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Header {
    pub(crate) number: u64,
    pub(crate) hash: [u8; 32],
    pub(crate) parent_hash: [u8; 32],
    pub(crate) logs_bloom: [u8; 256],
    pub(crate) receipts_root: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Log {
    pub(crate) address: [u8; 20],
    pub(crate) topics: Vec<[u8; 32]>,
    pub(crate) data: Vec<u8>,
    pub(crate) log_index: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Receipt {
    pub(crate) tx_type: u8,
    pub(crate) status: u64,
    pub(crate) cumulative_gas_used: u64,
    pub(crate) logs_bloom: [u8; 256],
    pub(crate) logs: Vec<Log>,
    pub(crate) transaction_hash: [u8; 32],
    pub(crate) transaction_index: u64,
    pub(crate) block_hash: [u8; 32],
}

/// An `Upgraded` event of the proxy found in verified receipts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UpgradeSeen {
    pub(crate) block: u64,
    pub(crate) block_hash: [u8; 32],
    pub(crate) transaction_hash: [u8; 32],
    pub(crate) log_index: u64,
    pub(crate) implementation: [u8; 20],
    pub(crate) receipt_status: u64,
}

/// A verified contiguous range: `from..=to`, linked from `first_parent_hash`
/// to `last_hash`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Coverage {
    pub(crate) from: u64,
    pub(crate) to: u64,
    pub(crate) first_parent_hash: [u8; 32],
    pub(crate) last_hash: [u8; 32],
    pub(crate) headers: u64,
    pub(crate) candidates: u64,
    pub(crate) upgrades: Vec<UpgradeSeen>,
}

impl Coverage {
    /// `self` followed by `next`; their hash chains must link.
    pub(crate) fn then(mut self, next: Coverage) -> Coverage {
        assert_eq!(next.from, self.to + 1, "coverage segments are not contiguous");
        assert_eq!(
            next.first_parent_hash, self.last_hash,
            "coverage segments do not link by parent hash"
        );
        self.to = next.to;
        self.last_hash = next.last_hash;
        self.headers += next.headers;
        self.candidates += next.candidates;
        self.upgrades.extend(next.upgrades);
        self
    }
}

// ---- RLP, the Merkle-Patricia root and the bloom (consensus encodings) ----

fn rlp_string(bytes: &[u8], out: &mut Vec<u8>) {
    if bytes.len() == 1 && bytes[0] < 0x80 {
        out.push(bytes[0]);
    } else {
        rlp_length(bytes.len(), 0x80, out);
        out.extend_from_slice(bytes);
    }
}

fn rlp_length(len: usize, offset: u8, out: &mut Vec<u8>) {
    if len < 56 {
        out.push(offset + len as u8);
    } else {
        let be = len.to_be_bytes();
        let skip = be.iter().take_while(|b| **b == 0).count();
        out.push(offset + 55 + (be.len() - skip) as u8);
        out.extend_from_slice(&be[skip..]);
    }
}

fn rlp_list(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 9);
    rlp_length(payload.len(), 0xc0, &mut out);
    out.extend_from_slice(payload);
    out
}

/// A scalar: big-endian without leading zeros (zero is the empty string).
fn rlp_u64(value: u64, out: &mut Vec<u8>) {
    let be = value.to_be_bytes();
    let skip = be.iter().take_while(|b| **b == 0).count();
    rlp_string(&be[skip..], out);
}

fn hex_prefix(nibbles: &[u8], leaf: bool) -> Vec<u8> {
    let flag = if leaf { 2 } else { 0 } + (nibbles.len() % 2) as u8;
    let mut out = Vec::with_capacity(nibbles.len() / 2 + 1);
    let rest = if nibbles.len() % 2 == 1 {
        out.push((flag << 4) | nibbles[0]);
        &nibbles[1..]
    } else {
        out.push(flag << 4);
        nibbles
    };
    out.extend(rest.chunks(2).map(|p| (p[0] << 4) | p[1]));
    out
}

/// The RLP of the trie node over `items` (remaining key nibbles, value),
/// sorted by key and distinct.
fn trie_node(items: &[(&[u8], &[u8])]) -> Vec<u8> {
    let mut payload = Vec::new();
    if let [(key, value)] = items {
        rlp_string(&hex_prefix(key, true), &mut payload);
        rlp_string(value, &mut payload);
        return rlp_list(&payload);
    }
    let (first, last) = (items[0].0, items[items.len() - 1].0);
    let common = first.iter().zip(last).take_while(|(a, b)| a == b).count();
    if common > 0 {
        rlp_string(&hex_prefix(&first[..common], false), &mut payload);
        let rest: Vec<(&[u8], &[u8])> = items.iter().map(|(k, v)| (&k[common..], *v)).collect();
        trie_child(&trie_node(&rest), &mut payload);
        return rlp_list(&payload);
    }
    for nibble in 0..16u8 {
        let group: Vec<(&[u8], &[u8])> = items
            .iter()
            .filter(|(k, _)| k.first() == Some(&nibble))
            .map(|(k, v)| (&k[1..], *v))
            .collect();
        if group.is_empty() {
            payload.push(0x80);
        } else {
            trie_child(&trie_node(&group), &mut payload);
        }
    }
    match items.iter().find(|(k, _)| k.is_empty()) {
        Some((_, value)) => rlp_string(value, &mut payload),
        None => payload.push(0x80),
    }
    rlp_list(&payload)
}

/// A child reference: the node itself when shorter than 32 bytes, else its hash.
fn trie_child(node: &[u8], out: &mut Vec<u8>) {
    if node.len() < 32 {
        out.extend_from_slice(node);
    } else {
        rlp_string(keccak256(node).as_slice(), out);
    }
}

/// The root of the trie keyed by rlp(index) (transactions, receipts).
pub(crate) fn ordered_trie_root(values: &[Vec<u8>]) -> [u8; 32] {
    if values.is_empty() {
        return keccak256([0x80]).0;
    }
    let keys: Vec<Vec<u8>> = (0..values.len() as u64)
        .map(|i| {
            let mut key = Vec::new();
            rlp_u64(i, &mut key);
            key.iter().flat_map(|b| [b >> 4, b & 0x0f]).collect()
        })
        .collect();
    let mut items: Vec<(&[u8], &[u8])> = keys
        .iter()
        .zip(values)
        .map(|(k, v)| (k.as_slice(), v.as_slice()))
        .collect();
    items.sort();
    keccak256(trie_node(&items)).0
}

/// The consensus (EIP-2718) encoding of a receipt, as `receiptsRoot` hashes it.
pub(crate) fn encode_receipt(r: &Receipt) -> Vec<u8> {
    let mut payload = Vec::new();
    rlp_u64(r.status, &mut payload);
    rlp_u64(r.cumulative_gas_used, &mut payload);
    rlp_string(&r.logs_bloom, &mut payload);
    let mut logs = Vec::new();
    for log in &r.logs {
        let mut entry = Vec::new();
        rlp_string(&log.address, &mut entry);
        let mut topics = Vec::new();
        for t in &log.topics {
            rlp_string(t, &mut topics);
        }
        entry.extend(rlp_list(&topics));
        rlp_string(&log.data, &mut entry);
        logs.extend(rlp_list(&entry));
    }
    payload.extend(rlp_list(&logs));
    let body = rlp_list(&payload);
    if r.tx_type == 0 {
        body
    } else {
        let mut typed = vec![r.tx_type];
        typed.extend(body);
        typed
    }
}

fn bloom_bits(item: &[u8]) -> [(usize, u8); 3] {
    let h = keccak256(item);
    [0, 2, 4].map(|i| {
        let v = ((usize::from(h[i]) << 8) | usize::from(h[i + 1])) & 2047;
        (255 - v / 8, 1u8 << (v % 8))
    })
}

pub(crate) fn bloom_add(bloom: &mut [u8; 256], item: &[u8]) {
    for (byte, bit) in bloom_bits(item) {
        bloom[byte] |= bit;
    }
}

pub(crate) fn bloom_may_contain(bloom: &[u8; 256], item: &[u8]) -> bool {
    bloom_bits(item).iter().all(|(byte, bit)| bloom[*byte] & bit != 0)
}

/// Can this block hold an `Upgraded` event of the proxy? (No false negatives.)
pub(crate) fn candidate(header: &Header, proxy: &[u8; 20], topic: &[u8; 32]) -> bool {
    bloom_may_contain(&header.logs_bloom, proxy) && bloom_may_contain(&header.logs_bloom, topic)
}

/// Verifies the receipts of `header`'s block and returns its upgrades.
pub(crate) fn verified_upgrades(
    header: &Header,
    receipts: &[Receipt],
    proxy: &[u8; 20],
    topic: &[u8; 32],
) -> Result<Vec<UpgradeSeen>, String> {
    let mut bloom = [0u8; 256];
    for (i, r) in receipts.iter().enumerate() {
        if r.block_hash != header.hash || r.transaction_index != i as u64 {
            return Err(format!(
                "block {}: receipt {i} is not the block's receipt {i}",
                header.number
            ));
        }
        for log in &r.logs {
            bloom_add(&mut bloom, &log.address);
            for t in &log.topics {
                bloom_add(&mut bloom, t);
            }
        }
    }
    let encoded: Vec<Vec<u8>> = receipts.iter().map(encode_receipt).collect();
    if ordered_trie_root(&encoded) != header.receipts_root {
        return Err(format!(
            "block {}: receipts do not rebuild receiptsRoot",
            header.number
        ));
    }
    if bloom != header.logs_bloom {
        return Err(format!("block {}: logs do not rebuild logsBloom", header.number));
    }
    let mut found = Vec::new();
    for r in receipts {
        for log in &r.logs {
            if &log.address == proxy && log.topics.first() == Some(topic) {
                let implementation = match log.topics.get(1) {
                    Some(t) if log.topics.len() == 2 && t[..12] == [0; 12] => {
                        t[12..].try_into().unwrap()
                    }
                    _ => return Err(format!("block {}: malformed Upgraded log", header.number)),
                };
                found.push(UpgradeSeen {
                    block: header.number,
                    block_hash: header.hash,
                    transaction_hash: r.transaction_hash,
                    log_index: log.log_index,
                    implementation,
                    receipt_status: r.status,
                });
            }
        }
    }
    Ok(found)
}

/// Checks a run of headers: consecutive numbers from `from`, each linked to
/// the one before.
fn linked(headers: &[Header], from: u64) -> Result<(), String> {
    for (i, h) in headers.iter().enumerate() {
        if h.number != from + i as u64 {
            return Err(format!("header {} where {} was expected", h.number, from + i as u64));
        }
        if i > 0 && h.parent_hash != headers[i - 1].hash {
            return Err(format!("header {} does not link to its parent", h.number));
        }
    }
    Ok(())
}

/// Complete coverage of `from..=to` on `url`: hash-verified, linked headers,
/// and verified receipts for every bloom candidate.
pub(crate) fn cover(url: &str, from: u64, to: u64) -> Coverage {
    assert!(from <= to, "empty coverage range {from}..={to}");
    let proxy = Address::from_str(PORTAL).unwrap().into_array();
    let topic = word(UPGRADED_TOPIC);
    let batches = (to - from) / HEADER_BATCH + 1;
    let next = AtomicU64::new(0);
    // Per batch: (first parent hash, last hash, candidate headers).
    let done: Mutex<Vec<(u64, [u8; 32], [u8; 32], Vec<Header>)>> = Mutex::new(Vec::new());
    thread::scope(|scope| {
        for _ in 0..HEADER_WORKERS.min(batches) {
            scope.spawn(|| {
                loop {
                    let n = next.fetch_add(1, Ordering::SeqCst);
                    if n >= batches {
                        break;
                    }
                    let start = from + n * HEADER_BATCH;
                    let end = (start + HEADER_BATCH - 1).min(to);
                    let headers = with_retries(&format!("headers {start}..={end}"), || {
                        let headers = wire::headers(url, start, end)?;
                        linked(&headers, start)?;
                        Ok(headers)
                    });
                    let first_parent = headers[0].parent_hash;
                    let last = headers[headers.len() - 1].hash;
                    let candidates =
                        headers.into_iter().filter(|h| candidate(h, &proxy, &topic)).collect();
                    done.lock().unwrap().push((start, first_parent, last, candidates));
                    if n % 200 == 0 {
                        eprintln!("R0 coverage: header batch {n}/{batches} (block {start})");
                    }
                }
            });
        }
    });
    let mut batches_done = done.into_inner().unwrap();
    batches_done.sort_by_key(|b| b.0);
    assert_eq!(batches_done.len() as u64, batches, "a header batch is missing");
    for pair in batches_done.windows(2) {
        assert_eq!(
            pair[1].1, pair[0].2,
            "header batch at {} does not link to the batch before",
            pair[1].0
        );
    }
    let first_parent_hash = batches_done[0].1;
    let last_hash = batches_done[batches_done.len() - 1].2;
    let candidates: Vec<Header> = batches_done.into_iter().flat_map(|b| b.3).collect();
    eprintln!(
        "R0 coverage: {} headers verified, {} bloom candidates to read in full",
        to - from + 1,
        candidates.len()
    );

    let next = AtomicU64::new(0);
    let found = Mutex::new(Vec::new());
    thread::scope(|scope| {
        for _ in 0..RECEIPT_WORKERS.min(candidates.len()) {
            scope.spawn(|| {
                loop {
                    let n = next.fetch_add(1, Ordering::SeqCst) as usize;
                    if n >= candidates.len() {
                        break;
                    }
                    let header = &candidates[n];
                    let upgrades = with_retries(&format!("receipts of {}", header.number), || {
                        let receipts = wire::block_receipts(url, header.number)?;
                        verified_upgrades(header, &receipts, &proxy, &topic)
                    });
                    found.lock().unwrap().extend(upgrades);
                    if n % 2000 == 0 {
                        eprintln!("R0 coverage: candidate {n}/{} (block {})", candidates.len(), header.number);
                    }
                }
            });
        }
    });
    let mut upgrades = found.into_inner().unwrap();
    upgrades.sort_by_key(|u| (u.block, u.log_index));
    Coverage {
        from,
        to,
        first_parent_hash,
        last_hash,
        headers: to - from + 1,
        candidates: candidates.len() as u64,
        upgrades,
    }
}

/// The JSON-RPC wire boundary of the coverage (curl, compressed, read-only
/// methods only): every response is decoded here into typed values.
pub(crate) mod wire {
    use super::*;

    fn post(url: &str, body: &str) -> Result<serde_json::Value, String> {
        // URL and request reach curl on stdin, never on its argv (C4).
        let out = darkbot_bsc_execution::rpc_wire::post(
            child_command("curl"),
            url,
            body,
            darkbot_bsc_execution::rpc_wire::Limits {
                connect_timeout_s: 5,
                max_time_s: 30,
                compressed: true,
            },
        )?;
        serde_json::from_slice(&out).map_err(|e| format!("invalid JSON-RPC response: {e}"))
    }

    fn result(v: serde_json::Value, what: &str) -> Result<serde_json::Value, String> {
        if let Some(err) = v.get("error") {
            return Err(format!("JSON-RPC {what}: {err}"));
        }
        v.get("result")
            .cloned()
            .ok_or_else(|| format!("JSON-RPC {what}: missing result"))
    }

    fn data(v: &serde_json::Value, name: &str) -> Result<Vec<u8>, String> {
        v.get(name)
            .and_then(serde_json::Value::as_str)
            .and_then(|s| s.strip_prefix("0x"))
            .and_then(|h| hex::decode(h).ok())
            .ok_or_else(|| format!("{name} is not hex data"))
    }

    fn fixed<const N: usize>(v: &serde_json::Value, name: &str) -> Result<[u8; N], String> {
        data(v, name)?
            .try_into()
            .map_err(|_| format!("{name} is not {N} bytes"))
    }

    /// A quantity as its minimal big-endian bytes (zero is empty).
    fn quantity_bytes(v: &serde_json::Value, name: &str) -> Result<Vec<u8>, String> {
        let digits = v
            .get(name)
            .and_then(serde_json::Value::as_str)
            .and_then(|s| s.strip_prefix("0x"))
            .filter(|h| !h.is_empty() && h.bytes().all(|c| c.is_ascii_hexdigit()))
            .ok_or_else(|| format!("{name} is not a hex quantity"))?
            .trim_start_matches('0');
        let padded = if digits.len() % 2 == 1 { format!("0{digits}") } else { digits.to_string() };
        hex::decode(padded).map_err(|_| format!("{name} is not a hex quantity"))
    }

    fn quantity(v: &serde_json::Value, name: &str) -> Result<u64, String> {
        let bytes = quantity_bytes(v, name)?;
        if bytes.len() > 8 {
            return Err(format!("{name} exceeds 64 bits"));
        }
        Ok(bytes.iter().fold(0u64, |acc, b| (acc << 8) | u64::from(*b)))
    }

    /// The header fields in consensus order (BSC after Prague): each is a
    /// fixed-size or variable byte string, or a quantity (`true`).
    pub(crate) const HEADER_FIELDS: [(&str, bool); 21] = [
        ("parentHash", false),
        ("sha3Uncles", false),
        ("miner", false),
        ("stateRoot", false),
        ("transactionsRoot", false),
        ("receiptsRoot", false),
        ("logsBloom", false),
        ("difficulty", true),
        ("number", true),
        ("gasLimit", true),
        ("gasUsed", true),
        ("timestamp", true),
        ("extraData", false),
        ("mixHash", false),
        ("nonce", false),
        ("baseFeePerGas", true),
        ("withdrawalsRoot", false),
        ("blobGasUsed", true),
        ("excessBlobGas", true),
        ("parentBeaconBlockRoot", false),
        ("requestsHash", false),
    ];

    /// A header whose reported hash is keccak256 of its RLP encoding.
    pub(crate) fn header(v: &serde_json::Value) -> Result<Header, String> {
        let mut payload = Vec::new();
        for (name, is_quantity) in HEADER_FIELDS {
            let bytes = if is_quantity { quantity_bytes(v, name)? } else { data(v, name)? };
            rlp_string(&bytes, &mut payload);
        }
        let hash: [u8; 32] = fixed(v, "hash")?;
        let number = quantity(v, "number")?;
        if keccak256(rlp_list(&payload)).0 != hash {
            return Err(format!("header {number}: hash is not keccak256(rlp(header))"));
        }
        Ok(Header {
            number,
            hash,
            parent_hash: fixed(v, "parentHash")?,
            logs_bloom: fixed(v, "logsBloom")?,
            receipts_root: fixed(v, "receiptsRoot")?,
        })
    }

    /// Headers `from..=to` in one batch, in block order.
    pub(crate) fn headers(url: &str, from: u64, to: u64) -> Result<Vec<Header>, String> {
        let calls: Vec<String> = (from..=to)
            .map(|n| {
                format!(
                    r#"{{"jsonrpc":"2.0","id":{n},"method":"eth_getHeaderByNumber","params":["0x{n:x}"]}}"#
                )
            })
            .collect();
        let v = post(url, &format!("[{}]", calls.join(",")))?;
        let mut answers = v.as_array().ok_or("header batch answer is not a list")?.clone();
        answers.sort_by_key(|a| a.get("id").and_then(serde_json::Value::as_u64));
        if answers.len() as u64 != to - from + 1 {
            return Err(format!("header batch {from}..={to}: {} answers", answers.len()));
        }
        answers
            .into_iter()
            .zip(from..=to)
            .map(|(a, n)| {
                if a.get("id").and_then(serde_json::Value::as_u64) != Some(n) {
                    return Err(format!("header batch: no answer for {n}"));
                }
                header(&result(a, "eth_getHeaderByNumber")?)
            })
            .collect()
    }

    fn receipt(v: &serde_json::Value) -> Result<Receipt, String> {
        let logs = v
            .get("logs")
            .and_then(serde_json::Value::as_array)
            .ok_or("receipt lacks logs")?
            .iter()
            .map(|l| {
                Ok(Log {
                    address: fixed(l, "address")?,
                    topics: l
                        .get("topics")
                        .and_then(serde_json::Value::as_array)
                        .ok_or("log lacks topics")?
                        .iter()
                        .map(|t| {
                            t.as_str()
                                .and_then(|s| s.strip_prefix("0x"))
                                .and_then(|h| hex::decode(h).ok())
                                .and_then(|b| <[u8; 32]>::try_from(b).ok())
                                .ok_or_else(|| "topic is not 32 bytes".to_string())
                        })
                        .collect::<Result<_, String>>()?,
                    data: data(l, "data")?,
                    log_index: quantity(l, "logIndex")?,
                })
            })
            .collect::<Result<_, String>>()?;
        let tx_type = quantity(v, "type")?;
        Ok(Receipt {
            tx_type: u8::try_from(tx_type).map_err(|_| "receipt type exceeds a byte")?,
            status: quantity(v, "status")?,
            cumulative_gas_used: quantity(v, "cumulativeGasUsed")?,
            logs_bloom: fixed(v, "logsBloom")?,
            logs,
            transaction_hash: fixed(v, "transactionHash")?,
            transaction_index: quantity(v, "transactionIndex")?,
            block_hash: fixed(v, "blockHash")?,
        })
    }

    /// Every receipt of block `number`, in transaction order.
    pub(crate) fn block_receipts(url: &str, number: u64) -> Result<Vec<Receipt>, String> {
        let v = post(
            url,
            &format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"eth_getBlockReceipts","params":["0x{number:x}"]}}"#
            ),
        )?;
        result(v, "eth_getBlockReceipts")?
            .as_array()
            .ok_or("block receipts are not a list")?
            .iter()
            .map(receipt)
            .collect()
    }

    /// The hash of block `number` (an independent anchor).
    pub(crate) fn block_hash(url: &str, number: u64) -> Result<[u8; 32], String> {
        let v = post(
            url,
            &format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"eth_getBlockByNumber","params":["0x{number:x}",false]}}"#
            ),
        )?;
        fixed(&result(v, "eth_getBlockByNumber")?, "hash")
    }

    /// A storage word of `address` at `block` (historical state: archive).
    pub(crate) fn storage_at(
        url: &str,
        address: [u8; 20],
        slot: [u8; 32],
        block: u64,
    ) -> Result<[u8; 32], String> {
        let v = post(
            url,
            &format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"eth_getStorageAt","params":["0x{}","0x{}","0x{block:x}"]}}"#,
                hex::encode(address),
                hex::encode(slot)
            ),
        )?;
        let word = result(v, "eth_getStorageAt")?;
        word.as_str()
            .and_then(|s| s.strip_prefix("0x"))
            .and_then(|h| hex::decode(h).ok())
            .and_then(|b| <[u8; 32]>::try_from(b).ok())
            .ok_or_else(|| "storage word is not 32 bytes".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nibbles(key: &[u8]) -> Vec<u8> {
        key.iter().flat_map(|b| [b >> 4, b & 0x0f]).collect()
    }

    fn root(pairs: &[(&str, &str)]) -> String {
        let keys: Vec<Vec<u8>> = pairs.iter().map(|(k, _)| nibbles(k.as_bytes())).collect();
        let mut items: Vec<(&[u8], &[u8])> = keys
            .iter()
            .zip(pairs)
            .map(|(k, (_, v))| (k.as_slice(), v.as_bytes()))
            .collect();
        items.sort();
        hex::encode(keccak256(trie_node(&items)))
    }

    #[test]
    fn the_trie_root_matches_the_reference_vectors() {
        // ethereum/tests TrieTests (trietest.json): "dogs" and "puppy".
        assert_eq!(
            root(&[("doe", "reindeer"), ("dog", "puppy"), ("dogglesworth", "cat")]),
            "8aad789dff2f538bca5d8ea56e8abe10f4c7ba3a5dea95fea4cd6e7c3a1168d3"
        );
        assert_eq!(
            root(&[("do", "verb"), ("horse", "stallion"), ("doge", "coin"), ("dog", "puppy")]),
            "5991bb8c6514148a29db676a14ac506cd2cd5775ace63c30a4fe457715e9ac84"
        );
        assert_eq!(
            hex::encode(ordered_trie_root(&[])),
            "56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
        );
    }

    #[test]
    fn a_header_is_bound_by_its_hash_and_the_bloom_has_no_false_negative() {
        let mut v = serde_json::json!({
            "parentHash": format!("0x{}", "11".repeat(32)),
            "sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
            "miner": format!("0x{}", "22".repeat(20)),
            "stateRoot": format!("0x{}", "33".repeat(32)),
            "transactionsRoot": format!("0x{}", "44".repeat(32)),
            "receiptsRoot": format!("0x{}", "55".repeat(32)),
            "logsBloom": format!("0x{}", "00".repeat(256)),
            "difficulty": "0x2", "number": "0x7a2b7e5", "gasLimit": "0x42c1d80",
            "gasUsed": "0x12ef6f7", "timestamp": "0x6ad2f5f4",
            "extraData": "0xdb83010708", "mixHash": format!("0x{}", "00".repeat(32)),
            "nonce": "0x0000000000000000", "baseFeePerGas": "0x0",
            "withdrawalsRoot": "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421",
            "blobGasUsed": "0x0", "excessBlobGas": "0x0",
            "parentBeaconBlockRoot": format!("0x{}", "00".repeat(32)),
            "requestsHash": format!("0x{}", "66".repeat(32)),
        });
        let mut payload = Vec::new();
        for (name, is_quantity) in wire::HEADER_FIELDS {
            let text = v[name].as_str().unwrap().trim_start_matches("0x").to_string();
            let mut bytes = hex::decode(if text.len() % 2 == 1 { format!("0{text}") } else { text.clone() }).unwrap();
            if is_quantity {
                bytes = bytes.into_iter().skip_while(|b| *b == 0).collect();
            }
            rlp_string(&bytes, &mut payload);
        }
        v["hash"] = serde_json::json!(format!("0x{}", hex::encode(keccak256(rlp_list(&payload)))));
        let header = wire::header(&v).unwrap();
        assert_eq!(header.number, 0x7a2b7e5);
        v["gasUsed"] = serde_json::json!("0x12ef6f8");
        assert!(wire::header(&v).unwrap_err().contains("keccak256(rlp(header))"));

        let proxy = Address::from_str(PORTAL).unwrap().into_array();
        let topic = word(UPGRADED_TOPIC);
        let mut with_event = header.clone();
        bloom_add(&mut with_event.logs_bloom, &proxy);
        bloom_add(&mut with_event.logs_bloom, &topic);
        assert!(candidate(&with_event, &proxy, &topic));
        assert!(!candidate(&header, &proxy, &topic));
    }

    #[test]
    fn receipts_must_rebuild_the_root_and_hide_no_upgrade() {
        let proxy = Address::from_str(PORTAL).unwrap().into_array();
        let topic = word(UPGRADED_TOPIC);
        let mut implementation = [0u8; 32];
        implementation[12..].copy_from_slice(&[0xe1; 20]);
        let upgrade = Log { address: proxy, topics: vec![topic, implementation], data: vec![], log_index: 7 };
        let other = Log { address: [9; 20], topics: vec![[8; 32]], data: vec![1, 2], log_index: 6 };
        let receipts: Vec<Receipt> = [vec![other], vec![upgrade]]
            .into_iter()
            .enumerate()
            .map(|(i, logs)| {
                let mut bloom = [0u8; 256];
                for l in &logs {
                    bloom_add(&mut bloom, &l.address);
                    l.topics.iter().for_each(|t| bloom_add(&mut bloom, t));
                }
                Receipt {
                    tx_type: i as u8 * 2,
                    status: 1,
                    cumulative_gas_used: 21_000 * (i as u64 + 1),
                    logs_bloom: bloom,
                    logs,
                    transaction_hash: [i as u8 + 1; 32],
                    transaction_index: i as u64,
                    block_hash: [0xbb; 32],
                }
            })
            .collect();
        let mut header = Header {
            number: 5,
            hash: [0xbb; 32],
            parent_hash: [0xaa; 32],
            logs_bloom: receipts.iter().fold([0u8; 256], |mut b, r| {
                b.iter_mut().zip(r.logs_bloom).for_each(|(x, y)| *x |= y);
                b
            }),
            receipts_root: ordered_trie_root(&receipts.iter().map(encode_receipt).collect::<Vec<_>>()),
        };
        let found = verified_upgrades(&header, &receipts, &proxy, &topic).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].implementation, found[0].log_index), ([0xe1; 20], 7));
        // An endpoint that drops the upgrade's receipt is caught by the root.
        assert!(verified_upgrades(&header, &receipts[..1], &proxy, &topic).is_err());
        header.receipts_root = [0; 32];
        assert!(verified_upgrades(&header, &receipts, &proxy, &topic).is_err());
    }
}
