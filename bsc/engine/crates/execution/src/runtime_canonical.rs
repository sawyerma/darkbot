//! The runtime's canonical side: the chain follower and the second source, the canonical
//! state they feed, the lots read on it, their recoveries and DEX quotes, and what the
//! strategy and the LIVE path are told of it.

use super::*;

/// 01.11. (R1): tells the LIVE path who decides its open lots' exits now:
/// those the strategy holds on a tracked curve are decided, the stranded
/// ones are named, every other open lot is undecided (no strategy: all).
pub(super) fn refresh_lot_deciders(c: &mut Canonical, live: &mut Option<LiveRuntime>) {
    let Some(l) = live.as_mut() else {
        return;
    };
    c.lot.open = l
        .open_positions()
        .into_iter()
        .filter_map(|p| match p {
            live::PositionChange::Opened { token, .. } => Some(token),
            live::PositionChange::Ended { .. } => None,
        })
        .collect();
    let lot = &mut c.lot;
    lot.stranded.retain(|(t, _)| lot.open.contains(t));
    lot.retry_at.retain(|(t, _)| lot.open.contains(t));
    lot.refused.retain(|(t, _)| lot.open.contains(t));
    let strategy = c.strategy.as_ref();
    let undecided = lot
        .open
        .iter()
        .copied()
        .filter(|t| {
            !strategy.is_some_and(|s| s.holds(t)) && !lot.stranded.iter().any(|(s, _)| s == t)
        })
        .collect();
    l.set_exit_deciders(
        undecided,
        lot.stranded.clone(),
        lot.faults,
        lot.fault.clone(),
    );
}

/// 01.11. (R1): at the end of block `block`, a curve read of each undecided
/// lot not read yet: once the observation window began (the strategy sees
/// the blocks), with a strategy to hold it and an armed path (its Portal),
/// at most `BSC_CANONICAL_LOT_READS_MAX` at a time, a lot whose read
/// failed not before `BSC_CANONICAL_LOT_READ_RETRY_MS` (an overtaken one at
/// once: the curve moved, nothing failed).
fn ask_lot_reads(c: &mut Canonical, live: &Option<LiveRuntime>, block: u64) {
    let (Some(l), Some(_), Some((number, hash))) = (live.as_ref(), c.strategy.as_ref(), c.head)
    else {
        return;
    };
    let Some(portal) = l.portal() else {
        return;
    };
    if number != block || !c.first_journaled {
        return;
    }
    let endpoint = c.lot.config.endpoint_at(c.active).to_string();
    let strategy = c.strategy.as_ref();
    let lot = &mut c.lot;
    let now = Instant::now();
    let wanted: Vec<[u8; 20]> = lot
        .open
        .iter()
        .copied()
        .filter(|t| {
            !strategy.is_some_and(|s| s.holds(t))
                && !lot.stranded.iter().any(|(s, _)| s == t)
                && !lot.reads.iter().any(|r| r.token == *t)
                && !lot.retry_at.iter().any(|(r, at)| r == t && *at > now)
        })
        .collect();
    for token in wanted {
        if lot.reads.len() >= lot.config.lot_reads_max {
            break;
        }
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let (read, endpoint, limits) = (lot.reader, endpoint.clone(), lot.config.limits);
        let spawned = std::thread::Builder::new()
            .name("bsc-lot-curve".into())
            .spawn(move || {
                use crate::venue_probe::ProbeError;
                let result = read(
                    &endpoint,
                    &limits,
                    portal,
                    alloy_primitives::Address::from(token),
                    number,
                    hash,
                )
                .map_err(|e| LotFault {
                    again: !matches!(e, ProbeError::Abi { .. } | ProbeError::Config(_)),
                    why: e.to_string(),
                });
                let _ = tx.send(result);
            });
        match spawned {
            Ok(_handle) => lot.reads.push(LotRead {
                token,
                block,
                hash,
                rx,
                overtaken: false,
                #[cfg(test)]
                handle: Some(_handle),
            }),
            Err(e) => {
                lot.faults += 1;
                lot.fault = Some(format!("curve read not started: {e}"));
                lot.retry_later(token);
            }
        }
    }
}

/// 01.11. (R1): before a block's events, the curve reads that came back:
/// one not overtaken holds its lot again (still open, not held; journaled
/// first, so a replay steps the same engine), or strands it off the curve
/// or outside the priced scope; a failed one counts and is asked again
/// after the backoff, a refused answer strands its lot only the
/// `BSC_CANONICAL_LOT_READ_REFUSED_MAX`-th time in a row; an overtaken one
/// is dropped (asked again at the next block's end). One still out stays.
fn hold_read_lots(
    c: &mut Canonical,
    live: &mut Option<LiveRuntime>,
    journal: &mut RuntimeJournal,
    at: u64,
) {
    use darkbot_bsc_trading::economics::flap::native_curve20::PricedToken;
    let (Some(l), Some(s)) = (live.as_ref(), c.strategy.as_mut()) else {
        return;
    };
    let lot = &mut c.lot;
    let reads = std::mem::take(&mut lot.reads);
    let open = l.open_positions();
    for r in reads {
        let answer = match r.rx.try_recv() {
            Ok(answer) => answer,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                lot.reads.push(r);
                continue;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Err(LotFault {
                again: true,
                why: "curve read ended without an answer".into(),
            }),
        };
        let curve = match answer {
            Ok(_) if r.overtaken => {
                lot.refused_in_a_row(r.token, false);
                continue;
            }
            Ok(curve) => {
                lot.refused_in_a_row(r.token, false);
                curve
            }
            Err(fault) => {
                lot.faults += 1;
                lot.fault = Some(format!(
                    "0x{}: {}",
                    alloy_primitives::hex::encode(r.token),
                    fault.why
                ));
                let refused = lot.refused_in_a_row(r.token, !fault.again);
                if !fault.again && refused >= lot.config.lot_read_refused_max {
                    lot.stranded
                        .push((r.token, crate::sources::LIVE_POSITION_UNREADABLE));
                } else {
                    lot.retry_later(r.token);
                }
                continue;
            }
        };
        let Some((tokens, spent_wei, filled_ms)) = open.iter().find_map(|p| match *p {
            live::PositionChange::Opened {
                token,
                tokens,
                spent_wei,
                filled_ms,
            } if token == r.token => Some((tokens, spent_wei, filled_ms)),
            _ => None,
        }) else {
            continue;
        };
        if s.holds(&r.token) {
            continue;
        }
        if !curve.terms.tradable {
            // R5: a token migrated to its DEX is named so (its exit the DEX
            // exit's when ON).
            let why = match curve.dex_listed {
                true => crate::sources::LIVE_POSITION_MIGRATED,
                false => crate::sources::LIVE_POSITION_OFF_CURVE,
            };
            lot.stranded.push((r.token, why));
            continue;
        }
        if PricedToken::new(&curve.terms).is_err() {
            lot.stranded
                .push((r.token, crate::sources::LIVE_POSITION_UNPRICED));
            continue;
        }
        s.reheld(
            &crate::runtime_strategy::Reheld {
                token: r.token,
                creator: l.launcher_of(&r.token).unwrap_or_default(),
                terms: curve.terms,
                reserve: curve.reserve,
                circulating: curve.circulating,
                tokens,
                spent_wei,
                filled_ms,
            },
            at,
            journal,
        );
    }
}

