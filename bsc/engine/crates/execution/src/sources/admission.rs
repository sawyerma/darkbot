//! The owner's admission of LIVE BUYs and exits: daily and campaign attempt slots,
//! the grant's and the campaign's spend, nonces and the admitted intents.

use super::*;

/// One UTC calendar day of the BUY attempt limit (the contract's day:
/// 00:00:00.000 UTC to the next).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuyDay {
    /// Days since 1970-01-01 UTC.
    pub index: u64,
    /// `YYYY-MM-DD`.
    pub utc: String,
}

/// Milliseconds of one UTC day.
pub const DAY_MS: u64 = 86_400_000;

/// The UTC day of `unix_ms`, days since 1970-01-01 (no text).
pub fn day_index(unix_ms: u64) -> u64 {
    unix_ms / DAY_MS
}

impl BuyDay {
    pub fn at(unix_ms: u64) -> BuyDay {
        let index = day_index(unix_ms);
        // Civil date from days since the epoch (proleptic Gregorian).
        let z = index as i64 + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + i64::from(month <= 2);
        BuyDay {
            index,
            utc: format!("{year:04}-{month:02}-{day:02}"),
        }
    }
}

/// The daily BUY attempts of one UTC day: taken slots and the limit in
/// force (an earlier, higher limit can leave more taken than allowed now).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuyUsage {
    pub day: BuyDay,
    pub used: u64,
    pub limit: u32,
    /// The owner generation the limit was read from.
    pub generation: u64,
}

impl BuyUsage {
    pub fn remaining(&self) -> u64 {
        u64::from(self.limit).saturating_sub(self.used)
    }
}

/// One taken slot: the attempt's place in its day.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuySlot {
    pub day: BuyDay,
    pub slot: u64,
    pub limit: u32,
}

pub(super) fn buy_day_dir(dir: &Path, day: &BuyDay) -> PathBuf {
    dir.join("buy-attempts").join(&day.utc)
}

/// The slot files of one day, each with the attempt that took it; a slot
/// file that does not decode counts as taken (never under-counted).
pub(super) fn buy_slots(day_dir: &Path) -> Result<Vec<(u64, Option<String>)>, String> {
    use darkbot_bsc_record::schema::bsc_runtime::buy_attempt_slot as bs;
    let entries = match fs::read_dir(day_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("buy attempts unreadable: {e}")),
    };
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let Some(slot) = name
            .to_string_lossy()
            .strip_suffix(".dbrec")
            .and_then(|n| n.parse::<u64>().ok())
        else {
            continue;
        };
        let attempt = fs::read(entry.path())
            .ok()
            .and_then(|b| decode(&bs::SCHEMA, &b).ok())
            .and_then(|r| r.text(bs::ATTEMPT_ID).ok().map(str::to_string));
        out.push((slot, attempt));
    }
    Ok(out)
}

/// The BUY attempts of the UTC day of `now_ms` against the limit in force
/// under the maximum `max`, read-only (for the status projection).
pub fn buy_usage(dir: &Path, now_ms: u64, max: u32) -> Result<BuyUsage, String> {
    let g = current_generation(dir)?;
    let day = BuyDay::at(now_ms);
    let used = buy_slots(&buy_day_dir(dir, &day))?.len() as u64;
    Ok(BuyUsage {
        day,
        used,
        limit: g.buy_attempts_per_day(max),
        generation: g.generation,
    })
}

/// The campaign's BUY attempts against its total limit in force.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TotalUsage {
    pub used: u64,
    pub limit: u32,
    /// The owner generation the limit was read from.
    pub generation: u64,
    /// The LIVE grant generation of every taken slot (0: none, e.g. G0.7).
    pub grants: Vec<u64>,
}

impl TotalUsage {
    pub fn remaining(&self) -> u64 {
        u64::from(self.limit).saturating_sub(self.used)
    }

    /// The attempts taken under the grant written at `generation`.
    pub fn used_by_grant(&self, generation: u64) -> u64 {
        self.grants.iter().filter(|g| **g == generation).count() as u64
    }
}

/// One taken slot of the campaign's total.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TotalSlot {
    pub slot: u64,
    pub limit: u32,
}

pub(super) fn total_dir(dir: &Path) -> PathBuf {
    dir.join("buy-attempts-total")
}

