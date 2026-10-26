//! Regressions of the C2 chain-state owner.
//!
//! Every test names the invariant it proves and its source. Sources:
//! retirement contract `bsc/docs/2_REGELN/BSC_PHASE_C_PYTHON_RETIREMENT.md`
//! §3 (the Pass-6 Python Phase-C counterexamples, re-created here as native
//! fixtures; the Python tests themselves are not in the repository), master
//! plan `bsc/docs/1_PLAN/BSC_PRODUCTION_BUILD_MASTER_PLAN.md` §7 and §12,
//! PHASENPLAN §10, and Phase-F capture contract §5 ("Never overwrite
//! original first-seen with enrichment time").

use alloy_primitives::{Address, B256};

use super::*;
use crate::chain_state::canonical::ExecutionStatus;
use crate::chain_state::input::{
    CanonicalBlock, CanonicalTx, ChainInput, FirstSeen, PendingObservation,
};

const CHAIN: u64 = 56;
const BASE: u64 = 100;

fn own() -> Address {
    Address::repeat_byte(0x11)
}

fn own2() -> Address {
    Address::repeat_byte(0x12)
}

fn foreign() -> Address {
    Address::repeat_byte(0x21)
}

fn creator() -> Address {
    Address::repeat_byte(0x22)
}

fn t(id: u64) -> B256 {
    let mut b = [0u8; 32];
    b[0] = 0x7a;
    b[24..].copy_from_slice(&id.to_be_bytes());
    B256::from(b)
}

fn bh(number: u64, fork: u8) -> B256 {
    let mut b = [0u8; 32];
    b[0] = 0xb0;
    b[1] = fork;
    b[24..].copy_from_slice(&number.to_be_bytes());
    B256::from(b)
}

fn limits() -> StateLimits {
    StateLimits {
        max_txs: 64,
        max_candidates_per_nonce: 3,
        max_watched_senders: 2,
        reorg_window: 6,
        finality_depth: 3,
    }
}

fn state_with(limits: StateLimits) -> ChainState {
    ChainState::new(CHAIN, limits, &[own()]).unwrap()
}

fn state() -> ChainState {
    state_with(limits())
}

fn pending(hash: B256, sender: Address, nonce: u64, source: u8, at_ns: u64) -> ChainInput {
    ChainInput::Pending(PendingObservation {
        hash,
        sender,
        nonce,
        first_seen: FirstSeen { source, at_ns },
    })
}

fn block_status(
    number: u64,
    fork: u8,
    parent_fork: u8,
    txs: &[(B256, Address, u64, ExecutionStatus)],
) -> ChainInput {
    ChainInput::Block(CanonicalBlock {
        number,
        hash: bh(number, fork),
        parent_hash: bh(number - 1, parent_fork),
        timestamp_ms: number * 3_000,
        txs: txs
            .iter()
            .enumerate()
            .map(|(i, (hash, sender, nonce, status))| CanonicalTx {
                hash: *hash,
                index: i as u32,
                sender: *sender,
                nonce: *nonce,
                status: *status,
            })
            .collect(),
    })
}

fn block(number: u64, fork: u8, parent_fork: u8, txs: &[(B256, Address, u64)]) -> ChainInput {
    let txs: Vec<_> = txs
        .iter()
        .map(|(h, s, n)| (*h, *s, *n, ExecutionStatus::Succeeded))
        .collect();
    block_status(number, fork, parent_fork, &txs)
}

fn fact(sender: Address, next_nonce: u64, number: u64, fork: u8) -> ChainInput {
    ChainInput::NonceFact {
        sender,
        next_nonce,
        block_number: number,
        block_hash: bh(number, fork),
    }
}

fn accepted(out: &Applied) {
    assert!(
        out.conflicts.iter().all(|c| !c.refused()),
        "refused: {:?}",
        out.conflicts
    );
}

/// Anchored at block `BASE` (fork 0) with the own sender reconciled to
/// `next`.
fn reconciled(next: u64) -> ChainState {
    let mut s = state();
    accepted(&s.apply(&block(BASE, 0, 0, &[])));
    let out = s.apply(&fact(own(), next, BASE, 0));
    assert_eq!(
        out.reconciliations,
        vec![ReconciliationProof::for_tests(CHAIN, own(), next)]
    );
    s
}

fn inclusion(nonce: u64, tx: B256, block: B256) -> InclusionProof {
    InclusionProof::for_tests(CHAIN, own(), nonce, tx, block, ExecutionStatus::Succeeded)
}

/// Structural invariants that must hold after every input.
fn check_invariants(s: &ChainState) {
    assert!(s.txs.len() <= s.limits.max_txs);
    assert!(len_u64(s.blocks.len()) <= s.limits.reorg_window);
    let mut refs: BTreeMap<Address, usize> = BTreeMap::new();
    for (hash, record) in &s.txs {
        assert!(s.slot(record.sender, record.nonce).contains(hash));
        *refs.entry(record.sender).or_default() += 1;
    }
    for ((sender, nonce), slot) in &s.slots {
        assert!(!slot.is_empty());
        assert!(slot.len() <= s.limits.max_candidates_per_nonce + 1);
        let included = slot
            .iter()
            .filter(|h| matches!(s.txs[*h].state, TxState::Included { .. }))
            .count();
        assert!(included <= 1, "two inclusions of {sender}/{nonce}");
        for h in slot {
            assert_eq!((s.txs[h].sender, s.txs[h].nonce), (*sender, *nonce));
        }
    }
    for (sender, record) in &s.senders {
        assert_eq!(record.refs, refs.get(sender).copied().unwrap_or(0));
        assert!(record.watched || record.refs > 0);
        if record.confidence == Confidence::Clean {
            assert!(record.implied.is_some());
        }
    }
    if let Some(last) = s.blocks.back() {
        assert_eq!(s.head, Some((last.number, last.hash)));
    }
    for pair in s.blocks.iter().zip(s.blocks.iter().skip(1)) {
        assert_eq!(pair.0.number + 1, pair.1.number);
        assert_eq!(pair.0.hash, pair.1.parent_hash);
    }
    for block in &s.blocks {
        for tx in block.txs.iter().filter(|t| t.recorded) {
            assert!(s.txs.contains_key(&tx.hash), "retained tx evicted");
        }
    }
}

// ---------------------------------------------------------------------
// First-seen
// ---------------------------------------------------------------------

/// Retirement §3 "exact earliest-first-seen semantics"; master plan §12
/// "First-seen is preserved; later full/enrichment never overwrites it";
/// Phase-F §5. A later or equally timed sighting from another source never
/// replaces the first-seen.
#[test]
fn earliest_first_seen_is_kept_and_a_later_sighting_never_overwrites_it() {
    let mut s = state();
    s.apply(&pending(t(1), foreign(), 0, 1, 100));
    s.apply(&pending(t(1), foreign(), 0, 2, 150));
    s.apply(&pending(t(1), foreign(), 0, 3, 100));
    let view = s.tx(&t(1)).unwrap();
    assert_eq!(
        view.first_seen,
        Some(FirstSeen {
            source: 1,
            at_ns: 100
        })
    );
    assert_eq!(view.sightings, 3);
    assert_eq!(s.counters().repeat_sightings, 2);
    assert_eq!(s.counters().earlier_first_seen, 0);
}

/// Retirement §3 "exact earliest-first-seen semantics": the first-seen is
/// the earliest time, so a sighting applied later but observed earlier (from
/// another lane) is the first-seen; it is counted.
#[test]
fn earlier_sighting_applied_later_is_the_first_seen() {
    let mut s = state();
    s.apply(&pending(t(1), foreign(), 0, 1, 100));
    s.apply(&pending(t(1), foreign(), 0, 4, 90));
    assert_eq!(
        s.tx(&t(1)).unwrap().first_seen,
        Some(FirstSeen {
            source: 4,
            at_ns: 90
        })
    );
    assert_eq!(s.counters().earlier_first_seen, 1);
}

// ---------------------------------------------------------------------
// Candidates and replacement
// ---------------------------------------------------------------------

