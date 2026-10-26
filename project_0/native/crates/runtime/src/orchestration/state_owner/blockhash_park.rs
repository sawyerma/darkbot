//! DEC-01 (V2-09): lane candidates refused before planning because the cached blockhash was
//! stale or missing. The owner asks the refresh worker for a new blockhash at once (a coalesced
//! wake-up, no network call of its own) and parks the candidate: at most one entry per account
//! (bounded by the account population like the lanes), kept until the cache stores a fresh
//! blockhash after the refusal's check — the worker rings the owner's doorbell when it stores
//! one. It then returns to its original lane (L0 stays L0) with its original work (`T_rx`), first
//! receive time and input revision; the detection checks every state/freshness/admission gate
//! again, so an expired `T_rx` falls out there as `StateRace`. An evaluation of the account past
//! a fresh blockhash with work at least as new supersedes the entry (never evaluated twice); an
//! evaluation that never reached a fresh blockhash leaves it parked (never lost). Only lane
//! candidates are parked; every other detection keeps the late check (the pipeline's `Off`).
use super::*;
use crate::analytics::evidence::{BlockhashParkEntry, BlockhashParkEvent};
use crate::execution::pipeline::BlockhashPark;

/// One parked candidate: its lane entry and the generation it waits beyond.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Parked {
    pub(super) work: DetectWork,
    pub(super) t_rx_first: u64,
    pub(super) lane: Lane,
    pub(super) revision: u64,
    pub(super) last_forwarded_slot: u64,
    /// The cache generation read before the refusal's freshness check.
    pub(super) generation: u64,
}

impl Parked {
    /// A second refusal of the account: first receive time kept, newest work (slot, then
    /// receive time) and revision, the stronger lane (a parked candidate is never demoted), the
    /// newest refusal's generation.
    fn merge(self, newer: Parked) -> Parked {
        let work = if (newer.work.2, newer.work.1) < (self.work.2, self.work.1) {
            self.work
        } else {
            newer.work
        };
        Parked {
            work,
            t_rx_first: self.t_rx_first.min(newer.t_rx_first),
            lane: self.lane.min(newer.lane),
            revision: self.revision.max(newer.revision),
            last_forwarded_slot: self.last_forwarded_slot.min(newer.last_forwarded_slot),
            generation: self.generation.max(newer.generation),
        }
    }
    fn entry(&self) -> BlockhashParkEntry {
        BlockhashParkEntry {
            account: self.work.0,
            trigger: std::borrow::Cow::Borrowed(self.work.3),
            slot: self.work.2,
            t_rx_ns: self.work.1,
            t_rx_first_ns: self.t_rx_first,
            lane: std::borrow::Cow::Borrowed(self.lane.name()),
            revision: self.revision,
            generation: self.generation,
        }
    }
}

/// Counters of the parking (Liquidator tab; cumulative for the run).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ParkStats {
    /// Candidates refused before planning (loss `Build`, cause BLOCKHASH_STALE).
    pub(super) refused: u64,
    /// Refusals coalesced into the account's parked entry.
    pub(super) merged: u64,
    /// Entries returned to their lane after a newer fresh blockhash.
    pub(super) rearmed: u64,
    /// Entries covered by a later evaluation of the account past a fresh blockhash.
    pub(super) superseded: u64,
    /// Refresh wake-ups sent to the worker / already pending (coalesced).
    pub(super) refresh_requests: u64,
    pub(super) refresh_coalesced: u64,
}

/// The parked candidates, one per account.
#[derive(Debug, Default)]
pub(super) struct BlockhashParking {
    parked: HashMap<Pubkey, Parked>,
    /// The cache generation the parked entries were last checked against.
    scanned_generation: u64,
    pub(super) stats: ParkStats,
}

impl BlockhashParking {
    pub(super) fn len(&self) -> usize {
        self.parked.len()
    }
    #[cfg(test)]
    pub(super) fn get(&self, account: &Pubkey) -> Option<&Parked> {
        self.parked.get(account)
    }
    /// A blockhash newer than the last check exists while candidates are parked: the owner's
    /// idle wait ends at once (A23-01) and the next pass checks them.
    pub(super) fn pending(&self, generation: u64) -> bool {
        !self.parked.is_empty() && generation > self.scanned_generation
    }
    /// Parks a refused candidate; `true` when it was coalesced into the account's entry.
    fn park(&mut self, p: Parked) -> (Parked, bool) {
        self.stats.refused = self.stats.refused.saturating_add(1);
        let (entry, merged) = match self.parked.get(&p.work.0) {
            Some(old) => (old.merge(p), true),
            None => (p, false),
        };
        if merged {
            self.stats.merged = self.stats.merged.saturating_add(1);
        }
        self.parked.insert(p.work.0, entry);
        (entry, merged)
    }
    /// The entries a fresh blockhash of `generation` re-arms (stored after their refusal's
    /// check), removed in a deterministic order (the lanes order them again by their keys).
    fn due(&mut self, generation: u64) -> Vec<Parked> {
        self.scanned_generation = generation;
        let mut due: Vec<Parked> = self
            .parked
            .values()
            .filter(|p| p.generation < generation)
            .copied()
            .collect();
        due.sort_by_key(|p| (p.t_rx_first, p.work.0));
        for p in &due {
            self.parked.remove(&p.work.0);
        }
        self.stats.rearmed = self.stats.rearmed.saturating_add(due.len() as u64);
        due
    }
    /// The account's entry, when `work` (an evaluation past a fresh blockhash) is at least as
    /// new as the parked work (slot, then receive time).
    fn supersede(&mut self, work: &DetectWork) -> Option<Parked> {
        let p = self.parked.get(&work.0)?;
        if (p.work.2, p.work.1) > (work.2, work.1) {
            return None;
        }
        let p = self.parked.remove(&work.0)?;
        self.stats.superseded = self.stats.superseded.saturating_add(1);
        Some(p)
    }
}