/// The campaign's slot files: slot, attempt and grant generation; a slot
/// that does not decode counts as taken by nobody (never under-counted).
pub(super) fn total_slots(dir: &Path) -> Result<Vec<(u64, Option<String>, u64)>, String> {
    use darkbot_bsc_record::schema::bsc_runtime::buy_attempt_total_slot as ts;
    let entries = match fs::read_dir(total_dir(dir)) {
        Ok(e) => e,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("total BUY attempts unreadable: {e}")),
    };
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let Some(slot) = entry
            .file_name()
            .to_string_lossy()
            .strip_suffix(".dbrec")
            .and_then(|n| n.parse::<u64>().ok())
        else {
            continue;
        };
        let record = fs::read(entry.path())
            .ok()
            .and_then(|b| decode(&ts::SCHEMA, &b).ok());
        let attempt = record
            .as_ref()
            .and_then(|r| r.text(ts::ATTEMPT_ID).ok().map(str::to_string));
        let grant = record
            .as_ref()
            .and_then(|r| r.u64(ts::GRANT_GENERATION).ok())
            .unwrap_or(0);
        out.push((slot, attempt, grant));
    }
    Ok(out)
}

#[cfg(test)]
thread_local! {
    /// Runs between a slot's limit read and its write (BSC3-004 test).
    pub(super) static BEFORE_SLOT_WRITE: std::cell::RefCell<Option<Box<dyn Fn()>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
thread_local! {
    /// Runs between the spend's read and its entry's write (budget test).
    pub(super) static AFTER_SPEND_READ: std::cell::RefCell<Option<Box<dyn Fn()>>> =
        const { std::cell::RefCell::new(None) };
}

pub(super) fn after_spend_read() {
    #[cfg(test)]
    AFTER_SPEND_READ.with(|h| {
        if let Some(h) = h.borrow().as_ref() {
            h();
        }
    });
}

pub(super) fn before_slot_write() {
    #[cfg(test)]
    BEFORE_SLOT_WRITE.with(|h| {
        if let Some(h) = h.borrow().as_ref() {
            h();
        }
    });
}

/// The daily slot `attempt_id` holds on any UTC day (BSC3-004: an attempt
/// tried again after midnight is the same attempt, never a second daily
/// slot).
pub(super) fn daily_slot_of(dir: &Path, attempt_id: &str) -> Result<Option<(BuyDay, u64)>, String> {
    use darkbot_bsc_record::schema::bsc_runtime::buy_attempt_slot as bs;
    let days = match fs::read_dir(dir.join("buy-attempts")) {
        Ok(e) => e,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("buy attempts unreadable: {e}")),
    };
    for day in days {
        let day = day.map_err(|e| e.to_string())?.path();
        // Review L4: a stray file (e.g. `.DS_Store`) is no day.
        if !day.is_dir() {
            continue;
        }
        for (slot, attempt) in buy_slots(&day)? {
            if attempt.as_deref() != Some(attempt_id) {
                continue;
            }
            let r = fs::read(day.join(format!("{slot:010}.dbrec")))
                .map_err(|e| e.to_string())
                .and_then(|b| decode(&bs::SCHEMA, &b).map_err(|e| e.to_string()))?;
            let e = |x: darkbot_bsc_record::RecordError| x.0;
            return Ok(Some((
                BuyDay {
                    index: r.u64(bs::DAY_INDEX).map_err(e)?,
                    utc: r.text(bs::DAY_UTC).map_err(e)?.to_string(),
                },
                slot,
            )));
        }
    }
    Ok(None)
}

/// BSC3-068: the slots of attempts no byte of which left (every ledger fact
/// of theirs WITHDRAWN), moved here whole from the days and the campaign's
/// slots (a rename each, never a rewrite or a removal): the slot
/// directories a count reads stay the slots that count; the returned ones
/// still bound the forward clock (BSC3-004).
pub(super) fn slot_returns_dir(dir: &Path) -> PathBuf {
    dir.join("buy-attempt-returns")
}