/// Retirement §3 "same-sender+nonce candidate competition"; master plan
/// §12 "Multiple sender/nonce candidates are tracked".
#[test]
fn several_candidates_per_sender_nonce_are_all_kept() {
    let mut s = state();
    for (i, id) in [1u64, 2, 3].iter().enumerate() {
        s.apply(&pending(t(*id), foreign(), 5, 1, 100 + i as u64));
    }
    let view = s.nonce(foreign(), 5);
    assert_eq!(view.candidates, vec![t(1), t(2), t(3)]);
    assert!(view.replaced.is_empty());
    assert_eq!(view.consumption, Consumption::Unknown);
    assert_eq!(s.counters().competing_candidates, 2);
}

/// Retirement §3 "observation order does not prove replacement"; master
/// plan §12 "observation order alone does not prove replacement". A later
/// candidate does not replace an earlier one, and only the canonical
/// inclusion decides the winner, whichever was seen first.
#[test]
fn observation_order_alone_never_marks_a_candidate_replaced() {
    for winner in [t(1), t(2)] {
        let mut s = reconciled(0);
        let a = s.apply(&pending(t(1), own(), 0, 1, 100));
        let b = s.apply(&pending(t(2), own(), 0, 1, 200));
        assert!(a.replaced.is_empty() && b.replaced.is_empty());
        assert_eq!(s.tx(&t(1)).unwrap().state, TxState::Candidate);
        assert_eq!(s.tx(&t(2)).unwrap().state, TxState::Candidate);

        let loser = if winner == t(1) { t(2) } else { t(1) };
        let out = s.apply(&block(BASE + 1, 0, 0, &[(winner, own(), 0)]));
        assert_eq!(
            out.replaced,
            vec![Replaced {
                sender: own(),
                nonce: 0,
                loser,
                winner
            }]
        );
        assert_eq!(out.inclusions, vec![inclusion(0, winner, bh(BASE + 1, 0))]);
        assert_eq!(s.tx(&loser).unwrap().state, TxState::Replaced { winner });
        check_invariants(&s);
    }
}

/// Retirement §3 "canonical path independent from pending visibility";
/// master plan §12 "Canonical state exists independently of pending
/// observations".
#[test]
fn canonical_effects_exist_without_any_pending_observation() {
    let mut s = reconciled(0);
    let out = s.apply(&block(
        BASE + 1,
        0,
        0,
        &[(t(9), foreign(), 7), (t(10), own(), 0)],
    ));
    accepted(&out);
    let view = s.tx(&t(9)).unwrap();
    assert_eq!(view.first_seen, None);
    assert!(
        matches!(view.state, TxState::Included { block_number, .. } if block_number == BASE + 1)
    );
    assert_eq!(out.inclusions, vec![inclusion(0, t(10), bh(BASE + 1, 0))]);
    assert_eq!(s.sender(foreign()), SenderView::Clean { next_nonce: 8 });
    assert_eq!(s.sender(own()), SenderView::Clean { next_nonce: 1 });
}

/// Retirement §3 "late-pending metadata must never rewrite final canonical
/// resolution"; master plan §12 "late pending cannot rewrite inclusion". A
/// late sighting is recorded as late only; a late competitor of a consumed
/// nonce is a loser at once.
#[test]
fn late_pending_after_inclusion_never_rewrites_the_inclusion() {
    let mut s = reconciled(0);
    s.apply(&block(BASE + 1, 0, 0, &[(t(1), own(), 0)]));
    let before = s.tx(&t(1)).unwrap();

    let out = s.apply(&pending(t(1), own(), 0, 2, 5));
    assert_eq!(out, Applied::default());
    let after = s.tx(&t(1)).unwrap();
    assert_eq!(after.state, before.state);
    assert_eq!(after.first_seen, None);
    assert_eq!(after.late_sightings, 1);
    assert_eq!(s.counters().late_sightings, 1);

    let out = s.apply(&pending(t(2), own(), 0, 2, 6));
    assert_eq!(
        out.replaced,
        vec![Replaced {
            sender: own(),
            nonce: 0,
            loser: t(2),
            winner: t(1)
        }]
    );
    assert_eq!(s.tx(&t(1)).unwrap().state, before.state);
    assert!(out.inclusions.is_empty());
}

/// PHASENPLAN §10 "included/reverted": a reverted transaction consumes its
/// nonce like a successful one, and its competitors lose.
#[test]
fn reverted_tx_consumes_the_nonce() {
    let mut s = reconciled(0);
    s.apply(&pending(t(2), own(), 0, 1, 10));
    let out = s.apply(&block_status(
        BASE + 1,
        0,
        0,
        &[(t(1), own(), 0, ExecutionStatus::Reverted)],
    ));
    assert_eq!(
        out.inclusions,
        vec![InclusionProof::for_tests(
            CHAIN,
            own(),
            0,
            t(1),
            bh(BASE + 1, 0),
            ExecutionStatus::Reverted
        )]
    );
    assert_eq!(out.replaced.len(), 1);
    assert_eq!(s.sender(own()), SenderView::Clean { next_nonce: 1 });
    assert!(matches!(
        s.nonce(own(), 0).consumption,
        Consumption::By {
            status: ExecutionStatus::Reverted,
            ..
        }
    ));
    assert_eq!(s.counters().reverted, 1);
}

/// Proofs and replacement records only for watched senders (own accounts);
/// foreign senders are state and counters only.
#[test]
fn proofs_and_replacements_only_for_watched_senders() {
    let mut s = reconciled(0);
    s.apply(&pending(t(1), foreign(), 0, 1, 1));
    s.apply(&pending(t(2), foreign(), 0, 1, 2));
    let out = s.apply(&block(BASE + 1, 0, 0, &[(t(1), foreign(), 0)]));
    assert_eq!(out, Applied::default());
    assert_eq!(s.counters().replaced, 1);
    for n in 2..=4 {
        let out = s.apply(&block(BASE + n, 0, 0, &[]));
        assert!(out.finalities.is_empty());
    }
    assert!(matches!(
        s.tx(&t(1)).unwrap().state,
        TxState::Included {
            finalized: true,
            ..
        }
    ));
    let out = s.apply(&fact(foreign(), 1, BASE + 4, 0));
    assert!(out.reconciliations.is_empty());
    assert_eq!(s.counters().ignored_nonce_facts, 1);
}

// ---------------------------------------------------------------------
// Canonical conflicts
// ---------------------------------------------------------------------

/// Master plan §12 / order C2: a transaction hash included in two
/// canonical blocks without an orphaning reorg is a conflict; nothing is
/// overwritten and no snapshot is reported until an accepted canonical
/// input follows.
#[test]
fn tx_included_in_two_blocks_without_reorg_is_a_conflict() {
    let mut s = reconciled(0);
    s.apply(&pending(t(50), creator(), 0, 1, 1));
    s.apply(&block(BASE + 1, 0, 0, &[(t(1), foreign(), 0)]));
    assert!(s.snapshot_id(t(50), 1, 1).is_some());

    let out = s.apply(&block(BASE + 2, 0, 0, &[(t(1), foreign(), 0)]));
    assert_eq!(
        out.conflicts,
        vec![Conflict::DoubleInclusion {
            tx: t(1),
            included_in: bh(BASE + 1, 0),
            block_hash: bh(BASE + 2, 0)
        }]
    );
    assert_eq!(s.high_water(), Some((BASE + 1, bh(BASE + 1, 0))));
    assert!(matches!(
        s.tx(&t(1)).unwrap().state,
        TxState::Included { block_hash, .. } if block_hash == bh(BASE + 1, 0)
    ));
    assert_eq!(s.snapshot_id(t(50), 1, 1), None);

    accepted(&s.apply(&block(BASE + 2, 1, 0, &[])));
    assert!(s.snapshot_id(t(50), 1, 1).is_some());
    check_invariants(&s);
}