/// One canonical event: the C2 owner first, then the follower's records
/// (and the strategy fed from them); once the block's events are all
/// applied (`BlockComplete`), the strategy's decisions go to the PAPER
/// path on that complete state (Phase I gate M2).
#[allow(clippy::too_many_arguments)]
pub(super) fn apply_canonical(
    c: &mut Canonical,
    owner: &mut Option<StateOwner>,
    exec: &mut Option<ExecRuntime>,
    live: &mut Option<LiveRuntime>,
    journal: &mut RuntimeJournal,
    clock: Clock,
    at: u64,
    event: CanonicalEvent,
) {
    let complete = match event {
        CanonicalEvent::BlockComplete { number, .. } => Some(number),
        _ => None,
    };
    // 01.11. (R1): the curve reads this event overtakes; before a block's
    // events, the reads that came back hold their lots again (so they see
    // every later trade).
    c.lot.overtake(&event);
    let concerns = c.lot.concerns(&event);
    let mut changed = false;
    if let CanonicalEvent::Block { .. } = event {
        hold_read_lots(c, live, journal, at);
        // R3b: the recovered blocks and nonces resolve their transactions
        // before the new block's events (they are older and final).
        changed = apply_recoveries(c, live, journal, at);
    }
    if let Some(o) = owner.as_mut() {
        o.canonical(&event);
        // Plan 28.10.: C2's evidence about the LIVE buyer resolves the
        // LIVE lanes' intents, journaled with what it did.
        let evidence = o.take_evidence();
        if let Some(l) = live.as_mut()
            && !evidence.is_empty()
        {
            for (e, outcome) in evidence.iter().zip(l.apply_evidence(&evidence)) {
                journal.record(
                    RecordKind::Runtime,
                    at,
                    crate::runtime_journal::lane_evidence_payload(e, outcome),
                );
            }
        }
    }
    // Plan 28.10.: the buyer's receipts and reorgs become ledger facts;
    // a position opened or ended is held or released by Strategy C (a
    // journaled input, so a replay steps the same engine).
    if let Some(l) = live.as_mut() {
        let changes = l.on_canonical(&event);
        changed |= tell_strategy(c, changes, at, journal);
    }
    c.event(at, event, journal);
    // R1: who decides the open lots' exits, as the event left it (a BUY of
    // this event's block, from the ingress or the strategy, reads it).
    if concerns || changed {
        refresh_lot_deciders(c, live);
    }
    // BSC3-056: the verdict including this event (a block's header), before
    // a BUY is planned on it.
    c.refresh_confirmation(live, wall_or_expired(observed_now()));
    if let Some(block) = complete {
        burn_nonce(live, owner, clock, journal);
        let clk = FnClock {
            id: CLOCK_ID_MONOTONIC,
            now: || clock.now_ns(),
        };
        exit_quoted_lots(
            c,
            live,
            owner,
            &clk,
            journal,
            block,
            wall_or_expired(observed_now()),
        );
        strategy_to_paper(c, exec, live, owner, clock, journal, block);
        ask_lot_reads(c, live, block);
        ask_dex_quotes(c, live, block);
        ask_recoveries(c, live, block);
    }
}

/// REV.1.0 R5 (BSC3-030): at the end of block `block`, with the DEX exit
/// ON, the Portal's quote of each migrated lot the DEX exit takes now
/// ([`LiveRuntime::dex_exit_due`]) and not quoted yet, of the lot's
/// tokens, pinned to the block's hash, off the loop; with the curve reads
/// at most `BSC_CANONICAL_LOT_READS_MAX` at a time.
fn ask_dex_quotes(c: &mut Canonical, live: &Option<LiveRuntime>, block: u64) {
    let (Some(l), Some((number, hash))) = (live.as_ref(), c.head) else {
        return;
    };
    let Some(portal) = l.portal() else {
        return;
    };
    if number != block {
        return;
    }
    let endpoint = c.lot.config.endpoint_at(c.active).to_string();
    let lot = &mut c.lot;
    let now = Instant::now();
    let migrated: Vec<([u8; 20], alloy_primitives::U256)> = lot
        .stranded
        .iter()
        .filter(|(t, why)| {
            *why == crate::sources::LIVE_POSITION_MIGRATED
                && l.dex_exit_due(t, number)
                && !lot.quotes.iter().any(|q| q.token == *t)
                && !lot.retry_at.iter().any(|(r, at)| r == t && *at > now)
        })
        .filter_map(|(t, _)| Some((*t, l.dex_exit_amount(t)?)))
        .collect();
    for (token, tokens) in migrated {
        if lot.reads.len() + lot.quotes.len() >= lot.config.lot_reads_max {
            break;
        }
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let (quote, endpoint, limits) = (lot.quoter, endpoint.clone(), lot.config.limits);
        let spawned = std::thread::Builder::new()
            .name("bsc-dex-quote".into())
            .spawn(move || {
                let result = quote(
                    &endpoint,
                    &limits,
                    portal,
                    alloy_primitives::Address::from(token),
                    tokens,
                    number,
                    hash,
                )
                .map_err(|e| e.to_string());
                let _ = tx.send(result);
            });
        match spawned {
            Ok(_handle) => lot.quotes.push(QuoteRead {
                token,
                amount: tokens,
                block,
                hash,
                rx,
                overtaken: false,
                #[cfg(test)]
                handle: Some(_handle),
            }),
            Err(e) => {
                lot.faults += 1;
                lot.fault = Some(format!("DEX quote not started: {e}"));
            }
        }
    }
}

/// R5: a DEX quote read at block `quoted_at` prices an exit at block
/// `block` only when it is that block's or the one before.
pub(super) fn quote_fresh(quoted_at: u64, block: u64) -> bool {
    quoted_at <= block && quoted_at.saturating_add(1) >= block
}

/// REV.1.0 R5 (BSC3-030): at block `block`'s complete state, the DEX quotes
/// that came back, of this block or the one before and still on the chain,
/// go to the LIVE path's DEX exit ([`LiveRuntime::on_dex_exit`]), its
/// intents journaled; an older or overtaken one is dropped (quoted again);
/// a failed one, or one the exit refused, counts and waits for the read
/// backoff (`BSC_CANONICAL_LOT_READ_RETRY_MS`). `now_unix_ms`: the wall
/// clock the head's age is measured against.
#[allow(clippy::too_many_arguments)]
pub(super) fn exit_quoted_lots(
    c: &mut Canonical,
    live: &mut Option<LiveRuntime>,
    owner: &Option<StateOwner>,
    clk: &dyn crate::submission::transport::Clock,
    journal: &mut RuntimeJournal,
    block: u64,
    now_unix_ms: u64,
) {
    let (Some(l), Some(o)) = (live.as_mut(), owner.as_ref()) else {
        return;
    };
    let quotes = std::mem::take(&mut c.lot.quotes);
    for q in quotes {
        let quote = match q.rx.try_recv() {
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                c.lot.quotes.push(q);
                continue;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err("DEX quote ended without an answer".to_string())
            }
            Ok(answer) => answer,
        };
        let quote = match quote {
            Ok(_) if q.overtaken => continue,
            Ok(quote) => quote,
            Err(why) => {
                c.lot.faults += 1;
                c.lot.fault = Some(format!(
                    "DEX quote 0x{}: {why}",
                    alloy_primitives::hex::encode(q.token)
                ));
                c.lot.retry_later(q.token);
                continue;
            }
        };
        if !quote_fresh(quote.block_number, block) {
            continue;
        }
        if let Some((intents, refused)) =
            l.on_dex_exit(q.token, q.amount, quote.output, o, clk, now_unix_ms)
        {
            journal_live(journal, &intents);
            if intents.is_empty() && refused.is_some() {
                c.lot.retry_later(q.token);
            }
        }
    }
}

