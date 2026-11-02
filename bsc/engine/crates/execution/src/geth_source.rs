//! REV.1.0 R6 (audit 011): the local bsc-geth source. Its C1.4 arena's
//! peer lanes are polled ([`Hot0Poller`]); each record's signed bytes are a
//! full transaction, its signer and nonce recovered here (as the WSS
//! sources verify theirs) before it reaches the runtime's ingress like a
//! WSS source's. UNQUALIFIED until an arena is configured
//! (`BSC_INGRESS_GETH_*`) and qualified (G0.4, a patched node: external).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use alloy_primitives::keccak256;
use darkbot_bsc_geth_ipc::hot0::{Hot0Config, Hot0Decision, Hot0Outcome, Hot0Poller};
use darkbot_bsc_geth_ipc::ingress::{C14LanePlan, decode_tx_signature};
use darkbot_bsc_geth_ipc::protocol_v2::{LaneClassV2, arena_path_v2};

/// Where the local geth's arena is and how it is polled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GethArena {
    /// The directory of its arena files.
    pub dir: PathBuf,
    /// The arena generation the node writes.
    pub generation: u64,
    /// Its peer lanes (max peers + churn reserve, the C1.4 plan).
    pub peer_lanes: usize,
    /// The wait after a poll that found nothing.
    pub idle: Duration,
    /// Its intent slots and dedup sets (a power of two).
    pub hot0: (usize, usize),
    /// The interval of [`GethEvent::Alive`] while the arena is open.
    pub alive: Duration,
    /// The wait before an arena that is not open is tried again.
    pub retry: Duration,
}

/// What the source tells the runtime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GethEvent {
    /// The arena is open.
    Connected,
    /// A signed transaction, its signer and nonce recovered.
    Full {
        hash: [u8; 32],
        raw: Vec<u8>,
        from: [u8; 20],
        nonce: u64,
    },
    /// Bytes that are no transaction of `chain_id` (or not signed by a
    /// recoverable key, or not the transaction their hash names).
    Rejected,
    /// The arena is still open: its lanes carry signed transactions only,
    /// a quiet one is no outage.
    Alive,
    /// The arena could not be opened, its node shut it down, or the stop.
    Disconnected,
}

/// The signer and nonce of the signed transaction `raw` of the chain
/// `chain_id` (legacy EIP-155 or typed 1–4, no blob wrapper).
pub fn sender_of(raw: &[u8], chain_id: u64) -> Result<([u8; 20], u64), String> {
    let signed = decode_tx_signature(raw).map_err(|e| format!("{e:?}"))?;
    if signed.chain_id != chain_id {
        return Err(format!("chain {} is not {chain_id}", signed.chain_id));
    }
    // R7: libsecp256k1, as the WSS sources recover theirs (BSC3-063).
    let from = crate::wss::recover(
        &keccak256(&signed.signing_payload).0,
        &signed.r,
        &signed.s,
        signed.y_parity,
    )
    .map_err(|e| format!("{e:?}"))?;
    Ok((from, signed.nonce))
}

/// A peer-lane record's event: [`GethEvent::Full`] when `raw` is the
/// transaction `hash` names and a signed one of `chain_id`, else
/// [`GethEvent::Rejected`].
fn event_of(raw: Vec<u8>, hash: &[u8], chain_id: u64) -> GethEvent {
    if keccak256(&raw).as_slice() != hash {
        return GethEvent::Rejected;
    }
    match sender_of(&raw, chain_id) {
        Ok((from, nonce)) => GethEvent::Full {
            hash: keccak256(&raw).0,
            raw,
            from,
            nonce,
        },
        Err(_) => GethEvent::Rejected,
    }
}

/// Reports `up` once per change (the first state always).
fn report(state: &mut Option<bool>, up: bool, sink: &mut dyn FnMut(GethEvent, Instant)) {
    if *state != Some(up) {
        *state = Some(up);
        let event = match up {
            true => GethEvent::Connected,
            false => GethEvent::Disconnected,
        };
        sink(event, Instant::now());
    }
}