/// The latest time a campaign slot was taken at, a returned one too
/// (BSC3-004: slots are taken on a forward clock only).
pub(super) fn latest_slot_ms(dir: &Path) -> Result<Option<u64>, String> {
    use darkbot_bsc_record::schema::bsc_runtime::buy_attempt_total_slot as ts;
    let mut latest = None;
    for d in [total_dir(dir), slot_returns_dir(dir)] {
        let entries = match fs::read_dir(&d) {
            Ok(e) => e,
            Err(e) if e.kind() == ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("total BUY attempts unreadable: {e}")),
        };
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry.file_name().to_string_lossy().ends_with(".dbrec") {
                continue;
            }
            if let Some(t) = fs::read(entry.path())
                .ok()
                .and_then(|b| decode(&ts::SCHEMA, &b).ok())
                .and_then(|r| r.u64(ts::PROVENANCE_OBSERVED_UNIX_MS).ok())
            {
                latest = latest.max(Some(t));
            }
        }
    }
    Ok(latest)
}

/// The campaign's BUY attempts against the total limit in force under the
/// maximum `max`, read-only.
pub fn total_usage(dir: &Path, max: u32) -> Result<TotalUsage, String> {
    let g = current_generation(dir)?;
    let slots = total_slots(dir)?;
    Ok(TotalUsage {
        used: slots.len() as u64,
        limit: g.buy_attempts_total(max),
        generation: g.generation,
        grants: slots.into_iter().map(|(_, _, grant)| grant).collect(),
    })
}

/// Tests only: the owner state at `now_ms` (the generation, today's and the
/// campaign's BUY attempts); the LIVE gate reads it through `GateReads`.
#[cfg(test)]
pub fn live_inputs(
    dir: &Path,
    now_ms: u64,
    max: u32,
) -> Result<(Generation, BuyUsage, TotalUsage), String> {
    Ok((
        current_generation(dir)?,
        buy_usage(dir, now_ms, max)?,
        total_usage(dir, max)?,
    ))
}

/// Everything besides the generation and the qualification the LIVE gate
/// reads at `now_ms`, owned: one gate for a request's receipt, the runtime
/// and every tab. A part that does not read stays unreadable, and the gate
/// names it.
pub struct GateReads {
    pub boot: Option<String>,
    pub now_unix_ms: u64,
    pub daily: Option<BuyUsage>,
    pub total: Option<TotalUsage>,
    pub spent_wei: Option<u128>,
}

impl GateReads {
    /// The reads under the maximum `max` (`BSC_BUY_ATTEMPTS_TOTAL_MAX`).
    pub fn read(dir: &Path, g: &Generation, now_ms: u64, max: u32) -> GateReads {
        Self::read_holding(dir, g, now_ms, None, max)
    }

    /// [`Self::read`] besides the slots of the attempt `held` (BSC3-078:
    /// the running LIVE path's, [`reported_held_attempt`]), as the LIVE
    /// path's gate and the runtime's reconcile read them.
    pub fn read_holding(
        dir: &Path,
        g: &Generation,
        now_ms: u64,
        held: Option<&str>,
        max: u32,
    ) -> GateReads {
        let (daily, total) = usage_besides(dir, now_ms, held, max);
        GateReads {
            boot: boot_session().ok(),
            now_unix_ms: now_ms,
            daily,
            total,
            spent_wei: match &g.live_set_by {
                Some(by) => spent_by_grant(dir, by.generation).ok(),
                None => Some(0),
            },
        }
    }

    pub fn ctx<'a>(&'a self, qualification: &'a LiveQualification) -> LiveContext<'a> {
        LiveContext {
            boot: self.boot.as_deref(),
            now_unix_ms: self.now_unix_ms,
            daily: self.daily.as_ref(),
            total: self.total.as_ref(),
            spent_wei: self.spent_wei,
            qualification,
        }
    }
}

pub(super) fn spend_dir(dir: &Path) -> PathBuf {
    dir.join("live-spend")
}

/// BSC3-076: the kind of a lease's spend entry ([`Store::lease_live`]).
pub(super) const LEASE_KIND: &str = "BUY_LEASE";
/// The leases returned unused: their entries, moved here whole from the
/// spend (a rename, never a rewrite or a removal), spend and recover as
/// nothing; the spend directory a scan reads stays the entries that count.
pub(super) fn lease_returns_dir(dir: &Path) -> PathBuf {
    dir.join("live-lease-returns")
}

/// BSC3-076 review M1: a lease's daily slot is of the UTC day it is held
/// on: renewed after midnight it takes that day's (within that day's
/// limit); the campaign's slot stays its one under its attempt.
pub fn lease_daily_id(attempt: &str, now_ms: u64) -> String {
    format!("{attempt}-{}", BuyDay::at(now_ms).utc)
}