/// A position the LIVE ledger opened or ended, to Strategy C (held or
/// released, journaled); whether there was any.
fn tell_strategy(
    c: &mut Canonical,
    changes: Vec<live::PositionChange>,
    at: u64,
    journal: &mut RuntimeJournal,
) -> bool {
    let changed = !changes.is_empty();
    for change in changes {
        let Some(s) = c.strategy.as_mut() else {
            continue;
        };
        match change {
            live::PositionChange::Opened {
                token,
                tokens,
                spent_wei,
                filled_ms,
            } => s.hold(token, tokens, spent_wei, filled_ms, at, journal),
            live::PositionChange::Ended { token } => s.unhold(token, at, journal),
        }
    }
    changed
}

/// REV.1.0 R3b (BSC3-024/041): at the end of block `block`, a receipt
/// recovery of each unresolved LIVE transaction the follower cannot
/// resolve any more ([`LiveRuntime::recovery_targets`]) not asked yet: off
/// the loop, against the followed endpoint, every block it rests on final
/// and confirmed by the second source; at most
/// `BSC_CANONICAL_RECOVERY_READS_MAX` at a time, one asked again no earlier
/// than `BSC_CANONICAL_RECOVERY_RETRY_MS` after its last answer. Without a
/// second source nothing is recovered (each named as such).
fn ask_recoveries(c: &mut Canonical, live: &mut Option<LiveRuntime>, block: u64) {
    let (Some(l), Some((number, _))) = (live.as_mut(), c.head) else {
        return;
    };
    if number != block {
        return;
    }
    let targets = l.recovery_targets(number);
    let r = &mut c.recovery;
    // Only what is still unresolved is named or waited for.
    r.pending
        .retain(|(k, _)| targets.iter().any(|(t, _)| t == k));
    r.retry_at
        .retain(|(k, _)| targets.iter().any(|(t, _)| t == k));
    let primary = c.lot.config.endpoint_at(c.active);
    let now = Instant::now();
    for (key, target) in targets {
        let (target, second) = match (target, c.confirm.as_ref()) {
            // Never asked: named why.
            (Err(why), _) => {
                r.note(key, why);
                continue;
            }
            (Ok(_), None) => {
                r.note(key, crate::sources::LIVE_RECOVERY_NO_SECOND_SOURCE);
                continue;
            }
            (Ok(target), Some((_, confirm))) => (target, confirm.endpoint.clone()),
        };
        let asked = r.reads.iter().any(|x| x.key == key)
            || r.retry_at.iter().any(|(k, at)| *k == key && *at > now);
        if asked {
            continue;
        }
        if r.reads.len() >= r.reads_max {
            r.note(key, crate::sources::LIVE_RECOVERY_QUEUED);
            continue;
        }
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let (read, primary, limits, search) = (
            r.reader,
            primary.to_string(),
            c.lot.config.limits,
            r.search_blocks,
        );
        let spawned = std::thread::Builder::new()
            .name("bsc-live-recovery".into())
            .spawn(move || {
                let _ = tx.send(read(&primary, &second, &limits, search, number, target));
            });
        match spawned {
            Ok(_handle) => {
                r.note(key, crate::sources::LIVE_RECOVERY_ASKED);
                r.reads.push(RecoveryRead {
                    key,
                    rx,
                    #[cfg(test)]
                    handle: Some(_handle),
                });
            }
            Err(e) => r.failed(key, format!("recovery read not started: {e}")),
        }
    }
    l.set_recovery(r.pending.clone(), r.faults, r.fault.clone());
}

/// REV.1.0 R3b: before a block's events, the recoveries that came back: a
/// final block found resolves its transactions as the follower's block
/// would (source RECEIPT); anything else is named and asked again after
/// the backoff. Whether a position changed.
fn apply_recoveries(
    c: &mut Canonical,
    live: &mut Option<LiveRuntime>,
    journal: &mut RuntimeJournal,
    at: u64,
) -> bool {
    use crate::chain_follower::Recovered;
    let r = &mut c.recovery;
    let Some(l) = live.as_mut().filter(|_| !r.reads.is_empty()) else {
        return false;
    };
    let reads = std::mem::take(&mut r.reads);
    let mut changes = Vec::new();
    for read in reads {
        let answer = match read.rx.try_recv() {
            Ok(answer) => answer,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                r.reads.push(read);
                continue;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err("recovery read ended without an answer".into())
            }
        };
        let key = read.key;
        match answer {
            Ok(Recovered::Block(events)) => match l.recover_block(&key, &events) {
                Some(c) => {
                    changes.extend(c);
                    r.pending.retain(|(k, _)| *k != key);
                }
                // A followed block is being applied: asked again.
                None => r.note(key, crate::sources::LIVE_RECOVERY_ASKED),
            },
            Ok(Recovered::Pending(why)) => r.note(key, why.name()),
            Err(why) => r.failed(key, why),
        }
        r.retry_later(key);
    }
    l.set_recovery(r.pending.clone(), r.faults, r.fault.clone());
    tell_strategy(c, changes, at, journal)
}

/// Every queued canonical event, now (Phase I gate H1: also between the
/// ingress events of a pass, every `BSC_RUNTIME_CANONICAL_SLICE_MS`).
pub(super) fn drain_canonical(
    canonical: &mut Option<Canonical>,
    crx: Option<&std::sync::mpsc::Receiver<(u64, CanonicalEvent)>>,
    owner: &mut Option<StateOwner>,
    exec: &mut Option<ExecRuntime>,
    live: &mut Option<LiveRuntime>,
    journal: &mut RuntimeJournal,
    clock: Clock,
) {
    let (Some(c), Some(crx)) = (canonical.as_mut(), crx) else {
        return;
    };
    while let Ok((at, event)) = crx.try_recv() {
        apply_canonical(c, owner, exec, live, journal, clock, at, event);
    }
}

/// The follower thread and its stop flag.
pub(super) struct Follower {
    pub(super) stop: Arc<AtomicBool>,
    pub(super) handle: JoinHandle<()>,
}

/// Every canonical event crosses to the runtime with its time on the
/// runtime clock (a block: when the follower first had its header). The
/// send blocks: the follower falls behind rather than losing an event.
pub(super) fn spawn_follower(
    config: CanonicalConfig,
    resume: Vec<(u64, [u8; 32])>,
    tx: SyncSender<(u64, CanonicalEvent)>,
    clock: Clock,
) -> Result<Follower, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let handle = thread::Builder::new()
        .name("bsc-canonical".into())
        .spawn(move || {
            let mut sink = |event: CanonicalEvent| {
                let at = match &event {
                    CanonicalEvent::Block { observed, .. } => clock.ns_at(*observed),
                    _ => clock.now_ns(),
                };
                // Err only once the runtime dropped the receiver at stop.
                let _ = tx.send((at, event));
            };
            // The watched senders' nonce facts (the C2 owner may be switched
            // on at any time; without it they are not applied).
            let watch = NonceWatch {
                senders: config.state.watched.clone(),
                every_blocks: config.state.nonce_fact_blocks,
            };
            if let Err(e) = run_follower(
                &config.endpoints(),
                &config.limits,
                &resume,
                &watch,
                &flag,
                &mut sink,
            ) {
                eprintln!("canonical follower refused its configuration: {e:?}");
            }
        })
        .map_err(|e| format!("spawn canonical follower: {e}"))?;
    Ok(Follower { stop, handle })
}