/// A nonce already consumed on the canonical chain cannot be consumed
/// again by another transaction without a reorg.
#[test]
fn consumed_nonce_consumed_again_is_a_conflict() {
    let mut s = reconciled(0);
    s.apply(&block(BASE + 1, 0, 0, &[(t(1), foreign(), 0)]));
    let out = s.apply(&block(BASE + 2, 0, 0, &[(t(2), foreign(), 0)]));
    assert_eq!(
        out.conflicts,
        vec![Conflict::NonceConsumedTwice {
            sender: foreign(),
            nonce: 0,
            consumed_by: t(1),
            tx: t(2)
        }]
    );
    assert_eq!(s.tx(&t(2)), None);
}

/// Order C2: a block whose parent is not the head is refused as a conflict;
/// the follower must send the reorg first.
#[test]
fn block_whose_parent_is_not_the_head_is_refused_as_conflict() {
    let mut s = reconciled(0);
    let before = s.clone();
    let skip = s.apply(&block(BASE + 2, 0, 0, &[(t(1), own(), 0)]));
    let fork = s.apply(&block(BASE + 1, 1, 5, &[(t(1), own(), 0)]));
    for out in [&skip, &fork] {
        assert!(matches!(
            out.conflicts.as_slice(),
            [Conflict::NotOnHead { .. }]
        ));
        assert!(out.inclusions.is_empty());
    }
    assert_eq!(s.txs, before.txs);
    assert_eq!(s.blocks, before.blocks);
    assert_eq!(s.senders, before.senders);
    assert_eq!(s.counters().refused_inputs, 2);
}

/// A block's transaction list must be complete and consistent; otherwise
/// it is refused (retirement §3 "receipt completeness", "no silent
/// evidence loss").
#[test]
fn malformed_and_oversize_blocks_are_refused() {
    let mut s = reconciled(0);
    let mut gap = block(BASE + 1, 0, 0, &[(t(1), own(), 0), (t(2), foreign(), 0)]);
    if let ChainInput::Block(b) = &mut gap {
        b.txs[1].index = 2;
    }
    let repeated = block(BASE + 1, 0, 0, &[(t(1), own(), 0), (t(1), own(), 0)]);
    let skipped = block(
        BASE + 1,
        0,
        0,
        &[(t(1), foreign(), 0), (t(2), foreign(), 2)],
    );
    for input in [gap, repeated, skipped] {
        let out = s.apply(&input);
        assert!(matches!(
            out.conflicts.as_slice(),
            [Conflict::MalformedBlock { .. }]
        ));
    }

    let mut small = StateLimits {
        max_txs: 1,
        max_candidates_per_nonce: 1,
        ..limits()
    };
    small.max_watched_senders = 1;
    let mut s = state_with(small);
    let out = s.apply(&block(
        BASE,
        0,
        0,
        &[(t(1), own(), 0), (t(2), foreign(), 0)],
    ));
    assert!(matches!(
        out.conflicts.as_slice(),
        [Conflict::OversizeBlock { txs: 2, .. }]
    ));
    assert_eq!(s.high_water(), None);
}

/// Immutable evidence identity (retirement §3): a hash reported with another
/// sender or nonce is a conflict and changes nothing.
#[test]
fn identity_mismatch_is_a_conflict_and_changes_nothing() {
    let mut s = reconciled(0);
    s.apply(&pending(t(1), foreign(), 0, 1, 1));
    let out = s.apply(&pending(t(1), foreign(), 1, 1, 2));
    assert_eq!(out.conflicts, vec![Conflict::IdentityMismatch { tx: t(1) }]);
    let out = s.apply(&block(BASE + 1, 0, 0, &[(t(1), creator(), 0)]));
    assert_eq!(out.conflicts, vec![Conflict::IdentityMismatch { tx: t(1) }]);
    let view = s.tx(&t(1)).unwrap();
    assert_eq!((view.sender, view.nonce, view.sightings), (foreign(), 0, 1));
}

/// A canonical inclusion that skips the known next nonce is reported (the
/// block is chain truth and is applied); a watched sender then needs a
/// nonce fact, a foreign one is fixed by the inclusion itself.
#[test]
fn nonce_discontinuity_is_reported_and_makes_a_watched_sender_unknown() {
    let mut s = reconciled(0);
    let out = s.apply(&block(BASE + 1, 0, 0, &[(t(1), own(), 2)]));
    let expected = Conflict::NonceDiscontinuity {
        sender: own(),
        expected: 0,
        observed: 2,
    };
    assert_eq!(out.conflicts, vec![expected]);
    assert!(!expected.refused());
    assert_eq!(out.inclusions, vec![inclusion(2, t(1), bh(BASE + 1, 0))]);
    assert_eq!(
        s.sender(own()),
        SenderView::Unknown {
            reason: SenderReason::NonceDiscontinuity {
                expected: 0,
                observed: 2
            }
        }
    );

    s.apply(&block(BASE + 2, 0, 0, &[(t(2), foreign(), 3)]));
    let out = s.apply(&block(BASE + 3, 0, 0, &[(t(3), foreign(), 6)]));
    assert_eq!(out.conflicts.len(), 1);
    assert_eq!(s.sender(foreign()), SenderView::Clean { next_nonce: 7 });
}

// ---------------------------------------------------------------------
// Reorgs, gaps, uncertainty
// ---------------------------------------------------------------------

/// Retirement §3 "reorg/orphan/uncertain handling"; master plan §12
/// "Bounded reorg ancestry", "dropped/replaced/orphaned outcomes are
/// explicit". Orphaned transactions and the candidates they replaced return
/// to candidates, nothing is dropped, watched nonces get one orphan proof
/// each, and every sender falls back to its earlier nonce.
#[test]
fn reorg_returns_orphaned_txs_to_candidates_and_proves_orphans() {
    let mut s = reconciled(0);
    s.apply(&pending(t(1), own(), 0, 1, 10));
    s.apply(&pending(t(11), foreign(), 3, 1, 11));
    s.apply(&pending(t(12), foreign(), 3, 1, 12));
    accepted(&s.apply(&block(
        BASE + 1,
        0,
        0,
        &[(t(1), own(), 0), (t(11), foreign(), 3)],
    )));
    accepted(&s.apply(&block(BASE + 2, 0, 0, &[(t(2), own(), 1)])));
    assert_eq!(s.sender(own()), SenderView::Clean { next_nonce: 2 });
    assert_eq!(s.sender(foreign()), SenderView::Clean { next_nonce: 4 });
    assert_eq!(
        s.tx(&t(12)).unwrap().state,
        TxState::Replaced { winner: t(11) }
    );

    let out = s.apply(&ChainInput::Reorg {
        orphaned: vec![(BASE + 1, bh(BASE + 1, 0)), (BASE + 2, bh(BASE + 2, 0))],
        new_head: (BASE + 2, bh(BASE + 2, 1)),
    });
    accepted(&out);
    assert_eq!(
        out.orphans,
        vec![
            OrphanProof::for_tests(CHAIN, own(), 0, bh(BASE + 1, 0)),
            OrphanProof::for_tests(CHAIN, own(), 1, bh(BASE + 2, 0)),
        ]
    );
    for id in [1, 2, 11, 12] {
        assert_eq!(s.tx(&t(id)).unwrap().state, TxState::Candidate, "tx {id}");
    }
    assert_eq!(s.tx(&t(1)).unwrap().orphaned, 1);
    assert_eq!(s.sender(own()), SenderView::Clean { next_nonce: 0 });
    assert_eq!(s.sender(foreign()), SenderView::Clean { next_nonce: 3 });
    assert_eq!(s.high_water(), Some((BASE, bh(BASE, 0))));
    assert_eq!(s.counters().held_txs, 4);

    // The new chain decides anew.
    let out = s.apply(&block(BASE + 1, 1, 0, &[(t(12), foreign(), 3)]));
    accepted(&out);
    assert_eq!(
        s.tx(&t(11)).unwrap().state,
        TxState::Replaced { winner: t(12) }
    );
    let out = s.apply(&block(BASE + 2, 1, 1, &[(t(1), own(), 0)]));
    assert_eq!(out.inclusions, vec![inclusion(0, t(1), bh(BASE + 2, 1))]);
    assert_eq!(s.tx(&t(2)).unwrap().state, TxState::Candidate);
    check_invariants(&s);
}