/// Audit E 3.3: whether a runtime status record is current at `now_ms`:
/// written within the number of its own rewrite intervals it names
/// (`status_stale_intervals`, the runtime's `BSC_RUNTIME_STATUS_STALE_INTERVALS`);
/// a record without either is never current.
pub fn status_is_current(r: &darkbot_bsc_record::Record, now_ms: u64) -> bool {
    use darkbot_bsc_record::schema::bsc_runtime::runtime_status as rs;
    match (
        r.u64(rs::PROVENANCE_OBSERVED_UNIX_MS),
        r.u64(rs::STATUS_INTERVAL_MS),
        r.opt_u64(rs::STATUS_STALE_INTERVALS),
    ) {
        (Ok(written), Ok(interval), Ok(Some(intervals))) => {
            now_ms.saturating_sub(written) <= intervals.saturating_mul(interval)
        }
        _ => false,
    }
}

/// BSC3-078: the attempt the running LIVE path's next C3 BUY takes, as its
/// status record `status` (`<rundir>/bsc-runtime/status.dbrec`) reports it
/// for the grant of `g`: the path armed under that grant, the record
/// current ([`status_is_current`]). Else none: every slot counts.
pub fn reported_held_attempt(status: &Path, g: &Generation, now_ms: u64) -> Option<String> {
    use darkbot_bsc_record::schema::bsc_runtime::runtime_status as rs;
    let grant = g.live_set_by.as_ref()?.generation;
    let r = darkbot_bsc_record::decode(&rs::SCHEMA, &fs::read(status).ok()?).ok()?;
    if !status_is_current(&r, now_ms) {
        return None;
    }
    let live = r.nested_all(rs::LIVE).ok()?.into_iter().next()?;
    let armed = live.text(rs::live::STATE).ok()? == "ARMED"
        && live.opt_u64(rs::live::GRANT_GENERATION).ok()? == Some(grant);
    armed
        .then(|| live.opt_b256(rs::live::LEASE_ATTEMPT).ok().flatten())
        .flatten()
        .map(|a| hex32(&a))
}

/// The daily and campaign usage at `now_ms` besides the slots of the
/// attempt `held` (BSC3-076 review H1: a lease's own attempt, already
/// taken, never refuses itself): its slot of this UTC day and its campaign
/// slot do not count. The LIVE path's gate and the runtime's reconcile read
/// the owner's usage through this one function.
pub fn usage_besides(
    dir: &Path,
    now_ms: u64,
    held: Option<&str>,
    max: u32,
) -> (Option<BuyUsage>, Option<TotalUsage>) {
    let mut daily = buy_usage(dir, now_ms, max).ok();
    let mut total = total_usage(dir, max).ok();
    let Some(attempt) = held else {
        return (daily, total);
    };
    let today = lease_daily_id(attempt, now_ms);
    if let Some(d) = daily.as_mut() {
        let mine = buy_slots(&buy_day_dir(dir, &d.day))
            .map(|slots| {
                slots
                    .iter()
                    .filter(|(_, a)| a.as_deref() == Some(&today))
                    .count()
            })
            .unwrap_or(0) as u64;
        d.used = d.used.saturating_sub(mine);
    }
    if let (Some(t), Ok(slots)) = (total.as_mut(), total_slots(dir)) {
        for (_, _, grant) in slots
            .iter()
            .filter(|(_, a, _)| a.as_deref() == Some(attempt))
        {
            t.used = t.used.saturating_sub(1);
            if let Some(at) = t.grants.iter().position(|g| g == grant) {
                t.grants.remove(at);
            }
        }
    }
    (daily, total)
}

/// BSC3-076: the owner's state as a lease saw it: any request applied
/// since (a new generation, a revoke, a switch off, a limit) changes it, and
/// the C3 BUY under the lease is refused at its launch (no attempt taken).
/// One `lstat` of the generations directory: no read on the entry path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnerMark {
    pub(super) dev: u64,
    pub(super) ino: u64,
    pub(super) mtime: (i64, i64),
    pub(super) ctime: (i64, i64),
    pub(super) nlink: u64,
    pub(super) size: u64,
}

pub(super) fn wei_u128(word: [u8; 32]) -> Option<u128> {
    if word[..16].iter().any(|b| *b != 0) {
        return None;
    }
    Some(u128::from_be_bytes(word[16..].try_into().ok()?))
}