/// BSC3-056: the second source's header thread (its events to `tx`).
pub(super) fn spawn_confirmer(
    config: &CanonicalConfig,
    confirm: &ConfirmConfig,
    tx: SyncSender<crate::chain_follower::ConfirmEvent>,
) -> Result<Follower, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let (endpoint, limits) = (confirm.endpoint.clone(), config.limits);
    let handle = thread::Builder::new()
        .name("bsc-confirm".into())
        .spawn(move || {
            let mut sink = |event| {
                let _ = tx.send(event);
            };
            if let Err(e) =
                crate::chain_follower::run_confirmer(&endpoint, &limits, &flag, &mut sink)
            {
                eprintln!("second source refused its configuration: {e:?}");
            }
        })
        .map_err(|e| format!("spawn second source: {e}"))?;
    Ok(Follower { stop, handle })
}

/// The runtime's view of the canonical chain and the launch join.
pub(super) struct Canonical {
    pub(super) state: &'static str,
    /// Phase E review L3: the Four.meme drift guard's last verdict
    /// (`UNPROBED`, `MATCH at block n`, `DRIFT at block n: ...`,
    /// `PROBE_FAILED: ...`); while drifted, no Four token derivation is
    /// proven (journaled as UNKNOWN).
    pub(super) four_deployment: String,
    pub(super) four_drift: bool,
    pub(super) head: Option<(u64, [u8; 32])>,
    head_seen_ns: Option<u64>,
    /// The head block's time (header ms) and the local wall ms when the
    /// follower first held it.
    head_block_ms: Option<u64>,
    head_seen_unix_ms: Option<u64>,
    /// The observation window starts with the first block mined at or
    /// after this wall time (ms): this process's start. Earlier blocks (a
    /// C2 resume backfill, or a head mined before the sources connected)
    /// and their launches, reorgs and gaps are outside it.
    window_from_ms: u64,
    pub(super) first_journaled: bool,
    /// The window's first block (its `H` record); heights below it are
    /// never counted as skipped.
    pub(super) window_first_block: u64,
    pub(super) backfill_blocks: u64,
    pub(super) backfill_launches: u64,
    /// BSC3-014: canonical launches whose block was not the last reported
    /// (no head to join them).
    pub(super) launch_head_missing: u64,
    /// Launch calls included and reverted, and the recent ones (a reorg
    /// within the follower's window takes them out again).
    reverted_launches: u64,
    /// Phase H: Flap curve trades journaled, and trade logs refused.
    flap_trades: u64,
    flap_trades_undecodable: u64,
    reverted_recent: Vec<(u64, [u8; 32], [u8; 32], crate::canonical_join::Venue)>,
    /// The deepest reorg the follower reports (`BSC_CANONICAL_REORG_DEPTH`).
    reorg_window: u64,
    /// Phase H: Strategy C, fed from these records while `BSC_STRATEGY_C`
    /// runs (SHADOW).
    pub(super) strategy: Option<crate::runtime_strategy::StrategyRuntime>,
    /// The strategy's configuration while its component is on, and why it
    /// cannot run (a refused gas evidence).
    pub(super) strategy_config: Option<darkbot_bsc_trading::strategy::engine::Config>,
    pub(super) strategy_refused: Option<String>,
    pub(super) blocks_skipped: u64,
    pub(super) blocks: u64,
    pub(super) reorgs: u64,
    pub(super) resyncs: u64,
    pub(super) rpc_errors: u64,
    rate_limited: u64,
    /// BSC3-036: the hosts of the configured endpoints (never a path),
    /// the one followed now, the failovers and the stalled steps.
    hosts: Vec<String>,
    pub(super) active: usize,
    failovers: u64,
    stalls: u64,
    /// BSC3-056: the followed chain's and the second source's recent
    /// headers, the second source's host and bounds, its failed steps.
    ours: crate::canonical_confirm::Ring,
    second: crate::canonical_confirm::Ring,
    pub(super) confirm: Option<(String, ConfirmConfig)>,
    confirm_faults: u64,
    /// The second source's thread's events (set when it runs).
    pub(super) confirm_rx: Option<std::sync::mpsc::Receiver<crate::chain_follower::ConfirmEvent>>,
    pub(super) ledger: Ledger,
    /// BSC3-062: [`CanonicalConfig::join_horizon_ms`] in µs.
    join_horizon_us: u64,
    /// 01.11. (R1): the exit deciders of the open LIVE lots.
    pub(super) lot: LotDeciders,
    /// REV.1.0 R3b: the receipt recoveries of unresolved LIVE transactions.
    pub(super) recovery: Recoveries,
}

/// REV.1.0 R3b: what the chain says of one unresolved LIVE transaction at
/// the followed head (production: [`read_recovery`]): the followed
/// endpoint, the second source, the follower's bounds, the search's depth,
/// the head, the target.
type ChainRecovery = fn(
    &str,
    &str,
    &FollowerLimits,
    u64,
    u64,
    crate::chain_follower::RecoveryTarget,
) -> Result<crate::chain_follower::Recovered, String>;

/// The production recovery: one connection to each endpoint, the
/// follower's bounds on every request (a runtime that stops leaves it to
/// end by them).
fn read_recovery(
    primary: &str,
    second: &str,
    limits: &FollowerLimits,
    search_blocks: u64,
    head: u64,
    target: crate::chain_follower::RecoveryTarget,
) -> Result<crate::chain_follower::Recovered, String> {
    let never = AtomicBool::new(false);
    crate::chain_follower::recover_transaction(
        primary,
        second,
        limits,
        search_blocks,
        head,
        target,
        &never,
    )
    .map_err(|e| format!("{e:?}"))
}