/// A reorg must name exactly a suffix of the retained chain ending at the
/// head; anything else is refused and changes nothing.
#[test]
fn reorg_not_matching_the_retained_chain_is_refused() {
    let mut s = reconciled(0);
    s.apply(&block(BASE + 1, 0, 0, &[(t(1), own(), 0)]));
    s.apply(&block(BASE + 2, 0, 0, &[]));
    let before = s.clone();
    let wrong_hash = ChainInput::Reorg {
        orphaned: vec![(BASE + 1, bh(BASE + 1, 9)), (BASE + 2, bh(BASE + 2, 0))],
        new_head: (BASE + 2, bh(BASE + 2, 1)),
    };
    let not_head = ChainInput::Reorg {
        orphaned: vec![(BASE + 1, bh(BASE + 1, 0))],
        new_head: (BASE + 2, bh(BASE + 2, 1)),
    };
    let not_contiguous = ChainInput::Reorg {
        orphaned: vec![(BASE, bh(BASE, 0)), (BASE + 2, bh(BASE + 2, 0))],
        new_head: (BASE + 2, bh(BASE + 2, 1)),
    };
    let empty = ChainInput::Reorg {
        orphaned: Vec::new(),
        new_head: (BASE + 2, bh(BASE + 2, 1)),
    };
    for input in [wrong_hash, not_head, not_contiguous, empty] {
        let out = s.apply(&input);
        assert_eq!(out.conflicts, vec![Conflict::ReorgMismatch]);
        assert!(out.orphans.is_empty());
    }
    assert_eq!(s.blocks, before.blocks);
    assert_eq!(s.txs, before.txs);
}

/// Master plan §12 "Bounded reorg ancestry": a reorg reaching below the
/// retained window cannot be undone exactly and is handled as too deep.
#[test]
fn reorg_below_the_retained_window_is_handled_as_too_deep() {
    let mut s = reconciled(0);
    for n in 1..=10 {
        accepted(&s.apply(&block(BASE + n, 0, 0, &[])));
    }
    let orphaned: Vec<_> = (BASE + 3..=BASE + 10).map(|n| (n, bh(n, 0))).collect();
    let out = s.apply(&ChainInput::Reorg {
        orphaned,
        new_head: (BASE + 10, bh(BASE + 10, 1)),
    });
    accepted(&out);
    let c = s.counters();
    assert_eq!((c.reorgs, c.reorgs_beyond_window), (0, 1));
    assert_eq!(c.unmatched_orphaned_blocks, 2);
    assert_eq!(s.high_water(), None);
    assert_eq!(
        s.sender(own()),
        SenderView::Unknown {
            reason: SenderReason::ReorgTooDeep
        }
    );
    // The follower continues with the new chain right after the fork.
    let out = s.apply(&block(BASE + 3, 1, 1, &[]));
    accepted(&out);
}

/// Retirement §3 "restart/backfill/high-water semantics"; master plan §12
/// "Gaps ... are explicit"; "Unknown state disables affected entries until
/// reconciliation". A reorg too deep orphans the listed blocks, makes every
/// sender unknown and the snapshot unknown until the watched senders are
/// reconciled.
#[test]
fn reorg_too_deep_makes_every_sender_and_the_snapshot_unknown_until_reconciled() {
    let mut s = reconciled(0);
    s.apply(&pending(t(50), creator(), 0, 1, 1));
    s.apply(&block(BASE + 1, 0, 0, &[(t(1), own(), 0)]));
    assert!(s.snapshot_id(t(50), 1, 1).is_some());

    let out = s.apply(&ChainInput::ReorgTooDeep {
        orphaned: vec![(BASE + 1, bh(BASE + 1, 0))],
        new_head: (BASE + 5, bh(BASE + 5, 1)),
    });
    assert_eq!(
        out.orphans,
        vec![OrphanProof::for_tests(CHAIN, own(), 0, bh(BASE + 1, 0))]
    );
    for sender in [own(), creator()] {
        assert_eq!(
            s.sender(sender),
            SenderView::Unknown {
                reason: SenderReason::ReorgTooDeep
            }
        );
    }
    assert_eq!(s.high_water(), None);
    assert_eq!(s.snapshot_id(t(50), 1, 1), None);

    let out = s.apply(&block(BASE + 4, 1, 1, &[]));
    assert!(matches!(
        out.conflicts.as_slice(),
        [Conflict::AnchorMismatch { .. }]
    ));
    accepted(&s.apply(&block(BASE + 5, 1, 7, &[])));
    assert_eq!(s.snapshot_id(t(50), 1, 1), None);

    // t(1) is a candidate again: reconciliation must wait for it.
    s.apply(&fact(own(), 0, BASE + 5, 1));
    assert_eq!(
        s.sender(own()),
        SenderView::Unknown {
            reason: SenderReason::UnresolvedCandidate { nonce: 0 }
        }
    );
    let out = s.apply(&block(BASE + 6, 1, 1, &[(t(1), own(), 0)]));
    assert_eq!(out.inclusions, vec![inclusion(0, t(1), bh(BASE + 6, 1))]);
    let out = s.apply(&fact(own(), 1, BASE + 6, 1));
    assert_eq!(
        out.reconciliations,
        vec![ReconciliationProof::for_tests(CHAIN, own(), 1)]
    );
    assert!(s.snapshot_id(t(50), 1, 1).is_some());
    check_invariants(&s);
}

/// Master plan §12 "Gaps, backfill and dropped/replaced/orphaned outcomes
/// are explicit"; PHASENPLAN §10 "canonical/orphan/uncertain". A gap makes
/// the not yet final transactions of the retained chain uncertain and every
/// sender unknown; a gap must start right after the head.
#[test]
fn gap_makes_every_sender_unknown_and_non_final_txs_uncertain() {
    let mut s = reconciled(0);
    s.apply(&block(BASE + 1, 0, 0, &[(t(1), own(), 0)]));
    let out = s.apply(&ChainInput::Gap {
        from: BASE + 3,
        to: BASE + 5,
    });
    assert_eq!(
        out.conflicts,
        vec![Conflict::GapMismatch {
            from: BASE + 3,
            to: BASE + 5
        }]
    );

    let out = s.apply(&ChainInput::Gap {
        from: BASE + 2,
        to: BASE + 5,
    });
    accepted(&out);
    assert!(out.orphans.is_empty());
    assert!(matches!(
        s.tx(&t(1)).unwrap().state,
        TxState::Uncertain { block_number, .. } if block_number == BASE + 1
    ));
    assert!(matches!(
        s.nonce(own(), 0).consumption,
        Consumption::Uncertain { .. }
    ));
    assert_eq!(
        s.sender(own()),
        SenderView::Unknown {
            reason: SenderReason::Gap
        }
    );
    assert_eq!(s.high_water(), None);
    assert_eq!(s.counters().held_uncertain, 1);
    let out = s.apply(&block(BASE + 7, 0, 0, &[]));
    assert!(matches!(
        out.conflicts.as_slice(),
        [Conflict::AnchorMismatch { .. }]
    ));
    accepted(&s.apply(&block(BASE + 6, 0, 3, &[])));
}

/// An uncertain transaction that a canonical block includes again proves
/// that its old block left the chain: orphan proof first, then inclusion.
#[test]
fn uncertain_tx_included_again_is_orphaned_then_included() {
    let mut s = reconciled(0);
    s.apply(&block(BASE + 1, 0, 0, &[(t(1), own(), 0)]));
    s.apply(&ChainInput::Gap {
        from: BASE + 2,
        to: BASE + 2,
    });
    s.apply(&block(BASE + 3, 1, 1, &[]));
    let out = s.apply(&block(BASE + 4, 1, 1, &[(t(1), own(), 0)]));
    assert_eq!(
        out.orphans,
        vec![OrphanProof::for_tests(CHAIN, own(), 0, bh(BASE + 1, 0))]
    );
    assert_eq!(out.inclusions, vec![inclusion(0, t(1), bh(BASE + 4, 1))]);
    assert_eq!(s.tx(&t(1)).unwrap().orphaned, 1);
}