/// What the grant written at `grant_generation` has spent: per (chain,
/// sender, nonce) the most of its admitted intents may lose (one of them
/// lands, whichever), and every intent without its sender and chain (an
/// entry written before them) in full. Every entry counts, whatever came
/// of it; none is ever released. An entry that does not decode makes the
/// spend unreadable (never under-counted).
pub fn spent_by_grant(dir: &Path, grant_generation: u64) -> Result<u128, String> {
    spend_of_grant(&admitted_intents(dir)?, grant_generation)?.wei()
}

/// The owner's LIVE spend lock held ([`Store::spend_lock`]), unlocked
/// explicitly when dropped: the lock lives on the file's open description,
/// which a child forked meanwhile holds a copy of until its exec; closing
/// this copy alone would leave it locked (a Linux CI flake: "held by
/// another admission" of nobody's).
pub(crate) struct SpendGuard {
    pub(super) file: fs::File,
}

impl Drop for SpendGuard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// [`spent_by_grant`] of the admitted intents `admitted`, by nonce.
pub(super) fn spend_of_grant(
    admitted: &[Admitted],
    grant_generation: u64,
) -> Result<NonceCosts, String> {
    spend_of(admitted, |a| a.grant_generation == grant_generation)
}

/// R4: what the campaign has spent, every grant's admissions as
/// [`spent_by_grant`] counts one grant's.
pub fn spent_total(dir: &Path) -> Result<u128, String> {
    spend_of(&admitted_intents(dir)?, |_| true)?.wei()
}

/// The admitted intents `admitted` that `keep` takes, by nonce.
pub(super) fn spend_of(
    admitted: &[Admitted],
    keep: impl Fn(&Admitted) -> bool,
) -> Result<NonceCosts, String> {
    let mut book = NonceCosts::default();
    for a in admitted.iter().filter(|a| keep(a)) {
        book.add(a.key(), U256::from(a.max_wei))
            .ok_or_else(|| format!("{LIVE_SPEND_UNREADABLE}: the spend overflows"))?;
    }
    Ok(book)
}

/// The account and chain a LIVE transaction's nonce is of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NonceOrigin {
    pub chain_id: u64,
    pub sender: [u8; 20],
}

/// One (chain, sender, nonce): its transactions are alternatives, at most
/// one of them lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NonceKey {
    pub origin: NonceOrigin,
    pub nonce: u64,
}

/// What transactions may cost when the alternatives of one nonce count
/// once, at the most of them; a cost without its nonce's sender and chain
/// counts alone, in full (never grouped by its nonce alone). Nothing
/// counted is ever taken back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NonceCosts {
    groups: std::collections::BTreeMap<NonceKey, U256>,
    alone: U256,
}

impl NonceCosts {
    /// Counts `wei` under `key` (none: alone). None: the sum overflows.
    pub fn add(&mut self, key: Option<NonceKey>, wei: U256) -> Option<()> {
        match key {
            Some(key) => {
                let most = self.groups.entry(key).or_default();
                *most = (*most).max(wei);
            }
            None => self.alone = self.alone.checked_add(wei)?,
        }
        Some(())
    }

    /// The most counted under `key` so far.
    pub fn of(&self, key: &NonceKey) -> U256 {
        self.groups.get(key).copied().unwrap_or_default()
    }

    /// What counting `wei` under `key` adds to [`Self::total`]: only what
    /// it is above the most of its alternatives.
    pub fn increase(&self, key: Option<NonceKey>, wei: U256) -> U256 {
        match key {
            Some(key) => wei.saturating_sub(self.of(&key)),
            None => wei,
        }
    }

    /// Every group's most and every cost alone (none: it overflows).
    pub fn total(&self) -> Option<U256> {
        self.groups
            .values()
            .try_fold(self.alone, |sum, wei| sum.checked_add(*wei))
    }

    pub(super) fn wei(&self) -> Result<u128, String> {
        self.total()
            .and_then(|t| u128::try_from(t).ok())
            .ok_or_else(|| format!("{LIVE_SPEND_UNREADABLE}: the spend overflows"))
    }
}

/// What a LIVE intent is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveIntentKind {
    Buy,
    Approve,
    Sell,
    /// REV.1.0 R3b: the operator's nonce burn (a plain transfer of nothing
    /// to the buyer itself): admitted as an exit step is (no slot, no
    /// input), of no token and no lot.
    Burn,
}