/// REV.1.0 R3b (BSC3-024/041): the receipt recoveries of the ledger's
/// unresolved LIVE transactions: the reads out, when each may be asked
/// again, each one's last word (why it is not resolved), the failures.
pub(super) struct Recoveries {
    pub(super) reads_max: usize,
    retry_ms: u64,
    search_blocks: u64,
    pub(super) reader: ChainRecovery,
    pub(super) reads: Vec<RecoveryRead>,
    pub(super) retry_at: Vec<([u8; 32], Instant)>,
    pub(super) pending: Vec<([u8; 32], &'static str)>,
    faults: u64,
    fault: Option<String>,
}

pub(super) struct RecoveryRead {
    pub(super) key: [u8; 32],
    rx: std::sync::mpsc::Receiver<Result<crate::chain_follower::Recovered, String>>,
    #[cfg(test)]
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Recoveries {
    fn new(config: &CanonicalConfig) -> Self {
        Recoveries {
            reads_max: config.recovery_reads_max,
            retry_ms: config.recovery_retry_ms,
            search_blocks: config.recovery_search_blocks,
            reader: read_recovery,
            reads: Vec::new(),
            retry_at: Vec::new(),
            pending: Vec::new(),
            faults: 0,
            fault: None,
        }
    }

    /// `key`'s last word.
    fn note(&mut self, key: [u8; 32], why: &'static str) {
        match self.pending.iter_mut().find(|(k, _)| *k == key) {
            Some(p) => p.1 = why,
            None => self.pending.push((key, why)),
        }
    }

    fn failed(&mut self, key: [u8; 32], why: String) {
        self.faults += 1;
        self.fault = Some(format!("0x{}: {why}", alloy_primitives::hex::encode(key)));
        self.note(key, crate::sources::LIVE_RECOVERY_FAILED);
        self.retry_later(key);
    }

    fn retry_later(&mut self, key: [u8; 32]) {
        let at = Instant::now() + Duration::from_millis(self.retry_ms);
        self.retry_at.retain(|(k, _)| *k != key);
        self.retry_at.push((key, at));
    }

    /// Tests: waits until every read out has answered.
    #[cfg(test)]
    pub(super) fn settle(&mut self) {
        for r in &mut self.reads {
            if let Some(h) = r.handle.take() {
                let _ = h.join();
            }
        }
    }
}

/// 01.11. (R1): reads a token's curve at one block (production:
/// [`crate::venue_probe::read_flap_curve`]).
type CurveReader = fn(
    &str,
    &FollowerLimits,
    alloy_primitives::Address,
    alloy_primitives::Address,
    u64,
    [u8; 32],
) -> Result<crate::venue_probe::FlapCurve, crate::venue_probe::ProbeError>;

/// REV.1.0 R5: the Portal's quote of a migrated token's SELL at one block
/// (production: [`crate::venue_probe::read_flap_dex_quote`]).
type DexQuoter = fn(
    &str,
    &FollowerLimits,
    alloy_primitives::Address,
    alloy_primitives::Address,
    alloy_primitives::U256,
    u64,
    [u8; 32],
) -> Result<crate::venue_probe::DexQuote, crate::venue_probe::ProbeError>;

/// REV.1.0 R5: one DEX quote of a migrated lot's tokens at a block;
/// `overtaken` once that block may have left the chain.
pub(super) struct QuoteRead {
    pub(super) token: [u8; 20],
    /// The tokens quoted: what the exit sells.
    pub(super) amount: alloy_primitives::U256,
    pub(super) block: u64,
    pub(super) hash: [u8; 32],
    pub(super) rx: std::sync::mpsc::Receiver<Result<crate::venue_probe::DexQuote, String>>,
    pub(super) overtaken: bool,
    #[cfg(test)]
    pub(super) handle: Option<std::thread::JoinHandle<()>>,
}

/// 01.11. (R1): who decides the open LIVE lots' exits. A lot the strategy
/// does not hold (a restart, a gap or a reorg ended its tracking, or its
/// token was never followed) is read (its curve at a block's end, off the
/// loop, pinned to the block's hash) and held again on that curve before
/// the next block's events, while nothing moved the curve since (no trade,
/// term or migration of the token, no reorg or gap). Off the curve, outside
/// the priced scope or unreadable for good it is stranded. The LIVE path
/// refuses new BUYs while a lot is undecided or stranded.
pub(super) struct LotDeciders {
    pub(super) config: CanonicalConfig,
    pub(super) reader: CurveReader,
    pub(super) reads: Vec<LotRead>,
    /// R5: the migrated lots' DEX quotes (the DEX exit ON).
    pub(super) quoter: DexQuoter,
    pub(super) quotes: Vec<QuoteRead>,
    /// The tokens of the open lots at the last refresh.
    open: Vec<[u8; 20]>,
    pub(super) stranded: Vec<([u8; 20], &'static str)>,
    /// A token read again no earlier than this (after a failed read), and
    /// its refused answers in a row.
    pub(super) retry_at: Vec<([u8; 20], Instant)>,
    pub(super) refused: Vec<([u8; 20], u32)>,
    faults: u64,
    pub(super) fault: Option<String>,
}

/// One curve read: the lot's token, the block it reads, its answer;
/// `overtaken` once the curve moved after that block.
pub(super) struct LotRead {
    pub(super) token: [u8; 20],
    block: u64,
    hash: [u8; 32],
    rx: std::sync::mpsc::Receiver<Result<crate::venue_probe::FlapCurve, LotFault>>,
    overtaken: bool,
    #[cfg(test)]
    handle: Option<std::thread::JoinHandle<()>>,
}

/// A failed curve read: `again` when a later read may succeed (transport,
/// a reorg between its reads), not when the answer itself is refused.
struct LotFault {
    again: bool,
    why: String,
}

impl LotDeciders {
    pub(super) fn new(config: &CanonicalConfig) -> Self {
        LotDeciders {
            config: config.clone(),
            reader: crate::venue_probe::read_flap_curve,
            reads: Vec::new(),
            quoter: crate::venue_probe::read_flap_dex_quote,
            quotes: Vec::new(),
            open: Vec::new(),
            stranded: Vec::new(),
            retry_at: Vec::new(),
            refused: Vec::new(),
            faults: 0,
            fault: None,
        }
    }

    /// The reads `event` overtakes: a trade, term or migration of their
    /// token, or any reorg, gap or refused Flap log; a block whose parent is
    /// not the block a read was of, for that read.
    pub(super) fn overtake(&mut self, event: &CanonicalEvent) {
        let token = match event {
            CanonicalEvent::FlapTrade(t) => Some(t.token),
            CanonicalEvent::FlapTerm(t) => Some(t.token),
            CanonicalEvent::FlapMigrated { token, .. } => Some(*token),
            CanonicalEvent::Reorg { .. }
            | CanonicalEvent::ReorgTooDeep { .. }
            | CanonicalEvent::Gap { .. }
            | CanonicalEvent::FlapTradeUndecodable { .. } => {
                self.reads.iter_mut().for_each(|r| r.overtaken = true);
                self.quotes.iter_mut().for_each(|q| q.overtaken = true);
                None
            }
            CanonicalEvent::Block {
                number,
                parent_hash,
                ..
            } => {
                for r in &mut self.reads {
                    if r.block.checked_add(1) == Some(*number) && r.hash != *parent_hash {
                        r.overtaken = true;
                    }
                }
                for q in &mut self.quotes {
                    if q.block.checked_add(1) == Some(*number) && q.hash != *parent_hash {
                        q.overtaken = true;
                    }
                }
                None
            }
            _ => None,
        };
        if let Some(token) = token {
            for r in self.reads.iter_mut().filter(|r| r.token == token) {
                r.overtaken = true;
            }
        }
    }

    /// Whether `event` can change who decides an open lot's exit.
    fn concerns(&self, event: &CanonicalEvent) -> bool {
        match event {
            CanonicalEvent::FlapTrade(t) => self.open.contains(&t.token),
            CanonicalEvent::FlapTerm(t) => self.open.contains(&t.token),
            CanonicalEvent::FlapMigrated { token, .. } => self.open.contains(token),
            CanonicalEvent::Block { .. }
            | CanonicalEvent::BlockComplete { .. }
            | CanonicalEvent::Reorg { .. }
            | CanonicalEvent::ReorgTooDeep { .. }
            | CanonicalEvent::Gap { .. }
            | CanonicalEvent::FlapTradeUndecodable { .. } => true,
            _ => false,
        }
    }

