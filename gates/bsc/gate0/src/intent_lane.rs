//! Conservative single-sender pilot lane. All methods require exclusive access.
//! Retain deduplication records for the whole session; never evict or retry silently.
//! A transport integration must call begin_send BEFORE attempting a write, and
//! confirm only from independently reconciled canonical evidence, never an ACK.
//! Restart recovery, live state validity and transport integration are NOT provided.
use alloy_primitives::{Address, B256};
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket {
    chain_id: u64,
    sender: Address,
    nonce: u64,
    token: Address,
}
impl Ticket {
    pub fn binding(self) -> (u64, Address, Address) {
        (self.chain_id, self.sender, self.token)
    }
    pub fn nonce(self) -> u64 {
        self.nonce
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Duplicate,
    Capacity,
    Halted,
    NonceExhausted,
    UnknownTicket,
    InvalidState,
    NotReady,
    WrongHash,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Reserved,
    Prepared(B256),
    Sending(B256),
    Confirmed,
}
struct Entry {
    ticket: Ticket,
    state: State,
}

pub struct IntentLane {
    chain_id: u64,
    sender: Address,
    next_nonce: u64,
    head: u64,
    capacity: usize,
    seen: HashSet<Address>,
    entries: BTreeMap<u64, Entry>,
    halted: bool,
}
impl IntentLane {
    pub fn new(chain_id: u64, sender: Address, first_nonce: u64, capacity: usize) -> Self {
        Self {
            chain_id,
            sender,
            next_nonce: first_nonce,
            head: first_nonce,
            capacity,
            seen: HashSet::with_capacity(capacity),
            entries: BTreeMap::new(),
            halted: false,
        }
    }
    pub fn identity(&self) -> (u64, Address) {
        (self.chain_id, self.sender)
    }
    pub fn reserve(&mut self, token: Address) -> Result<Ticket, Error> {
        if self.halted {
            return Err(Error::Halted);
        }
        if self.seen.contains(&token) {
            return Err(Error::Duplicate);
        }
        if self.seen.len() >= self.capacity {
            return Err(Error::Capacity);
        }
        let next = self
            .next_nonce
            .checked_add(1)
            .ok_or(Error::NonceExhausted)?;
        let ticket = Ticket {
            chain_id: self.chain_id,
            sender: self.sender,
            nonce: self.next_nonce,
            token,
        };
        self.seen.insert(token);
        self.entries.insert(
            ticket.nonce,
            Entry {
                ticket,
                state: State::Reserved,
            },
        );
        self.next_nonce = next;
        Ok(ticket)
    }
    pub fn prepare(&mut self, ticket: Ticket, tx_hash: B256) -> Result<(), Error> {
        if self.halted {
            return Err(Error::Halted);
        }
        let entry = self
            .entries
            .get_mut(&ticket.nonce)
            .ok_or(Error::UnknownTicket)?;
        if entry.ticket != ticket {
            return Err(Error::UnknownTicket);
        }
        if entry.state != State::Reserved {
            return Err(Error::InvalidState);
        }
        entry.state = State::Prepared(tx_hash);
        Ok(())
    }
    pub fn begin_send(&mut self) -> Result<(Ticket, B256), Error> {
        if self.halted {
            return Err(Error::Halted);
        }
        let entry = self.entries.get_mut(&self.head).ok_or(Error::NotReady)?;
        let State::Prepared(hash) = entry.state else {
            return Err(Error::NotReady);
        };
        // Consume the permit before IO: even a partial/failed write cannot retry.
        entry.state = State::Sending(hash);
        Ok((entry.ticket, hash))
    }
    pub fn confirm(&mut self, ticket: Ticket, tx_hash: B256) -> Result<(), Error> {
        if self.halted {
            return Err(Error::Halted);
        }
        if ticket.nonce != self.head {
            return Err(Error::UnknownTicket);
        }
        let entry = self
            .entries
            .get_mut(&self.head)
            .ok_or(Error::UnknownTicket)?;
        if entry.ticket != ticket {
            return Err(Error::UnknownTicket);
        }
        let State::Sending(expected) = entry.state else {
            return Err(Error::InvalidState);
        };
        if tx_hash != expected {
            self.halted = true;
            return Err(Error::WrongHash);
        }
        entry.state = State::Confirmed;
        self.head += 1; // reservation rejected u64::MAX, so cannot overflow
        Ok(())
    }
    /// Timeout, drop, revert, stale state or unknown write outcome: stop the lane.
    /// No nonce release, replacement or automatic restart is authorized here.
    pub fn halt(&mut self) {
        self.halted = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    fn token(n: u64) -> Address {
        Address::from_word(B256::from(alloy_primitives::U256::from(n)))
    }
    fn lane(cap: usize) -> IntentLane {
        IntentLane::new(56, token(9000), 7, cap)
    }
    fn hash(n: u64) -> B256 {
        B256::from(alloy_primitives::U256::from(n))
    }
    #[test]
    fn duplicate_before_and_after_confirmation_never_consumes_nonce() {
        let mut l = lane(3);
        let a = l.reserve(token(1)).unwrap();
        assert_eq!(l.reserve(token(1)), Err(Error::Duplicate));
        l.prepare(a, hash(1)).unwrap();
        assert_eq!(l.begin_send(), Ok((a, hash(1))));
        l.confirm(a, hash(1)).unwrap();
        assert_eq!(l.reserve(token(1)), Err(Error::Duplicate));
        assert_eq!(l.reserve(token(2)).unwrap().nonce(), 8);
    }
    #[test]
    fn reversed_build_completion_cannot_bypass_unready_or_unconfirmed_head() {
        let mut l = lane(2);
        let a = l.reserve(token(1)).unwrap();
        let b = l.reserve(token(2)).unwrap();
        l.prepare(b, hash(2)).unwrap();
        assert_eq!(l.begin_send(), Err(Error::NotReady));
        l.prepare(a, hash(1)).unwrap();
        assert_eq!(l.begin_send(), Ok((a, hash(1))));
        assert_eq!(l.begin_send(), Err(Error::NotReady));
        assert_eq!(l.confirm(b, hash(2)), Err(Error::UnknownTicket));
        l.confirm(a, hash(1)).unwrap();
        assert_eq!(l.begin_send(), Ok((b, hash(2))));
    }
    #[test]
    fn unknown_write_outcome_halts_without_retry_replacement_or_nonce_reuse() {
        let mut l = lane(3);
        let a = l.reserve(token(1)).unwrap();
        l.prepare(a, hash(1)).unwrap();
        l.begin_send().unwrap();
        l.halt();
        assert_eq!(l.begin_send(), Err(Error::Halted));
        assert_eq!(l.reserve(token(2)), Err(Error::Halted));
        assert_eq!(l.prepare(a, hash(2)), Err(Error::Halted));
        assert_eq!(l.confirm(a, hash(1)), Err(Error::Halted));
    }
    #[test]
    fn conflicting_preparation_and_confirmation_rejected() {
        let mut l = lane(1);
        let a = l.reserve(token(1)).unwrap();
        l.prepare(a, hash(1)).unwrap();
        assert_eq!(l.prepare(a, hash(2)), Err(Error::InvalidState));
        l.begin_send().unwrap();
        assert_eq!(l.confirm(a, hash(2)), Err(Error::WrongHash));
        assert_eq!(l.begin_send(), Err(Error::Halted));
    }
    #[test]
    fn tickets_cannot_cross_sender_or_chain_lanes() {
        let mut a = lane(1);
        let ticket = a.reserve(token(1)).unwrap();
        for (chain, sender) in [(56, token(9001)), (97, token(9000))] {
            let mut other = IntentLane::new(chain, sender, 7, 1);
            other.reserve(token(1)).unwrap();
            assert_eq!(other.prepare(ticket, hash(1)), Err(Error::UnknownTicket));
        }
    }
    #[test]
    fn capacity_and_nonce_exhaustion_fail_closed() {
        let mut l = lane(1);
        l.reserve(token(1)).unwrap();
        assert_eq!(l.reserve(token(2)), Err(Error::Capacity));
        let mut l = IntentLane::new(56, token(9), u64::MAX, 1);
        assert_eq!(l.reserve(token(1)), Err(Error::NonceExhausted));
    }
    #[test]
    fn eight_workers_repeated_signals_produce_exactly_1000_permits() {
        let l = Arc::new(Mutex::new(lane(1000)));
        std::thread::scope(|scope| {
            for _ in 0..8 {
                let l = Arc::clone(&l);
                scope.spawn(move || {
                    for n in 0..1000 {
                        let mut l = l.lock().unwrap();
                        match l.reserve(token(n)) {
                            Ok(t) => l.prepare(t, hash(n)).unwrap(),
                            Err(e) => assert_eq!(e, Error::Duplicate),
                        }
                    }
                });
            }
        });
        let mut l = l.lock().unwrap();
        let mut hashes = HashSet::new();
        for nonce in 7..1007 {
            let (t, h) = l.begin_send().unwrap();
            assert_eq!(t.nonce(), nonce);
            assert!(hashes.insert(h));
            assert_eq!(l.begin_send(), Err(Error::NotReady));
            l.confirm(t, h).unwrap();
        }
        assert_eq!(l.begin_send(), Err(Error::NotReady));
        assert_eq!(hashes.len(), 1000);
    }
}

#[cfg(test)]
mod lifecycle_measurements {
    use super::*;
    use std::time::{Duration, Instant};
    fn token(n: u8) -> Address {
        Address::repeat_byte(n)
    }
    fn hash(n: u8) -> B256 {
        B256::repeat_byte(n)
    }
    #[test]
    fn targeted_head_of_line_unknown_head_blocks_all_prepared_followers() {
        for resolve in [false, true] {
            let mut lane = IntentLane::new(56, token(200), 7, 9);
            let head = lane.reserve(token(1)).unwrap();
            lane.prepare(head, hash(1)).unwrap();
            lane.begin_send().unwrap();
            let mut first_blocked = Vec::new();
            let mut actionable = Vec::new();
            for n in 2..10 {
                let t = lane.reserve(token(n)).unwrap();
                lane.prepare(t, hash(n)).unwrap();
                let start = Instant::now();
                actionable.push(start);
                assert_eq!(lane.begin_send(), Err(Error::NotReady));
                first_blocked.push(start.elapsed().as_nanos() as u64);
            }
            // Deliberately unresolved for 25ms. This is injected local waiting,
            // not an observed builder/chain delay or a BUY-latency benchmark.
            std::thread::sleep(Duration::from_millis(25));
            assert_eq!(lane.begin_send(), Err(Error::NotReady));
            if resolve {
                lane.confirm(head, hash(1)).unwrap();
                assert_eq!(lane.begin_send().unwrap().0.nonce(), 8);
            } else {
                lane.halt();
                assert_eq!(lane.begin_send(), Err(Error::Halted));
            }
            let durations: Vec<u64> = actionable
                .iter()
                .map(|t| t.elapsed().as_nanos() as u64)
                .collect();
            assert!(durations.iter().all(|n| *n > 1_000_000));
            println!(
                "G06_HOL {}",
                serde_json::json!({"outcome":if resolve{"SIMULATED_RESOLUTION"}else{"HALT"},"blocked_followers":8,"actionable_to_first_blocked_ns":first_blocked,"actionable_to_outcome_ns":durations,"injected_hold_ns":25_000_000,"network_measurement":false})
            );
        }
    }
    #[test]
    fn diagnostic_restart_loses_all_four_in_memory_states() {
        for phase in ["Reserved", "Prepared", "Sending", "Confirmed"] {
            let mut old = IntentLane::new(56, token(200), 7, 1);
            let ticket = old.reserve(token(1)).unwrap();
            if phase != "Reserved" {
                old.prepare(ticket, hash(1)).unwrap();
            }
            if phase == "Sending" || phase == "Confirmed" {
                old.begin_send().unwrap();
            }
            if phase == "Confirmed" {
                old.confirm(ticket, hash(1)).unwrap();
            }
            drop(old);
            // Explicitly demonstrate the UNSAFE restart assumption: stale nonce 7.
            let mut restarted = IntentLane::new(56, token(200), 7, 1);
            let again = restarted.reserve(token(1)).unwrap();
            assert_eq!(again, ticket);
            restarted.prepare(again, hash(1)).unwrap();
            assert!(restarted.begin_send().is_ok());
            println!(
                "G06_RECOVERY {}",
                serde_json::json!({"previous_state":phase,"blind_reuse_possible":true,"status":"BLOCKER_REPRODUCED","restart_nonce_source":"deliberately_stale"})
            );
        }
    }
}