impl LiveIntentKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Buy => "BUY",
            Self::Approve => "APPROVE",
            Self::Sell => "SELL",
            Self::Burn => "BURN",
        }
    }

    /// An exit step of a lot (its approval or SELL): never a BUY or a
    /// burn.
    pub fn exit_step(self) -> bool {
        matches!(self, Self::Approve | Self::Sell)
    }
}

/// One LIVE intent the executor asks the owner to admit, immediately
/// before its dispatch (plan 28.10.).
#[derive(Clone, Copy, Debug)]
pub struct LiveIntent<'a> {
    /// The signed transaction's hash, hex: a BUY's attempt id.
    pub id: &'a str,
    pub component: ComponentId,
    pub kind: LiveIntentKind,
    pub input_wei: u128,
    /// Input and gas at the signed gas price and limit.
    pub max_wei: u128,
    /// The token it trades and the sender's nonce it is signed at: with
    /// the admission durable before any byte can leave (BSC3-024).
    pub token: [u8; 20],
    pub nonce: u64,
    /// The sender and chain of that nonce (the lane's): the alternatives
    /// of one nonce spend once.
    pub origin: NonceOrigin,
    /// The last block its bundle may be included in (BSC3-048).
    pub max_block: u64,
    /// A BUY: the most the exits of the sender's open positions and of its
    /// own (approval and SELL at the signed gas) may still cost; the
    /// grant's spend keeps room for it (BSC3-057). An exit step: zero.
    pub exit_reserve_wei: u128,
    /// SC4 d: the venue it trades on (none: a nonce burn).
    pub venue: Option<VenueId>,
}

/// One LIVE intent the owner admitted, as its entry holds it: the
/// write-ahead record of a transaction whose bytes may have left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Admitted {
    pub tx_hash: [u8; 32],
    pub kind: LiveIntentKind,
    pub component: ComponentId,
    pub grant_generation: u64,
    pub input_wei: u128,
    pub max_wei: u128,
    /// Absent in an entry written before BSC3-024.
    pub token: Option<[u8; 20]>,
    pub nonce: Option<u64>,
    /// Absent in an entry written before BSC3-048: the window is unknown.
    pub max_block: Option<u64>,
    /// Absent in an entry written before its sender and chain were: it
    /// counts alone.
    pub origin: Option<NonceOrigin>,
    /// BSC3-076: a lease of the next C3 BUY ([`Store::lease_live`]): no
    /// transaction of its own, its token none (zero), its id no hash.
    pub lease: bool,
    /// SC4 d: the venue it trades on (absent before v3: Flap's).
    pub venue: Option<VenueId>,
}

impl Admitted {
    /// The nonce it is one of the alternatives of (none: not known whole).
    pub fn key(&self) -> Option<NonceKey> {
        Some(NonceKey {
            origin: self.origin?,
            nonce: self.nonce?,
        })
    }
}