    /// Tests: waits until every read out has answered.
    #[cfg(test)]
    pub(super) fn settle(&mut self) {
        for r in &mut self.reads {
            if let Some(h) = r.handle.take() {
                let _ = h.join();
            }
        }
        for q in &mut self.quotes {
            if let Some(h) = q.handle.take() {
                let _ = h.join();
            }
        }
    }

    /// How many of `token`'s last reads in a row were refused answers, this
    /// one counted (`refused`): any other outcome, an answer or a failure
    /// in transit, ends the run (review L7).
    pub(super) fn refused_in_a_row(&mut self, token: [u8; 20], refused: bool) -> u32 {
        if !refused {
            self.refused.retain(|(t, _)| *t != token);
            return 0;
        }
        match self.refused.iter_mut().find(|(t, _)| *t == token) {
            Some((_, n)) => {
                *n += 1;
                *n
            }
            None => {
                self.refused.push((token, 1));
                1
            }
        }
    }

    pub(super) fn retry_later(&mut self, token: [u8; 20]) {
        let at = Instant::now() + Duration::from_millis(self.config.lot_read_retry_ms);
        self.retry_at.retain(|(t, _)| *t != token);
        self.retry_at.push((token, at));
    }

    /// REV.1.0 R3: the operator resumed `token`'s lot: stranded no more,
    /// its curve is read again at the next block's end (no backoff, no
    /// refusal counted from before).
    pub(super) fn resume(&mut self, token: [u8; 20]) {
        self.stranded.retain(|(t, _)| *t != token);
        self.retry_at.retain(|(t, _)| *t != token);
        self.refused.retain(|(t, _)| *t != token);
    }
}

impl Canonical {
    pub(super) fn new(config: &CanonicalConfig, window_from_ms: u64) -> Self {
        Canonical {
            state: "CONNECTING",
            four_deployment: "UNPROBED".into(),
            four_drift: false,
            head: None,
            head_seen_ns: None,
            head_block_ms: None,
            head_seen_unix_ms: None,
            window_from_ms,
            first_journaled: false,
            window_first_block: 0,
            backfill_blocks: 0,
            backfill_launches: 0,
            launch_head_missing: 0,
            reverted_launches: 0,
            flap_trades: 0,
            flap_trades_undecodable: 0,
            reverted_recent: Vec::new(),
            reorg_window: config.limits.reorg_depth,
            strategy: None,
            strategy_config: None,
            strategy_refused: None,
            blocks_skipped: 0,
            blocks: 0,
            reorgs: 0,
            resyncs: 0,
            rpc_errors: 0,
            rate_limited: 0,
            hosts: config
                .endpoints()
                .iter()
                .map(|e| crate::chain_follower::endpoint_host(e).unwrap_or_default())
                .collect(),
            active: 0,
            failovers: 0,
            stalls: 0,
            ours: crate::canonical_confirm::Ring::new(
                usize::try_from(config.limits.reorg_depth.saturating_add(1)).unwrap_or(usize::MAX),
            ),
            second: crate::canonical_confirm::Ring::new(
                usize::try_from(config.limits.reorg_depth.saturating_add(1)).unwrap_or(usize::MAX),
            ),
            confirm: config.confirm.as_ref().map(|c| {
                (
                    crate::chain_follower::endpoint_host(&c.endpoint).unwrap_or_default(),
                    c.clone(),
                )
            }),
            confirm_faults: 0,
            confirm_rx: None,
            ledger: Ledger::new(
                config.join_delay_ms.saturating_mul(1_000_000),
                config.limits.reorg_depth,
            ),
            join_horizon_us: config.join_horizon_ms.saturating_mul(1_000),
            lot: LotDeciders::new(config),
            recovery: Recoveries::new(config),
        }
    }

    /// Journals a canonical record and feeds the same bytes to the
    /// strategy (its replay reads them back the same way).
    fn put(&mut self, kind: RecordKind, at: u64, payload: Vec<u8>, journal: &mut RuntimeJournal) {
        match self.strategy.as_mut() {
            Some(s) => {
                // The record precedes its outputs in the journal.
                journal.record(kind, at, payload.clone());
                s.record(kind, &payload, at, Some(journal));
            }
            None => {
                journal.record(kind, at, payload);
            }
        }
    }

    fn orphan(&mut self, orphaned: &[(u64, [u8; 32])], at: u64, journal: &mut RuntimeJournal) {
        let hashes: Vec<[u8; 32]> = orphaned.iter().map(|(_, h)| *h).collect();
        for l in self.ledger.orphan(&hashes) {
            journal.record(RecordKind::Canonical, at, orphaned_launch_payload(&l));
        }
        let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.reverted_recent)
            .into_iter()
            .partition(|(_, block, _, _)| hashes.contains(block));
        self.reverted_recent = kept;
        for (number, block, tx, venue) in gone {
            self.reverted_launches = self.reverted_launches.saturating_sub(1);
            journal.record(
                RecordKind::Canonical,
                at,
                launch_id_payload(b'O', venue, number, &block, &tx),
            );
        }
    }

    /// BSC3-056: a header or fault of the second source.
    pub(super) fn confirm_event(&mut self, event: crate::chain_follower::ConfirmEvent) {
        match event {
            crate::chain_follower::ConfirmEvent::Header {
                number,
                hash,
                parent_hash,
                block_ms,
            } => self.second.push(number, hash, parent_hash, block_ms),
            crate::chain_follower::ConfirmEvent::Fault(_) => self.confirm_faults += 1,
        }
    }

    /// BSC3-056: whether the second source confirms the followed chain at
    /// `now_ms`, with the lag it is judged under; `None` without a second
    /// source.
    fn confirmation(&self, now_ms: u64) -> Option<crate::canonical_confirm::Confirmation> {
        self.confirm
            .as_ref()
            .map(|(_, c)| crate::canonical_confirm::Confirmation {
                verdict: crate::canonical_confirm::verdict(
                    &self.ours,
                    &self.second,
                    now_ms,
                    c.max_age_ms,
                    c.max_lag_blocks,
                ),
                max_lag_blocks: c.max_lag_blocks,
            })
    }

    /// BSC3-056: the second source's queued headers, then the verdict at
    /// `now_ms` handed to the LIVE path: before a block's BUY is planned
    /// (each canonical event) and every pass.
    pub(super) fn refresh_confirmation(&mut self, live: &mut Option<LiveRuntime>, now_ms: u64) {
        while let Some(event) = self.confirm_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.confirm_event(event);
        }
        if let Some(l) = live.as_mut() {
            l.set_confirmation(self.confirmation(now_ms));
        }
    }