/// An uncertain transaction loses its nonce to another canonical
/// transaction: orphan proof for its block, inclusion of the winner, and
/// the uncertain one is replaced.
#[test]
fn uncertain_tx_replaced_by_another_canonical_tx() {
    let mut s = reconciled(0);
    s.apply(&block(BASE + 1, 0, 0, &[(t(1), own(), 0)]));
    s.apply(&ChainInput::Gap {
        from: BASE + 2,
        to: BASE + 2,
    });
    s.apply(&block(BASE + 3, 1, 1, &[]));
    let out = s.apply(&block(BASE + 4, 1, 1, &[(t(2), own(), 0)]));
    assert_eq!(
        out.orphans,
        vec![OrphanProof::for_tests(CHAIN, own(), 0, bh(BASE + 1, 0))]
    );
    assert_eq!(out.inclusions, vec![inclusion(0, t(2), bh(BASE + 4, 1))]);
    assert_eq!(
        out.replaced,
        vec![Replaced {
            sender: own(),
            nonce: 0,
            loser: t(1),
            winner: t(2)
        }]
    );
}

/// A nonce fact at a block at or above an uncertain transaction's block
/// that shows its nonce unconsumed proves that block left the chain.
#[test]
fn nonce_fact_proves_an_uncertain_tx_was_orphaned() {
    let mut s = reconciled(0);
    s.apply(&block(BASE + 1, 0, 0, &[(t(1), own(), 0)]));
    s.apply(&ChainInput::Gap {
        from: BASE + 2,
        to: BASE + 3,
    });
    s.apply(&block(BASE + 4, 1, 1, &[]));

    // A count of 1 does not say which transaction consumed nonce 0.
    let out = s.apply(&fact(own(), 1, BASE + 4, 1));
    assert!(out.orphans.is_empty() && out.reconciliations.is_empty());
    assert_eq!(
        s.sender(own()),
        SenderView::Unknown {
            reason: SenderReason::UnresolvedCandidate { nonce: 0 }
        }
    );

    let out = s.apply(&fact(own(), 0, BASE + 4, 1));
    assert_eq!(
        out.orphans,
        vec![OrphanProof::for_tests(CHAIN, own(), 0, bh(BASE + 1, 0))]
    );
    assert!(out.reconciliations.is_empty());
    assert_eq!(s.tx(&t(1)).unwrap().state, TxState::Candidate);
}

// ---------------------------------------------------------------------
// Finality
// ---------------------------------------------------------------------

/// Order C2: finality after `finality_depth` confirmations on the
/// canonical chain is proven once.
#[test]
fn finality_after_depth_confirmations_is_proven_once() {
    let mut s = reconciled(0);
    s.apply(&block(BASE + 1, 0, 0, &[(t(1), own(), 0)]));
    for n in 2..=3 {
        assert!(s.apply(&block(BASE + n, 0, 0, &[])).finalities.is_empty());
    }
    let out = s.apply(&block(BASE + 4, 0, 0, &[]));
    assert_eq!(
        out.finalities,
        vec![FinalityProof::for_tests(CHAIN, own(), 0, bh(BASE + 1, 0))]
    );
    for n in 5..=12 {
        assert!(s.apply(&block(BASE + n, 0, 0, &[])).finalities.is_empty());
    }
    assert!(matches!(
        s.nonce(own(), 0).consumption,
        Consumption::By {
            finalized: true,
            ..
        }
    ));
}

/// A final block still inside the reorg window can be orphaned (the lane
/// accepts Canonical → Orphaned); after re-inclusion finality is proven
/// once more for the new block.
#[test]
fn orphaning_a_final_block_inside_the_window_proves_the_orphan() {
    let mut s = reconciled(0);
    s.apply(&block(BASE + 1, 0, 0, &[(t(1), own(), 0)]));
    for n in 2..=4 {
        s.apply(&block(BASE + n, 0, 0, &[]));
    }
    let orphaned: Vec<_> = (BASE + 1..=BASE + 4).map(|n| (n, bh(n, 0))).collect();
    let out = s.apply(&ChainInput::Reorg {
        orphaned,
        new_head: (BASE + 4, bh(BASE + 4, 1)),
    });
    assert_eq!(
        out.orphans,
        vec![OrphanProof::for_tests(CHAIN, own(), 0, bh(BASE + 1, 0))]
    );
    let out = s.apply(&block(BASE + 1, 1, 0, &[(t(1), own(), 0)]));
    assert_eq!(out.inclusions, vec![inclusion(0, t(1), bh(BASE + 1, 1))]);
    let mut finalities = Vec::new();
    for n in 2..=8 {
        finalities.extend(s.apply(&block(BASE + n, 1, 1, &[])).finalities);
    }
    assert_eq!(
        finalities,
        vec![FinalityProof::for_tests(CHAIN, own(), 0, bh(BASE + 1, 1))]
    );
}

// ---------------------------------------------------------------------
// Restart quarantine and reconciliation
// ---------------------------------------------------------------------

/// Master plan §7 "Restart remains fail-closed ... C2 may resume only after
/// a separately tested canonical/pending/nonce/intent reconciliation";
/// §12 "Restart quarantine precedes any controlled recovery". The state is
/// rebuilt from the journal, quarantined, and released per sender only by a
/// matching nonce fact.
#[test]
fn restart_quarantine_holds_until_a_matching_nonce_fact_reconciles() {
    let journal = vec![
        block(BASE, 0, 0, &[]),
        fact(own(), 0, BASE, 0),
        pending(t(1), own(), 0, 1, 10),
        block(BASE + 1, 0, 0, &[(t(1), own(), 0)]),
        block(BASE + 2, 0, 0, &[]),
    ];
    let mut s = state();
    for input in &journal {
        s.apply(&ChainInput::decode(&input.encode()).unwrap());
    }
    s.quarantine_all();
    assert_eq!(
        s.sender(own()),
        SenderView::Quarantined {
            reason: SenderReason::Restart
        }
    );
    s.apply(&pending(t(50), creator(), 0, 1, 1));
    assert_eq!(s.snapshot_id(t(50), 1, 1), None);

    let out = s.apply(&fact(own(), 1, BASE + 9, 0));
    assert!(out.reconciliations.is_empty());
    assert_eq!(
        s.sender(own()),
        SenderView::Quarantined {
            reason: SenderReason::NonceFactNotCanonical {
                block_number: BASE + 9,
                block_hash: bh(BASE + 9, 0)
            }
        }
    );

    let out = s.apply(&fact(own(), 0, BASE + 2, 0));
    assert!(out.reconciliations.is_empty());
    assert_eq!(
        s.sender(own()),
        SenderView::Quarantined {
            reason: SenderReason::NonceFactMismatch {
                fact_next: 0,
                implied_next: Some(1)
            }
        }
    );

    let out = s.apply(&fact(own(), 1, BASE + 2, 0));
    assert_eq!(
        out.reconciliations,
        vec![ReconciliationProof::for_tests(CHAIN, own(), 1)]
    );
    assert_eq!(s.sender(own()), SenderView::Clean { next_nonce: 1 });
    assert!(s.snapshot_id(t(50), 1, 1).is_some());
}

/// Order C2: a nonce fact reconciles only with no pending or unresolved
/// candidate of the sender.
#[test]
fn nonce_fact_with_a_pending_candidate_does_not_reconcile() {
    let mut s = reconciled(0);
    s.quarantine_all();
    s.apply(&pending(t(2), own(), 0, 1, 1));
    let out = s.apply(&fact(own(), 0, BASE, 0));
    assert!(out.reconciliations.is_empty());
    assert_eq!(
        s.sender(own()),
        SenderView::Quarantined {
            reason: SenderReason::UnresolvedCandidate { nonce: 0 }
        }
    );
    assert_eq!(s.counters().reconciliation_refusals, 1);

    // While running, a clean sender with an own pending transaction stays
    // clean, but gets no reconciliation proof.
    let mut s = reconciled(0);
    s.apply(&pending(t(2), own(), 0, 1, 1));
    let out = s.apply(&fact(own(), 0, BASE, 0));
    assert!(out.reconciliations.is_empty());
    assert_eq!(s.sender(own()), SenderView::Clean { next_nonce: 0 });
}