/// Every LIVE intent the owner ever admitted, in no particular order. An
/// entry that does not decode refuses them all (never under-counted).
pub fn admitted_intents(dir: &Path) -> Result<Vec<Admitted>, String> {
    use darkbot_bsc_record::schema::bsc_runtime::live_spend as ls;
    let entries = match fs::read_dir(spend_dir(dir)) {
        Ok(e) => e,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{LIVE_SPEND_UNREADABLE}: {e}")),
    };
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("{LIVE_SPEND_UNREADABLE}: {e}"))?;
        if !entry.file_name().to_string_lossy().ends_with(".dbrec") {
            continue;
        }
        let bad = || format!("{LIVE_SPEND_UNREADABLE}: {}", entry.path().display());
        let bytes = fs::read(entry.path()).map_err(|_| bad())?;
        let (r, v2, v3) = match decode(&ls::SCHEMA, &bytes) {
            Ok(r) => (r, true, true),
            Err(_) => match decode(&ls::V2, &bytes) {
                Ok(r) => (r, true, false),
                Err(_) => (decode(&ls::V1, &bytes).map_err(|_| bad())?, false, false),
            },
        };
        let venue = match v3 {
            true => match r.opt_text(ls::VENUE).map_err(|_| bad())? {
                Some(v) => Some(VenueId::parse(v).ok_or_else(bad)?),
                None => None,
            },
            false => None,
        };
        let (kind, lease) = match r.text(ls::KIND).map_err(|_| bad())? {
            "BUY" => (LiveIntentKind::Buy, false),
            LEASE_KIND => (LiveIntentKind::Buy, true),
            "APPROVE" => (LiveIntentKind::Approve, false),
            "SELL" => (LiveIntentKind::Sell, false),
            "BURN" => (LiveIntentKind::Burn, false),
            _ => return Err(bad()),
        };
        let id = r
            .text(ls::INTENT_ID)
            .ok()
            .and_then(parse_hex32)
            .ok_or_else(bad)?;
        out.push(Admitted {
            tx_hash: id,
            kind,
            component: r
                .text(ls::COMPONENT)
                .ok()
                .and_then(ComponentId::parse)
                .ok_or_else(bad)?,
            grant_generation: r.u64(ls::GRANT_GENERATION).map_err(|_| bad())?,
            input_wei: r
                .u256(ls::INPUT_WEI)
                .ok()
                .and_then(wei_u128)
                .ok_or_else(bad)?,
            max_wei: r
                .u256(ls::MAX_WEI)
                .ok()
                .and_then(wei_u128)
                .ok_or_else(bad)?,
            token: r.opt_address(ls::TOKEN).map_err(|_| bad())?,
            nonce: r.opt_u64(ls::NONCE).map_err(|_| bad())?,
            max_block: r.opt_u64(ls::MAX_BLOCK).map_err(|_| bad())?,
            origin: match v2 {
                true => Some(NonceOrigin {
                    chain_id: r.u64(ls::CHAIN_ID).map_err(|_| bad())?,
                    sender: r.address(ls::SENDER).map_err(|_| bad())?,
                }),
                false => None,
            },
            lease,
            venue,
        });
        // An entry that names its sender and chain names its nonce too.
        if out
            .last()
            .is_some_and(|a| a.origin.is_some() && a.nonce.is_none())
        {
            return Err(bad());
        }
    }
    Ok(out)
}

/// Proof that the owner's LIVE gate was clear, at one moment, for the
/// runtime, the grant's venue and the components armed: what arms a LIVE
/// end and loads its key. Only [`Store::live_now`] makes one; every intent
/// is admitted again under it ([`Store::admit_live`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveNow {
    pub(super) grant: LiveGrant,
    pub(super) grant_generation: u64,
    pub(super) at_ms: u64,
    pub(super) sources_live: Vec<SourceId>,
    pub(super) presence: Option<[u8; 65]>,
}

impl LiveNow {
    pub fn grant(&self) -> &LiveGrant {
        &self.grant
    }

    /// REV.1.0 R9 (BSC3-001): who signed the grant's presence, if it is
    /// signed and the signature recovers ([`presence_signer`]).
    pub fn presence_signer(&self) -> Option<[u8; 20]> {
        presence_signer(&self.grant, self.grant_generation, self.presence.as_ref()?)
    }

    /// The grant's sources effective LIVE now (requested LIVE and in the
    /// scope, the gate clear but for what only a BUY needs): only their
    /// sighting may trigger a LIVE BUY (PHASENPLAN v9 §2.3; a granted
    /// source the operator holds in SHADOW triggers nothing).
    pub fn sources_live(&self) -> &[SourceId] {
        &self.sources_live
    }

    /// The owner generation that wrote the grant.
    pub fn grant_generation(&self) -> u64 {
        self.grant_generation
    }

    pub fn at_ms(&self) -> u64 {
        self.at_ms
    }
}

/// What the owner admitted: a BUY's daily and campaign slot, and what the
/// grant has spent with this intent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveAdmission {
    pub slots: Option<(BuySlot, TotalSlot)>,
    pub spent_wei: u128,
}

/// [`Store::consume_buy_attempt`] on the canonical owner state.
/// Its bounds are those of the deployed runtime's configuration
/// ([`canonical_config_path`]), the one that runs on this owner state.
pub fn consume_buy_attempt_here(attempt_id: &str, now_ms: u64) -> Result<BuySlot, String> {
    let unreadable = |e: String| format!("BUY_ATTEMPT_LIMIT_UNREADABLE: {e}");
    let dir = canonical_state_dir().map_err(unreadable)?;
    let limits =
        OwnerLimits::at(&canonical_config_path().map_err(unreadable)?).map_err(unreadable)?;
    Store::open(dir, limits)?.consume_buy_attempt(attempt_id, now_ms)
}