/// Polls `arena` until `stop`: [`GethEvent::Connected`] once it is open,
/// each peer-lane record as [`GethEvent::Full`] or [`GethEvent::Rejected`]
/// with the moment it was read (the source's T_ingress, as a WSS frame's
/// receipt), [`GethEvent::Alive`] every `alive` while it is open;
/// [`GethEvent::Disconnected`] when it is not openable or its node shut it
/// down (drained first), tried again after `retry`, and at the stop.
pub fn run(
    arena: &GethArena,
    chain_id: u64,
    stop: &AtomicBool,
    sink: &mut dyn FnMut(GethEvent, Instant),
) {
    let plan = C14LanePlan::production(arena.peer_lanes);
    let peers: Vec<_> = plan
        .lanes()
        .into_iter()
        .filter(|l| l.class == LaneClassV2::PeerIngress)
        .collect();
    let path = arena_path_v2(&arena.dir, arena.generation);
    let config = Hot0Config {
        intent_slots: arena.hot0.0,
        dedup_sets: arena.hot0.1,
    };
    let mut state = None;
    while !stop.load(Ordering::Acquire) {
        if let Ok(mut poller) = Hot0Poller::open(&path, arena.generation, &peers, None, config) {
            let mut alive_at: Option<Instant> = None;
            let mut closing = false;
            while !stop.load(Ordering::Acquire) {
                if alive_at.is_none_or(|at| at.elapsed() >= arena.alive) {
                    let now = Instant::now();
                    alive_at = Some(now);
                    closing = poller.shut_down();
                    if !closing {
                        report(&mut state, true, sink);
                        sink(GethEvent::Alive, now);
                    }
                }
                let mut read = None;
                let outcome = poller.poll_once(
                    |_| {},
                    |record| {
                        read = Some((
                            record.payload.to_vec(),
                            record.hash.to_vec(),
                            Instant::now(),
                        ));
                        // The runtime's ingress dedups and decides; no intent slot.
                        Hot0Decision::Reject
                    },
                );
                match (outcome, read) {
                    // Its slot is released: the copy goes to the runtime.
                    (Hot0Outcome::Rejected, Some((raw, hash, at))) => {
                        report(&mut state, true, sink);
                        sink(event_of(raw, &hash, chain_id), at);
                    }
                    // Shut down and drained: no record follows.
                    (Hot0Outcome::Idle, _) if closing => break,
                    (Hot0Outcome::Idle, _) => std::thread::sleep(arena.idle),
                    _ => {}
                }
            }
        }
        report(&mut state, false, sink);
        let since = Instant::now();
        while !stop.load(Ordering::Acquire) && since.elapsed() < arena.retry {
            std::thread::sleep(arena.idle.min(arena.retry.saturating_sub(since.elapsed())));
        }
    }
    report(&mut state, false, sink);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wss::{BSC_MAINNET_CHAIN_ID, verify_tx_object};
    use darkbot_bsc_geth_ipc::RecordKind;
    use darkbot_bsc_geth_ipc::ingress::{SOURCE_ETH_BROADCAST, ingress_aux};
    use darkbot_bsc_geth_ipc::protocol_v2::{ArenaV2, CLOCK_ID_MONOTONIC, ProducerV2, RecordV2};
    use darkbot_bsc_trading::signer::Signer;
    use darkbot_bsc_tx_encoding::rlp::{encode_bytes, encode_list, encode_u64, encode_uint_be};
    use serde_json::Value;

    /// Real BSC transactions (the WSS verifier's fixtures): legacy, type 1
    /// and type 2, each with its announced signer, nonce and bytes.
    fn verified() -> Vec<crate::wss::VerifiedTx> {
        [
            include_str!("../tests/fixtures/wss/legacy.json"),
            include_str!("../tests/fixtures/wss/type1.json"),
            include_str!("../tests/fixtures/wss/type2.json"),
        ]
        .iter()
        .flat_map(|t| serde_json::from_str::<Vec<Value>>(t).unwrap())
        .map(|tx| {
            verify_tx_object(&serde_json::to_vec(&tx).unwrap(), BSC_MAINNET_CHAIN_ID).unwrap()
        })
        .collect()
    }

    fn arena_at(dir: PathBuf) -> GethArena {
        GethArena {
            dir,
            generation: 7,
            peer_lanes: 1,
            idle: Duration::from_millis(1),
            hot0: (4, 16),
            alive: Duration::ZERO,
            retry: Duration::from_millis(1),
        }
    }

    /// What `run` reports over `arena` until `done` holds, then 20 ms more
    /// (many polls and retries: what else would come), at most 5 s: its
    /// events but alive, and how often it was alive.
    fn reported(
        arena: &GethArena,
        done: fn(&[GethEvent], usize) -> bool,
    ) -> (Vec<GethEvent>, usize) {
        let (stop, finished) = (AtomicBool::new(false), AtomicBool::new(false));
        let (mut events, mut alive) = (Vec::new(), 0);
        std::thread::scope(|s| {
            s.spawn(|| {
                run(arena, BSC_MAINNET_CHAIN_ID, &stop, &mut |e, _| {
                    match e {
                        GethEvent::Alive => alive += 1,
                        e => events.push(e),
                    }
                    if done(&events, alive) {
                        finished.store(true, Ordering::Release);
                    }
                })
            });
            let start = Instant::now();
            while !finished.load(Ordering::Acquire) && start.elapsed() < Duration::from_secs(5) {
                std::thread::sleep(Duration::from_millis(1));
            }
            std::thread::sleep(Duration::from_millis(20));
            stop.store(true, Ordering::Release);
        });
        (events, alive)
    }

    /// From the signed bytes alone, every fixture's signer and nonce, as the
    /// WSS verifier found them from the provider's fields; another chain
    /// or a changed signature is no such transaction.
    #[test]
    fn the_signer_of_signed_bytes_is_the_verified_one() {
        let all = verified();
        assert!(
            all.iter().any(|v| v.tx_type == Some(2)) && all.iter().any(|v| v.tx_type.is_none())
        );
        for v in &all {
            assert_eq!(
                sender_of(&v.raw, BSC_MAINNET_CHAIN_ID),
                Ok((v.from, v.nonce))
            );
            assert!(sender_of(&v.raw, 97).is_err());
            let mut changed = v.raw.clone();
            let last = changed.len() - 1;
            changed[last] ^= 0x01;
            assert_ne!(
                sender_of(&changed, BSC_MAINNET_CHAIN_ID),
                Ok((v.from, v.nonce))
            );
        }
        assert!(sender_of(&[], BSC_MAINNET_CHAIN_ID).is_err());
    }

    /// `t ‖ rlp(fields, y, r, s)` signed by the key 0x42…, its `s` replaced
    /// by `n - s` (the same signature, high-s) when `high`.
    fn typed(t: u8, fields: &[Vec<u8>], high: bool) -> Vec<u8> {
        let key = k256::ecdsa::SigningKey::from_slice(&[0x42; 32]).unwrap();
        let mut payload = vec![t];
        payload.extend(encode_list(fields));
        let (sig, id) = key
            .sign_prehash_recoverable(keccak256(&payload).as_slice())
            .unwrap();
        let (s, y) = match high {
            false => (*sig.s(), id.is_y_odd()),
            true => (-*sig.s(), !id.is_y_odd()),
        };
        let mut items = fields.to_vec();
        items.push(encode_u64(u64::from(y)));
        items.push(encode_uint_be(&sig.r().to_bytes()));
        items.push(encode_uint_be(&s.to_bytes()));
        let mut raw = vec![t];
        raw.extend(encode_list(&items));
        raw
    }

    /// Review: type 3 and type 4 recover their signer and nonce; a blob
    /// network wrapper, a high-s signature, a pre-EIP-155 legacy one and
    /// bytes that are not the transaction their hash names are refused.
    #[test]
    fn every_typed_layout_recovers_its_signer_and_the_rest_is_refused() {
        let signer = Signer::from_secret_bytes(&[0x42; 32]).unwrap();
        let from = signer.address().into_array();
        let fee = [56, 9, 1, 2, 21_000].map(encode_u64);
        let mut head: Vec<Vec<u8>> = fee.to_vec();
        head.extend([
            encode_bytes(&[0x11; 20]),
            encode_u64(0),
            encode_bytes(&[]),
            encode_list::<Vec<u8>>(&[]),
        ]);
        let mut blob = head.clone();
        blob.extend([encode_u64(1), encode_list(&[encode_bytes(&[0x01; 32])])]);
        let mut auth = head.clone();
        auth.push(encode_list(&[encode_list(&[
            encode_u64(56),
            encode_bytes(&[0x22; 20]),
            encode_u64(0),
            encode_u64(0),
            encode_bytes(&[0x01; 32]),
            encode_bytes(&[0x02; 32]),
        ])]));
        for (t, fields) in [(3, &blob), (4, &auth)] {
            let raw = typed(t, fields, false);
            assert_eq!(sender_of(&raw, BSC_MAINNET_CHAIN_ID), Ok((from, 9)));
            assert_eq!(
                event_of(
                    raw.clone(),
                    keccak256(&raw).as_slice(),
                    BSC_MAINNET_CHAIN_ID
                ),
                GethEvent::Full {
                    hash: keccak256(&raw).0,
                    raw: raw.clone(),
                    from,
                    nonce: 9,
                }
            );
            assert_eq!(
                event_of(raw.clone(), &[0; 32], BSC_MAINNET_CHAIN_ID),
                GethEvent::Rejected
            );
            assert!(sender_of(&typed(t, fields, true), BSC_MAINNET_CHAIN_ID).is_err());
        }
        // The blob network form: `3 ‖ rlp([tx, blobs, commitments, proofs])`.
        let signed = typed(3, &blob, false);
        let mut wrapper = vec![3];
        let empty = encode_list::<Vec<u8>>(&[]);
        wrapper.extend(encode_list(&[
            signed[1..].to_vec(),
            empty.clone(),
            empty.clone(),
            empty,
        ]));
        assert!(sender_of(&wrapper, BSC_MAINNET_CHAIN_ID).is_err());
        // Legacy with v = 27: no chain id signed.
        let mut legacy: Vec<Vec<u8>> = [9, 1, 21_000].map(encode_u64).to_vec();
        legacy.extend([
            encode_bytes(&[0x11; 20]),
            encode_u64(0),
            encode_bytes(&[]),
            encode_u64(27),
            encode_bytes(&[0x01; 32]),
            encode_bytes(&[0x02; 32]),
        ]);
        assert!(sender_of(&encode_list(&legacy), BSC_MAINNET_CHAIN_ID).is_err());
    }

    /// A synthetic C1.4 arena: the node's EARLY_TX_SEEN of a real signed
    /// transaction reaches the runtime as a full transaction with its
    /// signer, bytes that are none as rejected, the open arena as alive; a
    /// stop ends it. Shut down by its node, drained first, then gone.
    #[test]
    fn an_arena_record_is_a_full_transaction_with_its_signer() {
        let dir = std::env::temp_dir().join(format!("bsc-geth-source-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let arena = arena_at(dir.clone());
        let plan = C14LanePlan::production(1);
        let path = arena_path_v2(&dir, 7);
        let mut created = ArenaV2::create(&path, 7, &plan.lanes()).unwrap();
        let peer = plan.lanes()[1];
        created.bind_peer_lane(peer.lane_id, [7; 32]).unwrap();
        let mut producer = ProducerV2::open(&path, 7, peer).unwrap();
        let v = &verified()[0];
        let mut publish = |payload: &[u8]| {
            producer
                .publish(&RecordV2 {
                    kind: RecordKind::EarlyTxSeen,
                    flags: 0,
                    timestamp_ns: 1_000,
                    clock_id: CLOCK_ID_MONOTONIC,
                    source: SOURCE_ETH_BROADCAST,
                    peer: [0x22; 32],
                    hash: keccak256(payload).0,
                    aux: ingress_aux(999),
                    payload,
                })
                .unwrap();
        };
        publish(&v.raw);
        publish(b"no transaction");
        let full = GethEvent::Full {
            hash: v.hash,
            raw: v.raw.clone(),
            from: v.from,
            nonce: v.nonce,
        };
        // Connected, the transaction, the other bytes, alive: then the stop.
        let (events, alive) = reported(&arena, |events, alive| events.len() == 3 && alive > 1);
        assert_eq!(
            events,
            [
                GethEvent::Connected,
                full.clone(),
                GethEvent::Rejected,
                GethEvent::Disconnected
            ]
        );
        assert!(alive > 1);
        // The node shuts the arena down: its last record still comes, then
        // it is gone (no alive, no reconnect while it stays shut).
        publish(&v.raw);
        created.begin_shutdown().unwrap();
        let (events, alive) = reported(&arena, |events, _| {
            events.contains(&GethEvent::Disconnected)
        });
        assert_eq!(
            (events, alive),
            (vec![GethEvent::Connected, full, GethEvent::Disconnected], 0)
        );
        drop(created);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// No arena at the configured place: disconnected once, tried again
    /// until it is there: connected.
    #[test]
    fn a_missing_arena_is_a_disconnect_until_it_is_there() {
        let dir = std::env::temp_dir().join(format!("bsc-geth-late-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let arena = arena_at(dir.clone());
        let stop = AtomicBool::new(false);
        let mut events = Vec::new();
        std::thread::scope(|s| {
            s.spawn(|| {
                run(&arena, BSC_MAINNET_CHAIN_ID, &stop, &mut |e, _| {
                    if e == GethEvent::Connected {
                        stop.store(true, Ordering::Release);
                    }
                    if e != GethEvent::Alive {
                        events.push(e);
                    }
                })
            });
            std::thread::sleep(Duration::from_millis(20));
            let plan = C14LanePlan::production(1);
            ArenaV2::create(&arena_path_v2(&dir, 7), 7, &plan.lanes()).unwrap();
        });
        assert_eq!(
            events,
            [
                GethEvent::Disconnected,
                GethEvent::Connected,
                GethEvent::Disconnected
            ]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