/// A fact at an older retained block accounts for the inclusions after it:
/// they must continue its nonce contiguously.
#[test]
fn nonce_fact_at_an_older_block_accounts_for_later_inclusions() {
    let mut s = reconciled(0);
    s.apply(&block(BASE + 1, 0, 0, &[(t(1), own(), 0)]));
    s.apply(&block(BASE + 2, 0, 0, &[(t(2), own(), 1)]));
    s.quarantine_all();
    let out = s.apply(&fact(own(), 0, BASE, 0));
    assert_eq!(
        out.reconciliations,
        vec![ReconciliationProof::for_tests(CHAIN, own(), 2)]
    );

    s.quarantine_all();
    s.apply(&fact(own(), 1, BASE, 0));
    assert_eq!(
        s.sender(own()),
        SenderView::Quarantined {
            reason: SenderReason::NonceFactMismatch {
                fact_next: 1,
                implied_next: Some(0)
            }
        }
    );
}

/// Master plan §12 "never authorize with speculative zero values": a
/// watched sender is unknown until its first nonce fact, also on a fresh
/// chain, and a fact needs a head.
#[test]
fn watched_sender_is_unknown_until_its_first_nonce_fact_never_zero() {
    let mut s = state();
    let unknown = SenderView::Unknown {
        reason: SenderReason::NotReconciled,
    };
    assert_eq!(s.sender(own()), unknown);
    assert_eq!(s.nonce(own(), 0).consumption, Consumption::Unknown);
    assert_eq!(
        s.sender(foreign()),
        SenderView::Unknown {
            reason: SenderReason::NotTracked
        }
    );

    let out = s.apply(&fact(own(), 0, BASE, 0));
    assert!(out.reconciliations.is_empty());
    assert_eq!(
        s.sender(own()),
        SenderView::Unknown {
            reason: SenderReason::ChainUnanchored
        }
    );

    s.apply(&block(BASE, 0, 0, &[]));
    s.apply(&pending(t(50), creator(), 0, 1, 1));
    assert_eq!(s.snapshot_id(t(50), 1, 1), None);
    let out = s.apply(&fact(own(), 0, BASE, 0));
    assert_eq!(out.reconciliations.len(), 1);
    assert_eq!(s.sender(own()), SenderView::Clean { next_nonce: 0 });
    assert_eq!(s.nonce(own(), 0).consumption, Consumption::Open);
}

// ---------------------------------------------------------------------
// Bounds
// ---------------------------------------------------------------------

/// Retirement §3 "bounded state and explicit overflow", "no silent evidence
/// loss": a full table with nothing settled refuses with a counted overflow
/// and makes the sender unknown.
#[test]
fn tx_table_overflow_is_counted_and_makes_the_sender_unknown() {
    let small = StateLimits {
        max_txs: 4,
        max_candidates_per_nonce: 2,
        ..limits()
    };
    let mut s = state_with(small);
    s.apply(&block(BASE, 0, 0, &[]));
    s.apply(&fact(own(), 0, BASE, 0));
    for i in 0..4u8 {
        let out = s.apply(&pending(
            t(u64::from(i)),
            Address::repeat_byte(0x30 + i),
            0,
            1,
            1,
        ));
        assert!(out.overflow.is_empty());
    }
    let out = s.apply(&pending(t(9), own(), 0, 1, 2));
    assert_eq!(
        out.overflow,
        vec![Overflow {
            kind: OverflowKind::Txs,
            sender: own(),
            nonce: 0,
            tx: t(9)
        }]
    );
    assert_eq!(s.tx(&t(9)), None);
    assert_eq!(
        s.sender(own()),
        SenderView::Unknown {
            reason: SenderReason::Overflow
        }
    );
    assert_eq!(s.counters().overflow_txs, 1);

    // A nonce fact cannot release it: the lost transaction may still run.
    let out = s.apply(&fact(own(), 0, BASE, 0));
    assert!(out.reconciliations.is_empty());
    assert_eq!(
        s.sender(own()),
        SenderView::Unknown {
            reason: SenderReason::Overflow
        }
    );
    check_invariants(&s);
}

/// Bounded state: settled records (final and out of the reorg window) are
/// evicted before anything overflows, in the order they settled.
#[test]
fn settled_records_are_evicted_before_overflow() {
    let small = StateLimits {
        max_txs: 3,
        max_candidates_per_nonce: 2,
        max_watched_senders: 1,
        reorg_window: 2,
        finality_depth: 1,
    };
    let mut s = state_with(small);
    s.apply(&block(BASE, 0, 0, &[]));
    s.apply(&fact(own(), 0, BASE, 0));
    let senders: Vec<Address> = (0..5u8).map(|i| Address::repeat_byte(0x40 + i)).collect();
    s.apply(&block(
        BASE + 1,
        0,
        0,
        &[
            (t(1), senders[0], 0),
            (t(2), senders[1], 0),
            (t(3), senders[2], 0),
        ],
    ));
    let out = s.apply(&pending(t(4), senders[3], 0, 1, 1));
    assert_eq!(out.overflow.len(), 1, "block 101 not settled yet");
    s.apply(&block(BASE + 2, 0, 0, &[]));
    s.apply(&block(BASE + 3, 0, 0, &[]));

    let out = s.apply(&pending(t(5), own(), 0, 1, 1));
    assert!(out.overflow.is_empty());
    assert_eq!(s.tx(&t(1)), None);
    assert_eq!(
        s.sender(senders[0]),
        SenderView::Unknown {
            reason: SenderReason::NotTracked
        }
    );
    s.apply(&pending(t(6), senders[3], 0, 1, 1));
    s.apply(&pending(t(7), senders[4], 0, 1, 1));
    assert_eq!(s.counters().evicted, 3);
    let out = s.apply(&pending(t(8), senders[4], 1, 1, 1));
    assert_eq!(out.overflow.len(), 1);
    check_invariants(&s);
}

/// Bounded candidates per nonce: the bound refuses with a counted overflow
/// and makes the sender unknown.
#[test]
fn candidates_per_nonce_overflow_is_counted_and_makes_the_sender_unknown() {
    let mut s = reconciled(0);
    for id in 1..=3 {
        assert!(
            s.apply(&pending(t(id), foreign(), 0, 1, id))
                .overflow
                .is_empty()
        );
    }
    let out = s.apply(&pending(t(4), foreign(), 0, 1, 4));
    assert_eq!(
        out.overflow,
        vec![Overflow {
            kind: OverflowKind::CandidatesPerNonce,
            sender: foreign(),
            nonce: 0,
            tx: t(4)
        }]
    );
    assert_eq!(
        s.sender(foreign()),
        SenderView::Unknown {
            reason: SenderReason::Overflow
        }
    );
    assert_eq!(s.counters().overflow_candidates, 1);
}

/// A watched sender whose transaction overflowed is reconciled only after
/// that nonce was consumed by a held inclusion.
#[test]
fn watched_overflow_blocks_reconciliation_until_the_lost_nonce_is_consumed() {
    let one = StateLimits {
        max_candidates_per_nonce: 1,
        ..limits()
    };
    let mut s = state_with(one);
    s.apply(&block(BASE, 0, 0, &[]));
    s.apply(&fact(own(), 0, BASE, 0));
    s.apply(&pending(t(1), own(), 0, 1, 1));
    let out = s.apply(&pending(t(2), own(), 0, 1, 2));
    assert_eq!(out.overflow.len(), 1);

    let out = s.apply(&block(BASE + 1, 0, 0, &[(t(2), own(), 0)]));
    assert_eq!(out.inclusions, vec![inclusion(0, t(2), bh(BASE + 1, 0))]);
    assert_eq!(
        s.tx(&t(1)).unwrap().state,
        TxState::Replaced { winner: t(2) }
    );
    let out = s.apply(&fact(own(), 1, BASE + 1, 0));
    assert_eq!(
        out.reconciliations,
        vec![ReconciliationProof::for_tests(CHAIN, own(), 1)]
    );
}