    pub(super) fn event(&mut self, at: u64, event: CanonicalEvent, journal: &mut RuntimeJournal) {
        // BSC3-056: the followed chain's recent headers, before the window
        // too (a LIVE BUY compares at the head).
        match &event {
            CanonicalEvent::Block {
                number,
                hash,
                parent_hash,
                timestamp_ms,
                ..
            } => self.ours.push(*number, *hash, *parent_hash, *timestamp_ms),
            CanonicalEvent::Reorg { orphaned, .. }
            | CanonicalEvent::ReorgTooDeep { orphaned, .. } => self.ours.orphan(orphaned),
            _ => {}
        }
        match event {
            CanonicalEvent::Block {
                number,
                hash,
                timestamp_ms,
                observed_unix_ms,
                ..
            } => {
                self.state = "FOLLOWING";
                self.head = Some((number, hash));
                self.head_seen_ns = Some(at);
                self.head_block_ms = Some(timestamp_ms);
                self.head_seen_unix_ms = observed_unix_ms;
                if !self.first_journaled {
                    if timestamp_ms < self.window_from_ms {
                        self.backfill_blocks += 1;
                        // REV.1.0 R2 (review P2): the catch-up before the
                        // window, a runtime record no G0.4 count reads: the
                        // LIVE campaign's chain while no run was up.
                        journal.record(
                            RecordKind::Runtime,
                            at,
                            crate::runtime_journal::catchup_block_payload(
                                number,
                                &hash,
                                timestamp_ms,
                            ),
                        );
                        return;
                    }
                    self.first_journaled = true;
                    self.window_first_block = number;
                    journal.record(
                        RecordKind::Canonical,
                        at,
                        window_payload(b'H', number, &hash),
                    );
                }
                self.blocks += 1;
                self.put(
                    RecordKind::Canonical,
                    at,
                    crate::runtime_strategy::block_payload(number, &hash, timestamp_ms),
                    journal,
                );
            }
            // Before the window: not part of the G0.4 counts (the C2 owner
            // has applied them already).
            CanonicalEvent::Launch(l) if !self.first_journaled => {
                self.backfill_launches += 1;
                // REV.1.0 R2 (review P2): a launch while no run was up, for
                // the LIVE campaign's skips (no sighting could see it).
                if let Some(block_ms) = self
                    .head_block_ms
                    .filter(|_| self.head.is_some_and(|(_, h)| h == l.block_hash))
                {
                    journal.record(
                        RecordKind::Runtime,
                        at,
                        crate::runtime_journal::catchup_launch_payload(&l, block_ms),
                    );
                }
            }
            // REV.1.0 R2 (review Q1): the catch-up's Portal trades and
            // refused trade logs too (runtime records), so a market window
            // reaching into it is complete only with them; terms and
            // migrations before the window stay unjournaled.
            CanonicalEvent::FlapTrade(t) if !self.first_journaled => {
                journal.record(
                    RecordKind::Runtime,
                    at,
                    crate::runtime_journal::catchup_trade_payload(&t),
                );
            }
            CanonicalEvent::FlapTradeUndecodable {
                block_number,
                block_hash,
                tx_index,
                log_index,
            } if !self.first_journaled => {
                journal.record(
                    RecordKind::Runtime,
                    at,
                    crate::runtime_journal::catchup_trade_gap_payload(
                        block_number,
                        &block_hash,
                        tx_index,
                        log_index,
                    ),
                );
            }
            CanonicalEvent::FlapTerm(_) | CanonicalEvent::FlapMigrated { .. }
                if !self.first_journaled => {}
            CanonicalEvent::FlapTerm(t) => {
                self.put(RecordKind::Canonical, at, flap_term_payload(&t), journal);
            }
            CanonicalEvent::FlapMigrated {
                block_number,
                block_hash,
                tx_index,
                log_index,
                token,
            } => {
                self.put(
                    RecordKind::Canonical,
                    at,
                    flap_migrated_payload(block_number, &block_hash, tx_index, log_index, &token),
                    journal,
                );
            }
            // Phase H: every Flap curve trade of the window, for the
            // strategy and its replay.
            CanonicalEvent::FlapTrade(t) => {
                self.flap_trades += 1;
                self.put(RecordKind::Canonical, at, flap_trade_payload(&t), journal);
            }
            CanonicalEvent::FlapTradeUndecodable {
                block_number,
                block_hash,
                tx_index,
                log_index,
            } => {
                self.flap_trades_undecodable += 1;
                self.put(
                    RecordKind::Canonical,
                    at,
                    trade_gap_payload(block_number, &block_hash, tx_index, log_index),
                    journal,
                );
            }
            CanonicalEvent::RevertedLaunch { .. } if !self.first_journaled => {}
            CanonicalEvent::RevertedLaunch {
                venue,
                block_number,
                block_hash,
                tx_hash,
                ..
            } => {
                self.reverted_launches += 1;
                self.reverted_recent
                    .push((block_number, block_hash, tx_hash, venue));
                // Older than any reorg the follower reports: final.
                let floor = block_number.saturating_sub(self.reorg_window);
                self.reverted_recent.retain(|(n, ..)| *n >= floor);
                journal.record(
                    RecordKind::Canonical,
                    at,
                    launch_id_payload(b'V', venue, block_number, &block_hash, &tx_hash),
                );
            }
            CanonicalEvent::Reorg { .. }
            | CanonicalEvent::ReorgTooDeep { .. }
            | CanonicalEvent::Gap { .. }
                if !self.first_journaled => {}
            CanonicalEvent::Launch(l) => {
                // Its block is the last one reported (launches follow it).
                let same = self.head.is_some_and(|(_, h)| h == l.block_hash);
                // BSC3-014: without that head nothing joins it: counted and
                // journaled as received, so the census shows it unjoined.
                let Some(block_ms) = self.head_block_ms.filter(|_| same) else {
                    self.launch_head_missing += 1;
                    journal.record(
                        RecordKind::Canonical,
                        at,
                        crate::runtime_journal::received_launch_payload(
                            l.venue,
                            l.block_number,
                            &l.block_hash,
                            &l.tx_hash,
                        ),
                    );
                    return;
                };
                let launch = LaunchRef {
                    venue: l.venue,
                    block_number: l.block_number,
                    block_hash: l.block_hash,
                    tx_index: l.tx_index,
                    tx_hash: l.tx_hash,
                    token: l.token,
                    external_buys: l.external_buys_after,
                    c1_scope: l.c1_scope,
                    venue_target: l.venue_target,
                    // Drifted: the frozen derivation proves nothing.
                    four_token: match l.four_token {
                        crate::chain_follower::FourToken::Matched
                        | crate::chain_follower::FourToken::Mismatch
                        | crate::chain_follower::FourToken::Candidate
                            if self.four_drift =>
                        {
                            crate::chain_follower::FourToken::Unknown
                        }
                        t => t,
                    },
                    tx: match l.unverifiable {
                        None => LaunchTx::Verified,
                        Some(r) if is_unrebuildable(r) => LaunchTx::Unrebuildable,
                        Some(_) => LaunchTx::AnswerRefused,
                    },
                    block_ms,
                    block_seen_ns: self.head_seen_ns.filter(|_| same).unwrap_or(at),
                    block_seen_unix_ms: self.head_seen_unix_ms.filter(|_| same),
                };
                if let (crate::canonical_join::Venue::Flap, Some(token), Some(creator)) =
                    (l.venue, l.token, l.creator)
                {
                    self.put(
                        RecordKind::Canonical,
                        at,
                        crate::runtime_strategy::flap_launch_payload(
                            l.block_number,
                            l.tx_index,
                            &token,
                            &creator,
                        ),
                        journal,
                    );
                }
                journal.record(RecordKind::Canonical, at, pending_launch_payload(&launch));
                self.ledger.push(launch);
            }
            CanonicalEvent::Reorg { orphaned, new_head } => {
                self.reorgs += 1;
                let depth = u32::try_from(orphaned.len()).unwrap_or(u32::MAX);
                self.put(
                    RecordKind::Reorg,
                    at,
                    reorg_payload(depth, new_head.0, &new_head.1),
                    journal,
                );
                self.orphan(&orphaned, at, journal);
            }
            CanonicalEvent::ReorgTooDeep { orphaned, new_head } => {
                // The new chain below its head is not followed: those heights
                // are skipped (the orphaned ones are counted as orphaned).
                self.resyncs += 1;
                let from = orphaned
                    .first()
                    .map_or(new_head.0, |(n, _)| *n)
                    .max(self.window_first_block);
                // Whatever was skipped, no curve the strategy holds is
                // exact: the resync tells it, or (nothing skipped) `I`.
                if new_head.0 > from {
                    self.blocks_skipped += new_head.0 - from;
                    self.put(
                        RecordKind::Reorg,
                        at,
                        resync_payload(from, new_head.0 - 1),
                        journal,
                    );
                } else {
                    self.put(
                        RecordKind::Canonical,
                        at,
                        crate::runtime_strategy::incomplete_payload(new_head.0),
                        journal,
                    );
                }
                self.orphan(&orphaned, at, journal);
            }
            CanonicalEvent::Gap { from, to } => {
                self.resyncs += 1;
                // Heights below the window were never part of it.
                let from = from.max(self.window_first_block);
                if to >= from {
                    self.blocks_skipped += to - from + 1;
                    self.put(RecordKind::Reorg, at, resync_payload(from, to), journal);
                }
            }
            // The C2 owner's reconciliation input; no G0.4 count.
            CanonicalEvent::NonceFact { .. } => {}
            // Phase I: the runtime acts on the complete block (strategy).
            CanonicalEvent::BlockComplete { .. } => {}
            CanonicalEvent::Failover { to, .. } => {
                self.active = to;
                self.failovers += 1;
            }
            CanonicalEvent::Fault(FollowerFault::Stalled { .. }) => {
                self.stalls += 1;
                self.state = "STALLED";
            }
            CanonicalEvent::Fault(fault) => {
                self.rpc_errors += 1;
                let limited = matches!(
                    fault,
                    FollowerFault::Request {
                        error: RequestError::RateLimited(_),
                        ..
                    }
                );
                self.rate_limited += u64::from(limited);
                self.state = if limited { "RATE_LIMITED" } else { "FAILING" };
            }
        }
    }