impl ShadowOwner {
    /// After a detection under its ownership: a lane candidate refused on a stale blockhash
    /// wakes the refresh worker and is parked; an evaluation past a fresh blockhash supersedes
    /// the account's parked entry; anything else (no evaluation reached the check, or a stale
    /// blockhash without parking) leaves it parked.
    pub(super) fn blockhash_after_detect(
        &mut self,
        work: DetectWork,
        origin: Option<generations::DetectOrigin>,
        early: Option<BlockhashPark>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match (early, origin) {
            (Some(BlockhashPark::Refused { generation }), Some(origin)) => {
                self.request_blockhash_refresh();
                let (entry, merged) = self.blockhash_parking.park(Parked {
                    work,
                    t_rx_first: origin.t_rx_first_ns.min(work.1),
                    lane: origin.lane,
                    revision: origin.revision,
                    last_forwarded_slot: origin.last_forwarded_slot,
                    generation,
                });
                let event = if merged {
                    BlockhashParkEvent::Merged
                } else {
                    BlockhashParkEvent::Parked
                };
                self.emit_park(event, &entry)
            }
            (Some(BlockhashPark::Passed), _) => match self.blockhash_parking.supersede(&work) {
                Some(p) => self.emit_park(BlockhashParkEvent::Superseded, &p),
                None => Ok(()),
            },
            _ => Ok(()),
        }
    }

    fn request_blockhash_refresh(&mut self) {
        let stats = &mut self.blockhash_parking.stats;
        if self.blockhash.request_refresh() {
            stats.refresh_requests = stats.refresh_requests.saturating_add(1);
        } else {
            stats.refresh_coalesced = stats.refresh_coalesced.saturating_add(1);
        }
    }

    /// Once per owner pass: after a newer blockhash the parked entries it re-arms return to
    /// their original lanes (coalesced with a pending entry of the account, never a second
    /// one), journaled as one record. A newer blockhash already stale re-arms nothing and asks
    /// the worker again. Without a newer blockhash nothing is scanned (no busy loop).
    pub(super) fn rearm_blockhash_parked(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let generation = self.blockhash.generation();
        if !self.blockhash_parking.pending(generation) {
            return Ok(());
        }
        if self
            .blockhash
            .fresh(self.clock.now_ns(), self.blockhash_max_age_ns)
            .is_none()
        {
            self.blockhash_parking.scanned_generation = generation;
            self.request_blockhash_refresh();
            return Ok(());
        }
        let due = self.blockhash_parking.due(generation);
        if due.is_empty() {
            return Ok(());
        }
        for p in &due {
            enqueue_or_coalesce_candidate_from(
                p.work.0,
                (
                    LaneAdmission {
                        t_rx_ns: p.work.1,
                        slot: p.work.2,
                        trigger: p.work.3,
                        lane: p.lane,
                        revision: p.revision,
                        last_forwarded_slot: p.last_forwarded_slot,
                    },
                    p.t_rx_first,
                ),
                &mut self.oracle_lanes,
                &mut self.metrics.oracle_enqueued,
                &mut self.metrics.oracle_coalesced,
                &mut self.metrics.oracle_max_pending,
            );
        }
        self.evidence
            .emit(OwnerEvidence::Record(RecordEvent::BlockhashRearm {
                cache_generation: generation,
                entries: due.iter().map(Parked::entry).collect(),
            }))?;
        Ok(())
    }

    /// The parking counters for the shutdown journal (the entries still parked end with the
    /// run like the lanes' pending entries).
    pub(super) fn blockhash_park_line(&self) -> String {
        let s = self.blockhash_parking.stats;
        format!(
            "blockhash_park: refused={} merged={} rearmed={} superseded={} parked_at_end={} \
             refresh_requests={} refresh_coalesced={}",
            s.refused,
            s.merged,
            s.rearmed,
            s.superseded,
            self.blockhash_parking.len(),
            s.refresh_requests,
            s.refresh_coalesced
        )
    }

    fn emit_park(
        &mut self,
        event: BlockhashParkEvent,
        p: &Parked,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.evidence
            .emit(OwnerEvidence::Record(RecordEvent::BlockhashPark {
                event,
                entry: p.entry(),
                cache_generation: self.blockhash.generation(),
            }))?;
        Ok(())
    }
}