/// A new pending transaction below the sender's known next nonce whose
/// consumer the state does not hold is counted, not held.
#[test]
fn stale_pending_below_the_known_nonce_is_counted_not_held() {
    let mut s = reconciled(5);
    let out = s.apply(&pending(t(1), own(), 3, 1, 1));
    assert_eq!(out, Applied::default());
    assert_eq!(s.tx(&t(1)), None);
    assert_eq!(s.counters().stale_pending, 1);
    assert_eq!(s.nonce(own(), 3).consumption, Consumption::ByUntracked);
}

/// Every bound comes from the caller and is validated; there is no
/// default.
#[test]
fn limits_are_validated() {
    let ok = limits();
    assert_eq!(ok.validate(), Ok(()));
    let cases = [
        (StateLimits { max_txs: 0, ..ok }, LimitsError::ZeroMaxTxs),
        (
            StateLimits {
                max_candidates_per_nonce: 0,
                ..ok
            },
            LimitsError::ZeroMaxCandidatesPerNonce,
        ),
        (
            StateLimits {
                max_candidates_per_nonce: 65,
                ..ok
            },
            LimitsError::CandidatesPerNonceAboveMaxTxs,
        ),
        (
            StateLimits {
                max_watched_senders: 0,
                ..ok
            },
            LimitsError::ZeroMaxWatchedSenders,
        ),
        (
            StateLimits {
                reorg_window: 0,
                finality_depth: 0,
                ..ok
            },
            LimitsError::ZeroReorgWindow,
        ),
        (
            StateLimits {
                finality_depth: 0,
                ..ok
            },
            LimitsError::ZeroFinalityDepth,
        ),
        (
            StateLimits {
                finality_depth: 7,
                ..ok
            },
            LimitsError::FinalityDepthAboveReorgWindow,
        ),
    ];
    for (limits, error) in cases {
        assert_eq!(ChainState::new(CHAIN, limits, &[]), Err(error));
    }
    assert_eq!(
        ChainState::new(CHAIN, ok, &[own(), own2(), foreign()]),
        Err(LimitsError::TooManyWatchedSenders { count: 3, max: 2 })
    );
    assert_eq!(
        ChainState::new(CHAIN, ok, &[own(), own()]),
        Err(LimitsError::DuplicateWatchedSender(own()))
    );
}

// ---------------------------------------------------------------------
// Snapshot identity
// ---------------------------------------------------------------------

/// Snapshot identity (master plan §5; `SnapshotId` doc): a block, a reorg
/// and a competing candidate change it; an uncontested pending sighting
/// does not; a resolved candidate has none.
#[test]
fn snapshot_identity_changes_with_every_effect() {
    let mut s = reconciled(0);
    assert_eq!(s.snapshot_id(t(50), 3, 7), None);
    s.apply(&pending(t(50), creator(), 0, 1, 1));
    let first = s.snapshot_id(t(50), 3, 7).unwrap();
    assert_eq!(first.canonical_block_hash, bh(BASE, 0));
    assert_eq!((first.policy_generation, first.codehash_generation), (3, 7));

    s.apply(&pending(t(60), foreign(), 0, 1, 2));
    assert_eq!(s.snapshot_id(t(50), 3, 7), Some(first));

    s.apply(&pending(t(51), creator(), 0, 1, 3));
    let competed = s.snapshot_id(t(50), 3, 7).unwrap();
    assert_ne!(competed, first);

    s.apply(&block(BASE + 1, 0, 0, &[]));
    let next = s.snapshot_id(t(50), 3, 7).unwrap();
    assert_ne!(next, competed);
    assert_eq!(next.canonical_block_hash, bh(BASE + 1, 0));

    s.apply(&ChainInput::Reorg {
        orphaned: vec![(BASE + 1, bh(BASE + 1, 0))],
        new_head: (BASE + 1, bh(BASE + 1, 1)),
    });
    let reorged = s.snapshot_id(t(50), 3, 7).unwrap();
    assert_eq!(reorged.canonical_block_hash, bh(BASE, 0));
    assert_ne!(reorged, first);
    assert_ne!(reorged.ordered_state_cursor, first.ordered_state_cursor);

    s.apply(&block(BASE + 1, 1, 0, &[(t(51), creator(), 0)]));
    assert_eq!(s.snapshot_id(t(50), 3, 7), None);
}

/// `coherent` is the snapshot precondition: false after a gap and after a
/// refused block, true again after an accepted block (no watched senders,
/// so nothing needs reconciling).
#[test]
fn coherent_follows_the_head_and_refusals() {
    let mut s = ChainState::new(CHAIN, limits(), &[]).unwrap();
    assert!(!s.coherent());
    accepted(&s.apply(&block(BASE, 0, 0, &[])));
    assert!(s.coherent());
    s.apply(&pending(t(50), creator(), 0, 1, 1));
    assert!(s.snapshot_id(t(50), 1, 1).is_some());

    s.apply(&ChainInput::Gap {
        from: BASE + 1,
        to: BASE + 2,
    });
    assert!(!s.coherent());
    assert_eq!(s.snapshot_id(t(50), 1, 1), None);
    accepted(&s.apply(&block(BASE + 3, 0, 0, &[])));
    assert!(s.coherent());

    let refused = s.apply(&block(BASE + 4, 0, 5, &[]));
    assert!(matches!(
        refused.conflicts.as_slice(),
        [Conflict::NotOnHead { .. }]
    ));
    assert!(!s.coherent());
    assert_eq!(s.snapshot_id(t(50), 1, 1), None);
    accepted(&s.apply(&block(BASE + 4, 0, 0, &[])));
    assert!(s.coherent());
    assert!(s.snapshot_id(t(50), 1, 1).is_some());

    // A watched sender that is not clean makes the state incoherent.
    let mut w = state();
    accepted(&w.apply(&block(BASE, 0, 0, &[])));
    assert!(!w.coherent());
    w.apply(&fact(own(), 0, BASE, 0));
    assert!(w.coherent());
}

/// `retained` is the retained window, ascending, bounded by `reorg_window`.
#[test]
fn retained_is_the_window_ascending() {
    let mut s = reconciled(0);
    assert_eq!(s.retained(), vec![(BASE, bh(BASE, 0))]);
    for n in 1..=8 {
        accepted(&s.apply(&block(BASE + n, 0, 0, &[])));
    }
    let expected: Vec<_> = (BASE + 3..=BASE + 8).map(|n| (n, bh(n, 0))).collect();
    assert_eq!(s.retained(), expected);
    s.apply(&ChainInput::Reorg {
        orphaned: vec![(BASE + 8, bh(BASE + 8, 0))],
        new_head: (BASE + 8, bh(BASE + 8, 1)),
    });
    assert_eq!(s.retained(), expected[..5].to_vec());
    s.apply(&ChainInput::Gap {
        from: BASE + 8,
        to: BASE + 8,
    });
    assert!(s.retained().is_empty());
}

// ---------------------------------------------------------------------
// Capture/replay determinism
// ---------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const POOL: usize = 6;

fn pool() -> [Address; POOL] {
    [
        own(),
        own2(),
        foreign(),
        creator(),
        Address::repeat_byte(0x23),
        Address::repeat_byte(0x24),
    ]
}

/// Fixed identity of the generated transaction `id`: two hashes per
/// `(sender, nonce)` slot.
fn identity(id: u64) -> (Address, u64) {
    let pool = pool();
    (pool[(id % POOL as u64) as usize], (id / POOL as u64) % 8)
}

fn id_for(sender_index: usize, nonce: u64, variant: u64) -> u64 {
    sender_index as u64 + POOL as u64 * nonce + POOL as u64 * 8 * variant
}

fn det_limits() -> StateLimits {
    StateLimits {
        max_txs: 24,
        max_candidates_per_nonce: 2,
        max_watched_senders: 2,
        reorg_window: 4,
        finality_depth: 2,
    }
}