    /// Joins the launches due at `now` (all of them with `u64::MAX`).
    pub(super) fn join(&mut self, now: u64, core: &IngressCore, journal: &mut RuntimeJournal) {
        let head = self.head.map_or(0, |(n, _)| n);
        let horizon_us = self.join_horizon_us;
        let forgotten = |h: &[u8; 32], block_ms: u64| {
            core.maybe_forgotten(h, block_ms.saturating_mul(1_000).saturating_sub(horizon_us))
        };
        for j in self
            .ledger
            .join_due(now, head, |h| core.sighting(h), forgotten)
        {
            journal.record(
                RecordKind::Canonical,
                j.launch.block_seen_ns,
                canonical_launch_payload(&j),
            );
        }
    }

    pub(super) fn draft(&self, now: u64) -> Draft {
        use rs::canonical as e;
        let c = self.ledger.counters();
        let mut d = Draft::new(&e::SCHEMA);
        d.set(e::STATE, self.state)
            .set(e::BLOCKS, self.blocks)
            .set(e::REORGS, self.reorgs)
            .set(e::RESYNCS, self.resyncs)
            .set(e::RPC_ERRORS, self.rpc_errors)
            .set(e::RATE_LIMITED, self.rate_limited)
            .set(
                e::ENDPOINT,
                self.hosts.get(self.active).cloned().unwrap_or_default(),
            )
            .set(e::FAILOVERS, self.failovers)
            .set(e::STALLS, self.stalls)
            .set_opt(
                e::CONFIRM_ENDPOINT,
                self.confirm.as_ref().map(|(host, _)| host.clone()),
            )
            .set_opt(
                e::CONFIRM_HEAD,
                self.confirm.as_ref().and(self.second.head()),
            )
            .set_opt(
                e::CONFIRM_STATE,
                self.confirmation(wall_or_expired(observed_now()))
                    .map(|c| c.verdict.name()),
            )
            .set(e::CONFIRM_FAULTS, self.confirm_faults)
            .set(e::FLAP_LAUNCHES, c.flap_launches)
            .set(e::FOUR_LAUNCHES, c.four_launches)
            .set(e::FOUR_PENDING_DECODED, c.four_pending_decoded)
            .set(e::FOUR_PENDING_UNKNOWN, c.four_pending_unknown)
            .set(e::FOUR_PENDING_MISMATCH, c.four_pending_mismatch)
            .set(e::FOUR_PENDING_CANDIDATE, c.four_pending_candidate)
            .set(e::FOUR_SEEN_FULL_BEFORE, c.four_seen_full_before)
            .set(e::FOUR_SEEN_HASH_ONLY_BEFORE, c.four_seen_hash_only_before)
            .set(e::FOUR_SEEN_AFTER, c.four_seen_after)
            .set(e::FOUR_MISSED, c.four_missed)
            .set(e::FOUR_EVICTED, c.four_evicted)
            .set(e::FOUR_CLOCK_UNKNOWN, c.four_clock_unknown)
            .set(e::LAUNCH_HEAD_MISSING, self.launch_head_missing)
            .set(e::FOUR_ACTIONABLE, c.four_actionable)
            .set(e::REVERTED_LAUNCHES, self.reverted_launches)
            .set(e::ORPHANED_LAUNCHES, c.orphaned_launches)
            .set(e::SEEN_FULL_BEFORE, c.seen_full_before)
            .set(e::SEEN_HASH_ONLY_BEFORE, c.seen_hash_only_before)
            .set(e::SEEN_AFTER, c.seen_after)
            .set(e::MISSED, c.missed)
            .set(e::EVICTED, c.evicted)
            .set(e::EXTERNAL_BUYS, c.external_buys)
            .set(e::BUYS_UNKNOWN, c.buys_unknown)
            .set(e::CLOCK_UNKNOWN, c.clock_unknown)
            .set(e::TX_UNVERIFIABLE, c.tx_unverifiable)
            .set(e::TX_ANSWER_REFUSED, c.tx_answer_refused)
            .set(e::C1_SCOPE_LAUNCHES, c.c1_scope_launches)
            .set(e::C1_SCOPE_FULL_BEFORE, c.c1_scope_full_before)
            .set(e::BLOCKS_SKIPPED, self.blocks_skipped)
            .set(e::WAITING, self.ledger.waiting() as u64)
            .set(e::BACKFILL_BLOCKS, self.backfill_blocks)
            .set(e::BACKFILL_LAUNCHES, self.backfill_launches)
            .set(e::FOUR_DEPLOYMENT, self.four_deployment.as_str());
        if let Some((n, _)) = self.head {
            d.set(e::HEAD, n);
        }
        let lag = self
            .head_seen_unix_ms
            .zip(self.head_block_ms)
            .and_then(|(seen, block)| {
                i64::try_from(seen)
                    .ok()?
                    .checked_sub(i64::try_from(block).ok()?)
            });
        if let Some(lag) = lag {
            d.set(e::HEAD_FETCH_LAG_MS, lag);
        }
        if let Some(at) = self.head_seen_ns {
            d.set(e::HEAD_AGE_MS, now.saturating_sub(at) / 1_000_000);
        }
        d
    }
}