/// A random but well-formed-leaning journal, generated against a live
/// state (refused inputs are part of it too).
fn random_journal(seed: u64, steps: usize) -> Vec<ChainInput> {
    let mut rng = Rng(seed);
    let mut s = ChainState::new(CHAIN, det_limits(), &[own(), own2()]).unwrap();
    let mut journal = Vec::new();
    let mut chain: Vec<(u64, B256)> = Vec::new();
    let mut expected: Option<(u64, Option<B256>)> = Some((BASE, None));
    let mut fork: u8 = 0;
    for _ in 0..steps {
        let roll = rng.below(100);
        let input = if roll < 40 {
            let id = rng.below(2 * POOL as u64 * 8);
            let (sender, nonce) = identity(id);
            pending(t(id), sender, nonce, rng.below(4) as u8, rng.below(1_000))
        } else if roll < 75 {
            let (number, parent) = match (s.high_water(), expected) {
                (Some((n, h)), _) => (n + 1, h),
                (None, Some((n, _))) => (n, bh(n.saturating_sub(1), 0xee)),
                (None, None) => (BASE, bh(BASE - 1, 0)),
            };
            let hash = match expected {
                Some((n, Some(h))) if s.high_water().is_none() && n == number => h,
                _ => bh(number, fork),
            };
            let mut txs = Vec::new();
            let mut used = [false; POOL];
            for _ in 0..rng.below(4) {
                let si = rng.below(POOL as u64) as usize;
                if used[si] {
                    continue;
                }
                used[si] = true;
                let nonce = match s.sender(pool()[si]) {
                    SenderView::Clean { next_nonce } if rng.below(10) < 8 => next_nonce,
                    _ => rng.below(8),
                };
                let id = id_for(si, nonce % 8, rng.below(2));
                let status = if rng.below(10) == 0 {
                    ExecutionStatus::Reverted
                } else {
                    ExecutionStatus::Succeeded
                };
                txs.push(CanonicalTx {
                    hash: t(id),
                    index: txs.len() as u32,
                    sender: pool()[si],
                    nonce: nonce % 8,
                    status,
                });
            }
            ChainInput::Block(CanonicalBlock {
                number,
                hash,
                parent_hash: parent,
                timestamp_ms: number,
                txs,
            })
        } else if roll < 83 {
            let depth = 1 + rng.below(2) as usize;
            let tail = chain.len().saturating_sub(depth);
            let orphaned = chain[tail..].to_vec();
            let top = orphaned.last().map_or(BASE, |b| b.0);
            fork = fork.wrapping_add(1);
            ChainInput::Reorg {
                orphaned,
                new_head: (top, bh(top, fork)),
            }
        } else if roll < 86 {
            let tail = chain.len().saturating_sub(2);
            let top = chain.last().map_or(BASE, |b| b.0) + 3;
            fork = fork.wrapping_add(1);
            ChainInput::ReorgTooDeep {
                orphaned: chain[tail..].to_vec(),
                new_head: (top, bh(top, fork)),
            }
        } else if roll < 89 {
            let from = s.high_water().map_or(BASE, |h| h.0 + 1);
            ChainInput::Gap {
                from,
                to: from + rng.below(3),
            }
        } else {
            let sender = if rng.below(2) == 0 { own() } else { own2() };
            let (number, hash) = s.high_water().unwrap_or((BASE, bh(BASE, 0)));
            let next_nonce = match s.sender(sender) {
                SenderView::Clean { next_nonce } => next_nonce,
                _ => rng.below(6),
            };
            ChainInput::NonceFact {
                sender,
                next_nonce,
                block_number: number,
                block_hash: hash,
            }
        };
        let out = s.apply(&input);
        check_invariants(&s);
        for proof in &out.inclusions {
            assert!(s.is_watched(proof.sender()));
        }
        let refused = out.conflicts.iter().any(Conflict::refused);
        match &input {
            ChainInput::Block(b) if !refused => chain.push((b.number, b.hash)),
            ChainInput::Reorg { orphaned, .. } if !refused => {
                if s.high_water().is_some() {
                    chain.truncate(chain.len().saturating_sub(orphaned.len()));
                } else {
                    chain.clear();
                    expected = Some((orphaned.first().map_or(BASE, |b| b.0), None));
                }
            }
            ChainInput::ReorgTooDeep { new_head, .. } => {
                chain.clear();
                expected = Some((new_head.0, Some(new_head.1)));
            }
            ChainInput::Gap { to, .. } if !refused => {
                chain.clear();
                expected = Some((to + 1, None));
            }
            _ => {}
        }
        let keep = chain
            .len()
            .saturating_sub(det_limits().reorg_window as usize);
        chain.drain(..keep);
        journal.push(input);
    }
    journal
}

fn replay(journal: &[ChainInput]) -> (ChainState, Vec<Applied>) {
    let mut s = ChainState::new(CHAIN, det_limits(), &[own(), own2()]).unwrap();
    let outputs = journal.iter().map(|input| s.apply(input)).collect();
    (s, outputs)
}

/// Retirement §3 "capture/replay deterministic ordering": the same inputs
/// in the same order give an identical state and identical outputs, also
/// after the journal's binary encode/decode round trip; the structural
/// invariants hold after every input of a random journal.
#[test]
fn replay_of_the_journal_yields_identical_state_and_outputs() {
    let mut totals = StateCounters::default();
    for seed in [1u64, 7, 42, 1_234_567, 0x00de_ad00_beef] {
        let journal = random_journal(seed, 600);
        let (a, out_a) = replay(&journal);
        let (b, out_b) = replay(&journal);
        assert_eq!(a, b);
        assert_eq!(out_a, out_b);

        let decoded: Vec<ChainInput> = journal
            .iter()
            .map(|input| ChainInput::decode(&input.encode()).unwrap())
            .collect();
        assert_eq!(decoded, journal);
        let (c, out_c) = replay(&decoded);
        assert_eq!(a, c);
        assert_eq!(out_a, out_c);

        let k = a.counters();
        totals.blocks += k.blocks;
        totals.reorgs += k.reorgs;
        totals.reorgs_too_deep += k.reorgs_too_deep;
        totals.gaps += k.gaps;
        totals.replaced += k.replaced;
        totals.reconciliations += k.reconciliations;
        totals.finalized_txs += k.finalized_txs;
        totals.evicted += k.evicted;
        totals.orphaned_txs += k.orphaned_txs;
        totals.refused_inputs += k.refused_inputs;
    }
    // The journals exercised every path.
    assert!(totals.blocks > 0, "{totals:?}");
    assert!(totals.reorgs > 0, "{totals:?}");
    assert!(totals.reorgs_too_deep > 0, "{totals:?}");
    assert!(totals.gaps > 0, "{totals:?}");
    assert!(totals.replaced > 0, "{totals:?}");
    assert!(totals.reconciliations > 0, "{totals:?}");
    assert!(totals.finalized_txs > 0, "{totals:?}");
    assert!(totals.evicted > 0, "{totals:?}");
    assert!(totals.orphaned_txs > 0, "{totals:?}");
    assert!(totals.refused_inputs > 0, "{totals:?}");
}

/// Exact integers: block numbers and nonces at the top of `u64` never wrap.
#[test]
fn integer_bounds_never_wrap() {
    let mut s = state();
    let top = ChainInput::Block(CanonicalBlock {
        number: u64::MAX,
        hash: bh(1, 1),
        parent_hash: bh(0, 0),
        timestamp_ms: 0,
        txs: vec![CanonicalTx {
            hash: t(1),
            index: 0,
            sender: foreign(),
            nonce: u64::MAX,
            status: ExecutionStatus::Succeeded,
        }],
    });
    accepted(&s.apply(&top));
    assert_eq!(
        s.sender(foreign()),
        SenderView::Unknown {
            reason: SenderReason::NonceExhausted
        }
    );
    let next = ChainInput::Block(CanonicalBlock {
        number: 0,
        hash: bh(2, 2),
        parent_hash: bh(1, 1),
        timestamp_ms: 0,
        txs: Vec::new(),
    });
    assert!(matches!(
        s.apply(&next).conflicts.as_slice(),
        [Conflict::NotOnHead { .. }]
    ));
    let gap = s.apply(&ChainInput::Gap {
        from: u64::MAX,
        to: u64::MAX,
    });
    assert!(matches!(
        gap.conflicts.as_slice(),
        [Conflict::GapMismatch { .. }]
    ));
}
