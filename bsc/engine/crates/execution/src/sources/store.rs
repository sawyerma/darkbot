//! The owner's durable state: the store of generations and receipts, its private
//! directories and durable writes, the boot session and the switch checks.

use super::*;

/// The owner's state directory (`generations/`, `receipts/`).
pub struct Store {
    dir: PathBuf,
    /// The owner of its directories (BSC3-003): every file read is theirs.
    owner: u32,
    /// Its bounds (generations kept, the attempts' maximum); none for the
    /// requests that only reduce authority ([`Store::open_reducing`]).
    limits: Option<OwnerLimits>,
    /// The host boot session every write is stamped with.
    boot: Result<String, String>,
    /// R4: the campaign's cumulative budget its BUYs are admitted within
    /// (the runtime's `BSC_EXEC_LIVE_SPEND_TOTAL_WEI`); a store opened
    /// without it (a CLI's, a test's) bounds no campaign.
    campaign_spend_max: Option<u128>,
}

/// A request ID is a single safe path component (the gateway's UUIDs fit).
pub(crate) fn safe_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// Publishes `bytes` at `path` only if nothing is there: written and synced
/// in a private temporary file first, then hard-linked into place (fails
/// with `AlreadyExists` if another writer won), so a reader never sees a
/// partial record.
pub(crate) fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    write_new_as(path, bytes, Durability::Full)
}

/// How a create-new write reaches the medium (BSC3-023).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Durability {
    /// Durable when the write returns: the file and its directory each
    /// flushed to the medium.
    Full,
    /// Handed to the drive only; durable once the caller has flushed the
    /// device ([`flush_device`]) before anything depends on it.
    Handed,
}

/// Tests: what reached the drive on this thread, in order (BSC3-023).
#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Medium {
    /// A file (named by its final path) or a directory flushed on its own.
    Full(PathBuf),
    /// A file's content handed to the drive, before anything after it.
    Ordered(PathBuf),
    /// A new file linked into place.
    Linked(PathBuf),
    /// A directory handed to the drive.
    Handed(PathBuf),
    /// The device flushed through this directory, for these written ones.
    Flush(PathBuf, Vec<PathBuf>),
    /// A file's content synced while its path was already published.
    Published(PathBuf),
}

#[cfg(test)]
thread_local! {
    pub(super) static MEDIUM: std::cell::RefCell<Vec<Medium>> = const { std::cell::RefCell::new(Vec::new()) };
    /// Tests: the next device flush on this thread fails.
    pub(super) static FLUSH_FAILS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn medium(event: Medium) {
    MEDIUM.with(|m| m.borrow_mut().push(event));
}

/// A new file's content (named by its final `path`) synced: under
/// `Handed` ordered before the link that publishes it.
fn sync_content_as(file: &fs::File, path: &Path, durability: Durability) -> std::io::Result<()> {
    #[cfg(test)]
    if path.exists() {
        medium(Medium::Published(path.to_path_buf()));
    }
    #[cfg(not(test))]
    let _ = path;
    match durability {
        Durability::Full => {
            #[cfg(test)]
            medium(Medium::Full(path.to_path_buf()));
            file.sync_all()
        }
        Durability::Handed => {
            #[cfg(test)]
            medium(Medium::Ordered(path.to_path_buf()));
            darkbot_bsc_journal::durable::ordered(file)
        }
    }
}

fn sync_dir_as(dir: &Path, durability: Durability) -> std::io::Result<()> {
    let file = fs::File::open(dir)?;
    match durability {
        Durability::Full => {
            #[cfg(test)]
            medium(Medium::Full(dir.to_path_buf()));
            file.sync_all()
        }
        Durability::Handed => {
            #[cfg(test)]
            medium(Medium::Handed(dir.to_path_buf()));
            darkbot_bsc_journal::durable::handed(&file)
        }
    }
}

/// Puts every write handed to the drive before it on the medium
/// (BSC3-023), through the directory `dir`. The flush covers `dir`'s
/// device only: every directory in `written` must be on it.
pub(super) fn flush_device(dir: &Path, written: &[PathBuf]) -> std::io::Result<()> {
    let device = fs::metadata(dir)?.dev();
    for w in written {
        if fs::metadata(w)?.dev() != device {
            return Err(std::io::Error::other(format!(
                "{} is not on the device of {}",
                w.display(),
                dir.display()
            )));
        }
    }
    #[cfg(test)]
    {
        medium(Medium::Flush(dir.to_path_buf(), written.to_vec()));
        if FLUSH_FAILS.with(|f| f.replace(false)) {
            return Err(std::io::Error::other("the device flush failed (test)"));
        }
    }
    darkbot_bsc_journal::durable::flush(&fs::File::open(dir)?)
}

fn write_new_as(path: &Path, bytes: &[u8], durability: Durability) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut nonce = [0u8; 8];
    fs::File::open("/dev/urandom")
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut nonce))?;
    let tmp = dir.join(format!(
        ".tmp-{}-{}",
        std::process::id(),
        nonce.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ));
    let written = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(bytes)?;
        sync_content_as(&f, path, durability)?;
        fs::hard_link(&tmp, path)?;
        #[cfg(test)]
        medium(Medium::Linked(path.to_path_buf()));
        Ok::<(), std::io::Error>(())
    })();
    let _ = fs::remove_file(&tmp);
    written?;
    sync_dir_as(dir, durability)
}

/// Why a directory or file of a private store is refused.
#[derive(Debug)]
pub(crate) enum DirRefusal {
    Io(String),
    /// Another user can read or change it.
    Exposed(String),
}

impl std::fmt::Display for DirRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DirRefusal::Io(e) | DirRefusal::Exposed(e) => f.write_str(e),
        }
    }
}

/// `dir` is owner-only and no directory on its way (as written and as
/// resolved) can be changed by another user: each belongs to `dir`'s owner
/// or root and is not writable by group or others unless sticky; a link on
/// the written path is the owner's or root's (its target is on the
/// resolved path). Returns that owner: a process that opens a file inside
/// an owner-only directory is its owner (or root). BSC3-003/067.
pub(crate) fn private_directory(dir: &Path) -> Result<u32, DirRefusal> {
    let io = |e: std::io::Error| DirRefusal::Io(format!("{}: {e}", dir.display()));
    let real = fs::canonicalize(dir).map_err(io)?;
    let meta = fs::metadata(&real).map_err(io)?;
    if !meta.is_dir() || meta.mode() & 0o077 != 0 {
        return Err(DirRefusal::Exposed(format!(
            "{} is accessible to others",
            dir.display()
        )));
    }
    let owner = meta.uid();
    for a in dir
        .ancestors()
        .chain(real.ancestors())
        .filter(|a| !a.as_os_str().is_empty())
    {
        let m = fs::symlink_metadata(a).map_err(io)?;
        let others_write =
            !m.file_type().is_symlink() && m.mode() & 0o022 != 0 && m.mode() & 0o1000 == 0;
        if (m.uid() != owner && m.uid() != 0) || others_write {
            return Err(DirRefusal::Exposed(format!(
                "{} can be changed by another user",
                a.display()
            )));
        }
    }
    Ok(owner)
}

/// The bytes of the store file `path` whose directory belongs to `owner`:
/// the name checked (a regular file, no link), the file opened once and
/// checked on its descriptor (the same inode, `owner`'s, writable by no
/// one else). Its link count is not: [`write_new`] publishes by a second
/// name for a moment, and no other user can link into an owner-only
/// directory. BSC3-003.
pub(crate) fn read_private(path: &Path, owner: u32) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let named = fs::symlink_metadata(path)?;
    let mut file = fs::File::open(path)?;
    let opened = file.metadata()?;
    if !named.is_file()
        || (opened.dev(), opened.ino()) != (named.dev(), named.ino())
        || opened.uid() != owner
        || opened.mode() & 0o022 != 0
    {
        return Err(std::io::Error::new(
            ErrorKind::PermissionDenied,
            "not a private file of the store's owner",
        ));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Creates `d` (0700) unless it exists; a new directory is made durable in
/// its parent, so a slot or generation written into it survives power loss.
pub(super) fn create_durable_dir(d: &Path) -> Result<(), String> {
    match fs::DirBuilder::new().mode(0o700).create(d) {
        Ok(()) => d
            .parent()
            .map_or(Ok(()), |p| fs::File::open(p).and_then(|f| f.sync_all()))
            .map_err(|e| format!("{}: {e}", d.display())),
        Err(e) if e.kind() == ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(format!("{}: {e}", d.display())),
    }
}

/// The host's boot session, read once per process: macOS
/// `kern.bootsessionuuid`, Linux `/proc/sys/kernel/random/boot_id`. It
/// changes with every boot, so an ENABLED switch never outlives a restart.
pub fn boot_session() -> Result<String, String> {
    static BOOT: OnceLock<Result<String, String>> = OnceLock::new();
    BOOT.get_or_init(|| {
        let raw = if cfg!(target_os = "macos") {
            std::process::Command::new("/usr/sbin/sysctl")
                .env_clear()
                .args(["-n", "kern.bootsessionuuid"])
                .output()
                .map_err(|e| format!("boot session: {e}"))
                .and_then(|o| {
                    if o.status.success() {
                        Ok(String::from_utf8_lossy(&o.stdout).into_owned())
                    } else {
                        Err("boot session: sysctl refused".into())
                    }
                })?
        } else {
            fs::read_to_string("/proc/sys/kernel/random/boot_id")
                .map_err(|e| format!("boot session: {e}"))?
        };
        let boot = raw.trim();
        if boot.is_empty() || boot.len() > 128 || !boot.bytes().all(|b| b.is_ascii_graphic()) {
            return Err("boot session: not a host boot identifier".into());
        }
        Ok(boot.to_string())
    })
    .clone()
}

/// The real UID of a `/proc/self/status` text (`Uid:` real, effective,
/// saved, filesystem).
pub(super) fn real_uid_of_status(status: &str) -> Option<&str> {
    status
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))
        .and_then(|l| l.split_whitespace().next())
}

/// The home of `uid` in a `/etc/passwd` text (seven fields, the shell may
/// be empty).
pub(super) fn home_of_passwd<'a>(passwd: &'a str, uid: &str) -> Option<&'a str> {
    passwd.lines().find_map(|l| {
        let f: Vec<&str> = l.split(':').collect();
        (f.len() >= 7 && f[2] == uid).then_some(f[5])
    })
}

/// The home of a `dscacheutil -q user` answer.
pub(super) fn home_of_dscache(entry: &str) -> Option<&str> {
    entry.lines().find_map(|l| l.strip_prefix("dir: "))
}

/// The account home of this process's real user from the user database
/// (macOS directory services, Linux `/etc/passwd` only: an account known
/// only through NSS is refused, fail-closed), read once; never `$HOME`.
pub fn account_home() -> Result<PathBuf, String> {
    static HOME: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    HOME.get_or_init(|| {
        let run = |program: &str, args: &[&str]| {
            std::process::Command::new(program)
                .env_clear()
                .args(args)
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                .ok_or(format!("account home: {program} refused"))
        };
        // BSC3-007: the real UID on both systems (`id -ru`; the first of
        // the four `Uid:` fields), never the effective one.
        let home = if cfg!(target_os = "macos") {
            let uid = run("/usr/bin/id", &["-ru"])?;
            let entry = run(
                "/usr/bin/dscacheutil",
                &["-q", "user", "-a", "uid", uid.trim()],
            )?;
            home_of_dscache(&entry).map(str::to_string)
        } else {
            let status = fs::read_to_string("/proc/self/status")
                .map_err(|e| format!("account home: {e}"))?;
            let uid = real_uid_of_status(&status).ok_or("account home: no real uid")?;
            let passwd =
                fs::read_to_string("/etc/passwd").map_err(|e| format!("account home: {e}"))?;
            home_of_passwd(&passwd, uid).map(str::to_string)
        }
        .filter(|h| h.starts_with('/'))
        .ok_or("account home: not in the user database")?;
        Ok(PathBuf::from(home.trim()))
    })
    .clone()
}

/// The owner state darkbotctl's controller keeps (`$HOME/.darkbot/run`,
/// its RUNDIR, `bsc-sources`): the one place a Mainnet signing or sending
/// path reads its switches and its daily BUY limit from. It lies under the
/// account home from the user database; a `HOME` naming another directory
/// is refused, so no self-made state elsewhere (written by the owner CLI
/// with another `--state`) is ever honored instead of the one the GUI and
/// `darkbotctl bsc source show` display.
pub fn canonical_state_dir() -> Result<PathBuf, String> {
    let home = account_home()?;
    match std::env::var_os("HOME").filter(|h| !h.is_empty()) {
        Some(h) if Path::new(&h) != home => Err(format!(
            "HOME is not the account home {}: the switches are read only from the account's owner state",
            home.display()
        )),
        _ => Ok(home.join(".darkbot/run/bsc-sources")),
    }
}

/// The deployed runtime's configuration (`darkbotctl`'s
/// `$APP_HOME/bsc-source`), under the account-home rule of
/// [`canonical_state_dir`]: the bounds of the owner state there.
pub fn canonical_config_path() -> Result<PathBuf, String> {
    canonical_state_dir()?;
    Ok(account_home()?
        .join("Library/Application Support/DarkBot/bsc-source")
        .join(RUNTIME_CONFIG_PATH))
}

/// The generation on file in `dir` without creating anything: the initial
/// one (everything DISABLED) when there is none.
/// The owner of `dir`'s `generations`: a real, owner-only directory
/// (BSC3-003; the way to it is checked when a store opens and before a
/// switch is required).
fn generations_owner(dir: &Path) -> Result<u32, String> {
    let m = fs::symlink_metadata(dir.join("generations")).map_err(|e| e.to_string())?;
    if !m.is_dir() || m.mode() & 0o077 != 0 {
        return Err("generations is not a private real directory".into());
    }
    Ok(m.uid())
}

pub fn current_generation(dir: &Path) -> Result<Generation, String> {
    match fs::symlink_metadata(dir.join("generations")) {
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Generation::initial()),
        Err(e) => return Err(e.to_string()),
        Ok(_) => {}
    }
    let owner = generations_owner(dir)?;
    match highest_generation(dir)? {
        None => Ok(Generation::initial()),
        Some(n) => {
            let bytes = read_private(
                &dir.join("generations").join(format!("{n:020}.dbrec")),
                owner,
            )
            .map_err(|e| e.to_string())?;
            let g = Generation::decode(&bytes)?;
            if g.generation != n {
                return Err(format!(
                    "generation file {n} holds generation {}",
                    g.generation
                ));
            }
            Ok(g)
        }
    }
}

/// Refused unless every switch in `needed` is ENABLED in the generation on
/// file in `dir` and was written in the boot session `boot`. Returns the
/// generation checked. A missing, unreadable or foreign state refuses: every
/// switch is DISABLED then.
pub fn require_enabled(dir: &Path, boot: &str, needed: &[SwitchId]) -> Result<u64, String> {
    // BSC3-003: the state of a store another user can change counts nothing
    // (a missing one is everything DISABLED below).
    if dir.join("generations").exists() {
        private_directory(&dir.join("generations"))
            .map_err(|e| format!("CONTROL_STATE_UNREADABLE: {e}; every switch DISABLED"))?;
    }
    let g = current_generation(dir)
        .map_err(|e| format!("CONTROL_STATE_UNREADABLE: {e}; every switch DISABLED"))?;
    let off: Vec<String> = needed
        .iter()
        .filter_map(|id| match g.switch_now(*id, Some(boot)) {
            Effective::Enabled => None,
            other => Some(format!(
                "{} {}{}",
                id.name(),
                other.name(),
                other
                    .reason()
                    .map(|r| format!(" ({r})"))
                    .unwrap_or_default()
            )),
        })
        .collect();
    if off.is_empty() {
        Ok(g.generation)
    } else {
        Err(format!(
            "CONTROL_DISABLED at generation {}: {}",
            g.generation,
            off.join(", ")
        ))
    }
}

/// [`require_enabled`] on the canonical owner state of this boot session.
pub fn require_enabled_here(needed: &[SwitchId]) -> Result<u64, String> {
    let dir = canonical_state_dir()
        .map_err(|e| format!("CONTROL_STATE_UNREADABLE: {e}; every switch DISABLED"))?;
    let boot = boot_session().map_err(|e| format!("{e}; every switch DISABLED"))?;
    require_enabled(&dir, &boot, needed)
}

impl Store {
    pub fn open(dir: impl Into<PathBuf>, limits: OwnerLimits) -> Result<Self, String> {
        Self::open_in_session(dir, limits, boot_session())
    }

    /// A store for the requests that only reduce authority or put the
    /// state on file (init, the restart reset, a revocation; review E3 M1:
    /// a stop never depends on a readable configuration): it prunes
    /// nothing, and everything that needs the bounds is refused.
    pub fn open_reducing(dir: impl Into<PathBuf>) -> Result<Self, String> {
        Self::open_with(dir.into(), None, boot_session())
    }

    /// A store whose writes are stamped with the boot session `boot`
    /// (tests simulate a restart with another session).
    pub fn open_in_session(
        dir: impl Into<PathBuf>,
        limits: OwnerLimits,
        boot: Result<String, String>,
    ) -> Result<Self, String> {
        Self::open_with(dir.into(), Some(limits), boot)
    }

    /// The bounds, or why there are none.
    fn bounds(&self) -> Result<OwnerLimits, String> {
        self.limits
            .ok_or_else(|| "the owner's bounds are not configured (--config)".into())
    }

    fn open_with(
        dir: PathBuf,
        limits: Option<OwnerLimits>,
        boot: Result<String, String>,
    ) -> Result<Self, String> {
        let mut owner = None;
        for d in [dir.clone(), dir.join("generations"), dir.join("receipts")] {
            create_durable_dir(&d)?;
            if !fs::symlink_metadata(&d).is_ok_and(|m| m.is_dir()) {
                return Err(format!("{} is not a real directory", d.display()));
            }
            // BSC3-003: an existing directory is taken only owner-only, of
            // one owner, on a way no other user can change.
            let o = private_directory(&d).map_err(|r| r.to_string())?;
            if owner.is_some_and(|x| x != o) {
                return Err(format!("{} has another owner", d.display()));
            }
            owner = Some(o);
        }
        let owner = owner.ok_or("no store directory")?;
        Ok(Store {
            dir,
            owner,
            limits,
            boot,
            campaign_spend_max: None,
        })
    }

    /// R4: admits its BUYs within the campaign's cumulative budget
    /// `max_wei` (every grant's admissions, their exits kept room for).
    pub fn with_campaign_spend(mut self, max_wei: u128) -> Self {
        self.campaign_spend_max = Some(max_wei);
        self
    }

    pub(super) fn generation_path(&self, n: u64) -> PathBuf {
        self.dir.join("generations").join(format!("{n:020}.dbrec"))
    }

    /// The owner state directory (the LIVE ledger lives under it).
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn boot(&self) -> Result<&str, String> {
        self.boot.as_deref().map_err(|e| e.clone())
    }

    /// The highest generation on file, or generation 0 when there is none.
    pub fn current(&self) -> Result<Generation, String> {
        current_generation(&self.dir)
    }

    /// R9: the presence signature generation `n` holds, while it is kept.
    fn presence_at(&self, n: u64) -> Option<Option<[u8; 65]>> {
        let bytes = read_private(&self.generation_path(n), self.owner).ok()?;
        Generation::decode(&bytes).ok().map(|g| g.live_presence)
    }

    /// Writes generation 0 (everything OFF and DISABLED) unless a generation
    /// exists: from then on the owner's state is on file for the status
    /// projection.
    pub fn init(&self, now_ms: u64) -> Result<Generation, String> {
        let current = self.current()?;
        if self.highest()?.is_none() {
            match write_new(&self.generation_path(0), &current.encode(now_ms)?) {
                Ok(()) => {}
                Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
                Err(e) => return Err(format!("generation 0 not written: {e}")),
            }
        }
        self.current()
    }

    /// The owner's start (darkbotctl start/restart, the runtime's start or
    /// its crash restart): [`Store::init`], then every ENABLED switch
    /// DISABLED in a new generation (`RESTART_RESET`). Sources, components
    /// and venues keep their modes.
    pub fn restart(&self, now_ms: u64) -> Result<Generation, String> {
        let boot = self.boot()?.to_string();
        let mut current = self.init(now_ms)?;
        // Each lost race means another writer advanced; then again.
        while current.any_enabled() {
            let mut next = current.clone();
            next.generation = current
                .generation
                .checked_add(1)
                .ok_or("generation exhausted")?;
            next.request_id = Some(RESTART_RESET.into());
            next.reset_switches(next.generation);
            next.boot = Some(boot.clone());
            match write_new(
                &self.generation_path(next.generation),
                &next.encode(now_ms)?,
            ) {
                Ok(()) => {
                    self.prune(next.generation);
                    return Ok(next);
                }
                Err(e) if e.kind() == ErrorKind::AlreadyExists => current = self.current()?,
                Err(e) => return Err(format!("restart reset not written: {e}")),
            }
        }
        Ok(current)
    }

    fn highest(&self) -> Result<Option<u64>, String> {
        highest_generation(&self.dir)
    }

    /// Takes one slot of the daily BUY attempt limit for `attempt_id` in
    /// the UTC day of `now_ms` (plan 27.10., `8b4721bb`) and one of the
    /// campaign's total (plan 28.10.): refused when either limit in force
    /// (0 initially) is used up. The same attempt again gets its slots
    /// (retry, crash before the caller went on). Slots are create-new files,
    /// so concurrent attempts never share one; a lost race or a crash can
    /// only over-count, never under-count.
    pub fn consume_buy_attempt(&self, attempt_id: &str, now_ms: u64) -> Result<BuySlot, String> {
        self.consume_attempt_as(attempt_id, 0, now_ms, Durability::Full)
            .map(|(day, _)| day)
    }

    /// [`Store::consume_buy_attempt`] for an attempt under the LIVE grant
    /// written at `grant_generation` (0: none).
    #[cfg(any(test, feature = "test-support"))]
    pub fn consume_attempt(
        &self,
        attempt_id: &str,
        grant_generation: u64,
        now_ms: u64,
    ) -> Result<(BuySlot, TotalSlot), String> {
        self.consume_attempt_as(attempt_id, grant_generation, now_ms, Durability::Full)
    }

    pub(super) fn consume_attempt_as(
        &self,
        attempt_id: &str,
        grant_generation: u64,
        now_ms: u64,
        durability: Durability,
    ) -> Result<(BuySlot, TotalSlot), String> {
        self.consume_attempt_split(attempt_id, attempt_id, grant_generation, now_ms, durability)
    }

    /// [`Store::consume_attempt_as`] with the daily slot named `daily_id`
    /// (BSC3-076: a lease's per UTC day, [`lease_daily_id`]).
    fn consume_attempt_split(
        &self,
        daily_id: &str,
        attempt_id: &str,
        grant_generation: u64,
        now_ms: u64,
        durability: Durability,
    ) -> Result<(BuySlot, TotalSlot), String> {
        // BSC3-004: a clock set back takes no slot (its day would be one
        // already counted, or a day before the campaign's last attempt).
        if let Some(latest) = latest_slot_ms(&self.dir)?
            && now_ms < latest
        {
            return Err(format!(
                "BUY_ATTEMPT_CLOCK_BEHIND: now {now_ms} ms is before the last BUY attempt at {latest} ms"
            ));
        }
        let day = self.consume_daily(daily_id, now_ms, durability)?;
        let total = self.consume_total(attempt_id, grant_generation, now_ms, durability)?;
        Ok((day, total))
    }

    /// The owner's LIVE spend lock, held until dropped: an exclusive lock
    /// of `live-spend.lock` under the owner state (released by the system
    /// when the process ends). Never waited for: held by another admission
    /// (another thread or process), or not to be taken, this one admits
    /// nothing; the clock it was given stays the admission's.
    pub(super) fn spend_lock(&self) -> Result<SpendGuard, String> {
        let path = self.dir.join("live-spend.lock");
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&path)
            .map_err(|e| format!("{LIVE_SPEND_UNREADABLE}: {}: {e}", path.display()))?;
        match file.try_lock() {
            Ok(()) => Ok(SpendGuard { file }),
            Err(fs::TryLockError::WouldBlock) => Err(format!(
                "{LIVE_SPEND_UNREADABLE}: {}: held by another admission",
                path.display()
            )),
            Err(fs::TryLockError::Error(e)) => {
                Err(format!("{LIVE_SPEND_UNREADABLE}: {}: {e}", path.display()))
            }
        }
    }

    /// Admits one LIVE intent as the owner's state holds now, immediately
    /// before its dispatch (plan 28.10.): the grant is still the one the
    /// executor was armed under; the runtime, the intent's component and
    /// the grant's venue are effective LIVE (an exit: without the BUY
    /// attempt reasons); a BUY's input is within the grant; the intent's
    /// most fits the grant's spend. Then a BUY takes its daily and campaign
    /// slot, and every intent writes its spend entry, create-new: both are
    /// durable before any byte can leave. Each refusal names its reason.
    /// The same transaction is never admitted twice (no retry).
    pub fn admit_live(
        &self,
        armed: &LiveNow,
        intent: &LiveIntent<'_>,
        qualification: &LiveQualification,
        now_ms: u64,
    ) -> Result<LiveAdmission, String> {
        self.admit(armed, intent, intent.id, false, qualification, now_ms)
    }

    /// BSC3-076: admits the next C3 BUY before its launch is seen, as
    /// [`Store::admit_live`] admits one (its gate, input, spend at its
    /// nonce with its exit reserve, its daily and campaign slot, its
    /// window), its entry `intent.id` a lease of no token: the launch's
    /// entry path then writes nothing before its first byte (master plan:
    /// no file system work between ingress and the send). `attempt` names
    /// its slots: a lease renewed or moved to another nonce under it takes
    /// no second slot. Its BUY's own outcome becomes the ledger's fact
    /// after the dispatch.
    pub fn lease_live(
        &self,
        armed: &LiveNow,
        intent: &LiveIntent<'_>,
        attempt: &str,
        qualification: &LiveQualification,
        now_ms: u64,
    ) -> Result<LiveAdmission, String> {
        if intent.kind != LiveIntentKind::Buy || intent.component != ComponentId::ExecC3 {
            return Err("a lease is of a C3 BUY".into());
        }
        if parse_hex32(attempt).is_none() {
            return Err("a lease's attempt is named by 32 bytes, hex".into());
        }
        self.admit(armed, intent, attempt, true, qualification, now_ms)
    }

    /// BSC3-076: returns the leases `entries` unused (no BUY was handed over
    /// under them: only the process that holds them knows): each entry moved
    /// whole from the spend to the returns (one rename each, the directories
    /// on the medium before this returns). A returned lease spends and
    /// recovers as nothing; its attempt's slots stay taken (a later lease
    /// under that attempt takes no other). An entry no longer in the spend
    /// (returned already) is skipped; one that is no lease is refused.
    pub fn return_leases(&self, entries: &[[u8; 32]]) -> Result<(), String> {
        let admitted = admitted_intents(&self.dir)?;
        let returns = lease_returns_dir(&self.dir);
        create_durable_dir(&returns)?;
        let mut moved = false;
        for id in entries {
            let Some(entry) = admitted.iter().find(|a| a.tx_hash == *id) else {
                continue;
            };
            if !entry.lease {
                return Err(format!(
                    "{LIVE_SPEND_UNREADABLE}: {} is no lease",
                    hex32(id)
                ));
            }
            let name = format!("{}.dbrec", hex32(id));
            fs::rename(spend_dir(&self.dir).join(&name), returns.join(&name))
                .map_err(|e| format!("{LIVE_SPEND_UNREADABLE}: lease not returned: {e}"))?;
            moved = true;
        }
        if moved {
            flush_device(&spend_dir(&self.dir), &[returns])
                .map_err(|e| format!("{LIVE_SPEND_UNREADABLE}: return not on the medium: {e}"))?;
        }
        Ok(())
    }

    /// BSC3-068: gives back the slots of the BUY attempt `attempt` (hex),
    /// every ledger fact of which the caller found WITHDRAWN (no byte of it
    /// left) and which no lease holds: its campaign slot and each daily
    /// slot it took (a lease's of each UTC day it was held on), under the
    /// spend lock (no admission counts them meanwhile), each moved whole to
    /// the returns, every directory on the medium before this returns. Run
    /// again (a crash in between), it moves what is left. A slot that does
    /// not decode is nobody's and stays. How many it moved.
    pub fn return_attempt(&self, attempt: &str) -> Result<u64, String> {
        if parse_hex32(attempt).is_none() {
            return Err("an attempt is named by 32 bytes, hex".into());
        }
        let _admitting = self.spend_lock()?;
        let returns = slot_returns_dir(&self.dir);
        let lease_day = format!("{attempt}-");
        let mut moves: Vec<(PathBuf, String)> = Vec::new();
        let days = match fs::read_dir(self.dir.join("buy-attempts")) {
            Ok(e) => e
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?,
            Err(e) if e.kind() == ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(format!("buy attempts unreadable: {e}")),
        };
        for day in days.iter().map(|d| d.path()).filter(|d| d.is_dir()) {
            let utc = day
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            for (slot, of) in buy_slots(&day)? {
                if of.is_some_and(|a| a == attempt || a.starts_with(&lease_day)) {
                    moves.push((
                        day.join(format!("{slot:010}.dbrec")),
                        format!("{attempt}-daily-{utc}-{slot:010}.dbrec"),
                    ));
                }
            }
        }
        for (slot, of, _) in total_slots(&self.dir)? {
            if of.as_deref() == Some(attempt) {
                moves.push((
                    total_dir(&self.dir).join(format!("{slot:010}.dbrec")),
                    format!("{attempt}-total-{slot:010}.dbrec"),
                ));
            }
        }
        if moves.is_empty() {
            return Ok(0);
        }
        create_durable_dir(&returns)?;
        let mut touched = vec![returns.clone()];
        for (from, name) in &moves {
            let to = returns.join(name);
            if to.exists() {
                return Err(format!("{}: returned already", to.display()));
            }
            fs::rename(from, &to).map_err(|e| format!("slot not returned: {e}"))?;
            if let Some(d) = from.parent().filter(|d| !touched.iter().any(|t| t == d)) {
                touched.push(d.to_path_buf());
            }
        }
        for d in &touched {
            sync_dir_as(d, Durability::Handed).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        flush_device(&returns, &touched).map_err(|e| format!("return not on the medium: {e}"))?;
        Ok(moves.len() as u64)
    }

    /// BSC3-076: the owner's state as a lease sees it ([`OwnerMark`]).
    pub fn owner_mark(&self) -> Result<OwnerMark, String> {
        use std::os::unix::fs::MetadataExt;
        let m = fs::symlink_metadata(self.dir.join("generations")).map_err(|e| e.to_string())?;
        Ok(OwnerMark {
            dev: m.dev(),
            ino: m.ino(),
            mtime: (m.mtime(), m.mtime_nsec()),
            ctime: (m.ctime(), m.ctime_nsec()),
            nlink: m.nlink(),
            size: m.size(),
        })
    }

    fn admit(
        &self,
        armed: &LiveNow,
        intent: &LiveIntent<'_>,
        attempt: &str,
        lease: bool,
        qualification: &LiveQualification,
        now_ms: u64,
    ) -> Result<LiveAdmission, String> {
        use darkbot_bsc_record::schema::bsc_runtime::live_spend as ls;
        // One admission at a time, across threads and processes: what is
        // read and checked below is what the entry is written against.
        let _admitting = self.spend_lock()?;
        let g = self.current()?;
        let admitted = admitted_intents(&self.dir);
        let book = admitted
            .as_deref()
            .map_err(Clone::clone)
            .and_then(|a| spend_of_grant(a, armed.grant_generation));
        let spent = book.as_ref().ok().and_then(|b| b.wei().ok());
        after_spend_read();
        // BSC3-076 review H1: a lease's own attempt (renewed, moved to
        // another nonce) refuses nothing with the slots it already holds.
        let (daily, total) = usage_besides(
            &self.dir,
            now_ms,
            lease.then_some(attempt),
            self.bounds()?.buy_attempts_total_max,
        );
        let ctx = LiveContext {
            boot: self.boot().ok(),
            now_unix_ms: now_ms,
            daily: daily.as_ref(),
            total: total.as_ref(),
            spent_wei: spent,
            qualification,
        };
        let grant = match (&g.live, &g.live_set_by) {
            (Some(grant), Some(by))
                if by.generation == armed.grant_generation && *grant == armed.grant =>
            {
                grant
            }
            _ => return Err(LIVE_GRANT_CHANGED.into()),
        };
        let exit = intent.kind != LiveIntentKind::Buy;
        let entities = [
            Entity::Runtime,
            Entity::Component(intent.component),
            Entity::Venue(grant.venue),
        ];
        // 01.11. (R1): Strategy C decides every lot's exits; its own BUYs
        // only under a grant naming STRATEGY_C.
        if !exit && intent.component == ComponentId::StrategyC && !grant.strategies.strategy_c() {
            return Err(LIVE_NOT_IN_SCOPE.into());
        }
        g.live_clear(&entities, &ctx, exit)?;
        if !exit && intent.input_wei > grant.max_input_wei {
            return Err(LIVE_INPUT_ABOVE_GRANT.into());
        }
        let (Ok(book), Some(spent)) = (&book, spent) else {
            return Err(LIVE_SPEND_UNREADABLE.into());
        };
        // An alternative of an admitted nonce adds only what it may lose
        // above the most of the others: one of them lands.
        let key = NonceKey {
            origin: intent.origin,
            nonce: intent.nonce,
        };
        let more = u128::try_from(book.increase(Some(key), U256::from(intent.max_wei)))
            .map_err(|_| LIVE_SPEND_UNREADABLE)?;
        let after = spent
            .checked_add(more)
            .filter(|after| *after <= grant.max_spend_wei)
            .ok_or(LIVE_GRANT_SPEND_EXHAUSTED)?;
        if !exit
            && after
                .checked_add(intent.exit_reserve_wei)
                .is_none_or(|kept| kept > grant.max_spend_wei)
        {
            return Err(LIVE_EXIT_RESERVE_MISSING.into());
        }
        // R4: the campaign's cumulative budget, every grant's admissions: a
        // BUY keeps room for its exits; exits always go on.
        if !exit && let Some(max) = self.campaign_spend_max {
            let campaign = admitted
                .as_deref()
                .map_err(|_| LIVE_SPEND_UNREADABLE)
                .and_then(|a| spend_of(a, |_| true).map_err(|_| LIVE_SPEND_UNREADABLE))?;
            let total = campaign.wei().map_err(|_| LIVE_SPEND_UNREADABLE)?;
            let more = u128::try_from(campaign.increase(Some(key), U256::from(intent.max_wei)))
                .map_err(|_| LIVE_SPEND_UNREADABLE)?;
            total
                .checked_add(more)
                .and_then(|after| after.checked_add(intent.exit_reserve_wei))
                .filter(|kept| *kept <= max)
                .ok_or(LIVE_CAMPAIGN_SPEND_EXHAUSTED)?;
        }
        let entry = spend_dir(&self.dir).join(format!("{}.dbrec", intent.id));
        if parse_hex32(intent.id).is_none() {
            return Err("a LIVE intent is named by its transaction hash".into());
        }
        if entry.exists() {
            return Err(LIVE_INTENT_SEEN.into());
        }
        let slots = match exit {
            false => Some(self.consume_attempt_split(
                &match lease {
                    true => lease_daily_id(attempt, now_ms),
                    false => attempt.to_string(),
                },
                attempt,
                armed.grant_generation,
                now_ms,
                Durability::Handed,
            )?),
            true => None,
        };
        let mut d = Draft::new(&ls::SCHEMA);
        d.set(ls::INTENT_ID, intent.id)
            .set(
                ls::KIND,
                match lease {
                    true => LEASE_KIND,
                    false => intent.kind.name(),
                },
            )
            .set(ls::COMPONENT, intent.component.name())
            .set(ls::PROVENANCE_TOOL, "bsc_runtime")
            .set(ls::PROVENANCE_OBSERVED_UNIX_MS, now_ms)
            .set(ls::GRANT_GENERATION, armed.grant_generation)
            .set(
                ls::INPUT_WEI,
                darkbot_bsc_record::Value::u128(intent.input_wei),
            )
            .set(ls::MAX_WEI, darkbot_bsc_record::Value::u128(intent.max_wei))
            .set(ls::TOKEN, darkbot_bsc_record::Value::Address(intent.token))
            .set(ls::NONCE, intent.nonce)
            .set(ls::MAX_BLOCK, intent.max_block)
            .set(ls::CHAIN_ID, intent.origin.chain_id)
            .set(
                ls::SENDER,
                darkbot_bsc_record::Value::Address(intent.origin.sender),
            )
            .set(
                ls::EXIT_RESERVE_WEI,
                darkbot_bsc_record::Value::u128(intent.exit_reserve_wei),
            );
        let bytes = d.encode().map_err(|e| e.to_string())?;
        create_durable_dir(&spend_dir(&self.dir))?;
        match write_new_as(&entry, &bytes, Durability::Handed) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                return Err(LIVE_INTENT_SEEN.into());
            }
            Err(e) => return Err(format!("{LIVE_SPEND_UNREADABLE}: entry not written: {e}")),
        }
        // BSC3-023: the slots and the entry were handed to the drive; one
        // flush puts them on the medium before the admission returns, so
        // before any byte of the intent can leave (BSC3-024). Not flushed:
        // no admission (they over-count, nothing went).
        let written: Vec<PathBuf> = slots
            .iter()
            .flat_map(|(day, _)| [buy_day_dir(&self.dir, &day.day), total_dir(&self.dir)])
            .collect();
        flush_device(&spend_dir(&self.dir), &written)
            .map_err(|e| format!("{LIVE_SPEND_UNREADABLE}: not on the medium: {e}"))?;
        Ok(LiveAdmission {
            slots,
            spent_wei: after,
        })
    }

    /// The owner's LIVE gate as it holds now for the runtime, the grant's
    /// venue and `components` (plan 28.10.): the proof a LIVE end is armed
    /// with, or the first reason it is not.
    pub fn live_now(
        &self,
        components: &[ComponentId],
        qualification: &LiveQualification,
        now_ms: u64,
    ) -> Result<LiveNow, String> {
        self.live_now_with(components, qualification, now_ms, false)
    }

    /// [`Store::live_now`] for the exits of positions the grant opened
    /// (BSC3-071): without the BUY attempt and spend reasons an exit does
    /// not need ([`Store::admit_live`] admits an exit the same way). A
    /// revoked or expired grant, a switch off or a restart still refuse.
    pub fn live_now_exits(
        &self,
        components: &[ComponentId],
        qualification: &LiveQualification,
        now_ms: u64,
    ) -> Result<LiveNow, String> {
        self.live_now_with(components, qualification, now_ms, true)
    }

    /// [`Store::live_now`] for an arm whose lease holds the attempt `held`
    /// (BSC3-076 review H1): that attempt's own slots refuse nothing.
    pub fn live_now_holding(
        &self,
        components: &[ComponentId],
        qualification: &LiveQualification,
        now_ms: u64,
        held: Option<&str>,
    ) -> Result<LiveNow, String> {
        self.live_now_held(components, qualification, now_ms, false, held)
    }

    fn live_now_with(
        &self,
        components: &[ComponentId],
        qualification: &LiveQualification,
        now_ms: u64,
        exit: bool,
    ) -> Result<LiveNow, String> {
        self.live_now_held(components, qualification, now_ms, exit, None)
    }

    fn live_now_held(
        &self,
        components: &[ComponentId],
        qualification: &LiveQualification,
        now_ms: u64,
        exit: bool,
        held: Option<&str>,
    ) -> Result<LiveNow, String> {
        let g = self.current()?;
        let (grant, by) = match (&g.live, &g.live_set_by) {
            (Some(grant), Some(by)) => (grant, by),
            _ => return Err(LIVE_GRANT_MISSING.into()),
        };
        let (daily, total) = usage_besides(
            &self.dir,
            now_ms,
            held,
            self.bounds()?.buy_attempts_total_max,
        );
        let ctx = LiveContext {
            boot: self.boot().ok(),
            now_unix_ms: now_ms,
            daily: daily.as_ref(),
            total: total.as_ref(),
            spent_wei: spent_by_grant(&self.dir, by.generation).ok(),
            qualification,
        };
        let entities: Vec<Entity> = [Entity::Runtime, Entity::Venue(grant.venue)]
            .into_iter()
            .chain(components.iter().map(|c| Entity::Component(*c)))
            .collect();
        g.live_clear(&entities, &ctx, exit)?;
        let sources_live = grant
            .sources
            .iter()
            .copied()
            .filter(|s| g.live_effective(Entity::Source(*s), &ctx, true) == Effective::Live)
            .collect();
        Ok(LiveNow {
            grant: grant.clone(),
            grant_generation: by.generation,
            at_ms: now_ms,
            sources_live,
            presence: g.live_presence,
        })
    }

    fn consume_total(
        &self,
        attempt_id: &str,
        grant_generation: u64,
        now_ms: u64,
        durability: Durability,
    ) -> Result<TotalSlot, String> {
        use darkbot_bsc_record::schema::bsc_runtime::buy_attempt_total_slot as ts;
        let g = self.current()?;
        let max = self.bounds()?.buy_attempts_total_max;
        let limit = g.buy_attempts_total(max);
        create_durable_dir(&total_dir(&self.dir))?;
        let taken = total_slots(&self.dir)?;
        if let Some((slot, _, _)) = taken
            .iter()
            .find(|(_, a, _)| a.as_deref() == Some(attempt_id))
        {
            if *slot >= u64::from(limit) {
                return Err(format!(
                    "BUY_ATTEMPT_TOTAL_REACHED: slot {slot} is beyond the limit {limit}"
                ));
            }
            // Review (BSC3-023): its link may have been all of a write that
            // ended before its directory was synced.
            sync_dir_as(&total_dir(&self.dir), durability)
                .map_err(|e| format!("total buy attempt slot not synced: {e}"))?;
            return Ok(TotalSlot { slot: *slot, limit });
        }
        for slot in 0..u64::from(limit) {
            if taken.iter().any(|(t, _, _)| *t == slot) {
                continue;
            }
            let mut d = Draft::new(&ts::SCHEMA);
            d.set(ts::ATTEMPT_ID, attempt_id)
                .set(ts::SLOT, slot)
                .set(ts::LIMIT, u64::from(limit))
                .set(ts::GENERATION, g.generation)
                .set(ts::GRANT_GENERATION, grant_generation)
                .set(ts::PROVENANCE_TOOL, "bsc_runtime")
                .set(ts::PROVENANCE_OBSERVED_UNIX_MS, now_ms);
            let bytes = d.encode().map_err(|e| e.to_string())?;
            match write_new_as(
                &total_dir(&self.dir).join(format!("{slot:010}.dbrec")),
                &bytes,
                durability,
            ) {
                Ok(()) => {
                    let now = self.current()?.buy_attempts_total(max);
                    if slot >= u64::from(now) {
                        return Err(format!(
                            "BUY_ATTEMPT_TOTAL_REACHED: slot {slot} taken while the limit fell to {now}"
                        ));
                    }
                    return Ok(TotalSlot { slot, limit });
                }
                Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("total buy attempt slot not written: {e}")),
            }
        }
        Err(format!(
            "BUY_ATTEMPT_TOTAL_REACHED: {} of {limit} BUY attempts of the campaign taken at generation {}",
            taken.len().max(limit as usize),
            g.generation
        ))
    }

    fn consume_daily(
        &self,
        attempt_id: &str,
        now_ms: u64,
        durability: Durability,
    ) -> Result<BuySlot, String> {
        use darkbot_bsc_record::schema::bsc_runtime::buy_attempt_slot as bs;
        // The G0.7 runner's attempt ID rule, exactly (the slot files are
        // named by their number, the ID is only their content).
        if !(1..=128).contains(&attempt_id.len())
            || attempt_id.starts_with('.')
            || !attempt_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        {
            return Err("the attempt ID is not a valid attempt identifier".into());
        }
        let g = self.current()?;
        let max = self.bounds()?.buy_attempts_total_max;
        let limit = g.buy_attempts_per_day(max);
        let day = BuyDay::at(now_ms);
        let day_dir = buy_day_dir(&self.dir, &day);
        for d in [self.dir.join("buy-attempts"), day_dir.clone()] {
            create_durable_dir(&d)?;
        }
        if let Some((day, slot)) = daily_slot_of(&self.dir, attempt_id)? {
            // Review M2: a retry finds its slot, within the limit in force.
            if slot >= u64::from(limit) {
                return Err(format!(
                    "BUY_ATTEMPT_LIMIT_REACHED: slot {slot} of {} is beyond the limit {limit}",
                    day.utc
                ));
            }
            sync_dir_as(&buy_day_dir(&self.dir, &day), durability)
                .map_err(|e| format!("buy attempt slot not synced: {e}"))?;
            return Ok(BuySlot { day, slot, limit });
        }
        let taken = buy_slots(&day_dir)?;
        for slot in 0..u64::from(limit) {
            // BSC3-023: a slot seen taken costs no write (one taken
            // meanwhile still fails its link below).
            if taken.iter().any(|(t, _)| *t == slot) {
                continue;
            }
            let mut d = Draft::new(&bs::SCHEMA);
            d.set(bs::ATTEMPT_ID, attempt_id)
                .set(bs::DAY_UTC, day.utc.as_str())
                .set(bs::DAY_INDEX, day.index)
                .set(bs::SLOT, slot)
                .set(bs::LIMIT, u64::from(limit))
                .set(bs::GENERATION, g.generation)
                .set(bs::PROVENANCE_TOOL, "bsc_runtime")
                .set(bs::PROVENANCE_OBSERVED_UNIX_MS, now_ms);
            let bytes = d.encode().map_err(|e| e.to_string())?;
            before_slot_write();
            match write_new_as(
                &day_dir.join(format!("{slot:010}.dbrec")),
                &bytes,
                durability,
            ) {
                Ok(()) => {
                    // BSC3-004: a limit lowered meanwhile holds; the slot
                    // stays taken (counted, never given back).
                    let now = self.current()?.buy_attempts_per_day(max);
                    if slot >= u64::from(now) {
                        return Err(format!(
                            "BUY_ATTEMPT_LIMIT_REACHED: slot {slot} taken while the limit fell to {now}"
                        ));
                    }
                    return Ok(BuySlot { day, slot, limit });
                }
                Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("buy attempt slot not written: {e}")),
            }
        }
        Err(format!(
            "BUY_ATTEMPT_LIMIT_REACHED: {} of {limit} BUY attempts on {} (UTC) taken at generation {}",
            taken.len().max(limit as usize),
            day.utc,
            g.generation
        ))
    }

    /// Applies one request (or replays its receipt). `now_ms` is the record
    /// time; a clock fault is the caller's refusal, never time 0. A LIVE
    /// grant's scope comes only through [`Store::grant_live`]; `DISABLED` on
    /// `BSC_LIVE_GRANT` revokes it.
    pub fn request(
        &self,
        request_id: &str,
        entity: impl Into<Entity>,
        requested: Mode,
        expected_generation: u64,
        now_ms: u64,
    ) -> Result<Receipt, String> {
        if let Mode::Granted(_) = requested {
            return Err("a LIVE grant's scope comes through `live grant`".into());
        }
        self.apply(
            request_id,
            entity.into(),
            requested,
            None,
            expected_generation,
            now_ms,
        )
    }

    /// Grants LIVE for `grant`'s scope (plan 28.10.): refused unless the
    /// scope is valid and `confirmed_sha256` is its scope text's SHA-256,
    /// the one the operator confirmed. A later grant replaces it.
    pub fn grant_live(
        &self,
        request_id: &str,
        grant: LiveGrant,
        confirmed_sha256: [u8; 32],
        expected_generation: u64,
        now_ms: u64,
    ) -> Result<Receipt, String> {
        self.grant_live_present(
            request_id,
            grant,
            confirmed_sha256,
            None,
            expected_generation,
            now_ms,
        )
    }

    /// [`Store::grant_live`] with the operator's presence signature of the
    /// grant (R9, BSC3-001: stored with it, checked by the runtime against
    /// its anchor, `BSC_EXEC_LIVE_PRESENCE_ANCHOR`).
    pub fn grant_live_present(
        &self,
        request_id: &str,
        grant: LiveGrant,
        confirmed_sha256: [u8; 32],
        presence: Option<[u8; 65]>,
        expected_generation: u64,
        now_ms: u64,
    ) -> Result<Receipt, String> {
        grant.validate_within(self.bounds()?.buy_attempts_total_max)?;
        // R9 review LOW-3: a presence signature recovers to a key for the
        // generation this grant is written as (whose key: the runtime's
        // anchor decides).
        if let Some(signature) = &presence
            && presence_signer(&grant, expected_generation + 1, signature).is_none()
        {
            return Err("the presence signature recovers to no key".into());
        }
        // BSC3-069: a new grant names the execution risk it admits.
        if grant.risk.is_none() {
            return Err(format!(
                "{LIVE_GRANT_RISK_MISSING}: a LIVE grant names its execution risk"
            ));
        }
        let digest = grant.scope_sha256();
        if digest != confirmed_sha256 {
            return Err(format!(
                "the confirmed scope {} is not this grant's scope {}",
                hex32(&confirmed_sha256),
                hex32(&digest)
            ));
        }
        self.apply(
            request_id,
            Entity::LiveGrant,
            Mode::Granted(digest),
            Some((grant, presence)),
            expected_generation,
            now_ms,
        )
    }

    fn apply(
        &self,
        request_id: &str,
        entity: Entity,
        requested: Mode,
        grant: Option<(LiveGrant, Option<[u8; 65]>)>,
        expected_generation: u64,
        now_ms: u64,
    ) -> Result<Receipt, String> {
        if !safe_id(request_id) {
            return Err("the request ID is not a safe identifier".into());
        }
        if !entity.accepts(requested)
            || (matches!(entity, Entity::BuyLimit | Entity::BuyTotal)
                && matches!(requested, Mode::PerDay(n) if n > self.bounds()?.buy_attempts_total_max))
        {
            return Err(format!("{} takes {}", entity.name(), entity.modes()));
        }
        let receipt_path = self
            .dir
            .join("receipts")
            .join(format!("{request_id}.dbrec"));
        let same = |r: &Receipt| {
            r.entity == entity
                && r.requested == requested
                && r.expected_generation == expected_generation
        };
        match read_private(&receipt_path, self.owner) {
            Ok(bytes) => {
                let r = Receipt::decode(&bytes)?;
                return if !same(&r) {
                    Err("the request ID already belongs to another request".into())
                } else if let Some((_, presence)) = &grant
                    && r.applied()
                    && self
                        .presence_at(r.generation)
                        .is_some_and(|p| p != *presence)
                {
                    // R9 review LOW-2: a retry is the same grant with the
                    // same presence signature.
                    Err("the request ID already granted with another presence signature".into())
                } else {
                    Ok(r)
                };
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(format!("receipt unreadable: {e}")),
        }
        let current = self.current()?;
        let mut receipt = Receipt {
            request_id: request_id.into(),
            entity,
            requested,
            expected_generation,
            outcome: "REJECTED_STALE",
            generation: current.generation,
            effective: current.effective_of(entity),
        };
        if current.request_id.as_deref() == Some(request_id) {
            // Crash recovery: the generation names this request, the receipt
            // was lost; only the same payload is that request.
            if !current.applied(request_id, entity, requested) {
                return Err("the request ID already belongs to another request".into());
            }
            receipt.outcome = "APPLIED";
        } else if expected_generation < current.generation {
            // BSC3-006: later requests moved the generation on since; the
            // generation this request would have written tells whether it
            // did (its receipt lost), else another request took it. One no
            // longer kept tells nothing: UNRESOLVED, never a guess.
            let path = self.generation_path(expected_generation.saturating_add(1));
            match read_private(&path, self.owner) {
                Ok(bytes) => {
                    let then = Generation::decode(&bytes)?;
                    if then.request_id.as_deref() == Some(request_id) {
                        if !then.applied(request_id, entity, requested) {
                            return Err("the request ID already belongs to another request".into());
                        }
                        receipt.outcome = "APPLIED";
                        receipt.generation = then.generation;
                        receipt.effective = then.effective_of(entity);
                    }
                }
                Err(e) if e.kind() == ErrorKind::NotFound => receipt.outcome = "UNRESOLVED",
                Err(e) => return Err(format!("{}: {e}", path.display())),
            }
        } else if current.generation == expected_generation {
            let boot = self.boot()?.to_string();
            let mut next = current.clone();
            next.generation = current
                .generation
                .checked_add(1)
                .ok_or("generation exhausted")?;
            next.request_id = Some(request_id.into());
            // A switch ENABLED in another boot session never carries over.
            if current.boot.as_deref() != Some(boot.as_str()) {
                next.reset_switches(next.generation);
            }
            next.boot = Some(boot);
            let set_by = SetBy {
                request_id: request_id.into(),
                generation: next.generation,
            };
            match grant {
                Some((grant, presence)) => next.set_live(Some(grant), set_by, presence),
                None => next.set(entity, requested, set_by),
            }
            match write_new(
                &self.generation_path(next.generation),
                &next.encode(now_ms)?,
            ) {
                Ok(()) => {
                    receipt.outcome = "APPLIED";
                    receipt.generation = next.generation;
                    receipt.effective = next.effective_of(entity);
                }
                // Another writer took this generation first: stale, unless it
                // was this very request (an identical twin, e.g. CLI and GUI).
                Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                    let now = self.current()?;
                    if now.request_id.as_deref() == Some(request_id)
                        && now.applied(request_id, entity, requested)
                    {
                        receipt.outcome = "APPLIED";
                    }
                    receipt.generation = now.generation;
                    receipt.effective = now.effective_of(entity);
                }
                Err(e) => return Err(format!("generation not written: {e}")),
            }
        }
        match write_new(&receipt_path, &receipt.encode(now_ms)?) {
            Ok(()) => {}
            // A concurrent identical request wrote it first: its receipt counts.
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                let r = Receipt::decode(
                    &read_private(&receipt_path, self.owner).map_err(|e| e.to_string())?,
                )?;
                return if same(&r) {
                    Ok(r)
                } else {
                    Err("the request ID already belongs to another request".into())
                };
            }
            Err(e) => return Err(format!("receipt not written: {e}")),
        }
        if receipt.applied() {
            self.prune(receipt.generation);
        }
        Ok(receipt)
    }

    /// Removes generations older than `keep` before `current`, and the
    /// receipts whose generation and expected generation are both older
    /// (BSC3-008; best effort): a request that expected a later generation
    /// keeps its refusal until that generation is past.
    fn prune(&self, current: u64) {
        // A reducing store keeps every generation (the next bounded write
        // prunes).
        let Some(below) = self
            .limits
            .and_then(|l| current.checked_sub(l.keep_generations))
        else {
            return;
        };
        if let Ok(entries) = fs::read_dir(self.dir.join("receipts")) {
            for entry in entries.flatten() {
                let path = entry.path();
                if read_private(&path, self.owner)
                    .ok()
                    .and_then(|bytes| Receipt::decode(&bytes).ok())
                    .is_some_and(|r| r.generation.max(r.expected_generation) < below)
                {
                    let _ = fs::remove_file(&path);
                }
            }
        }
        if let Ok(entries) = fs::read_dir(self.dir.join("generations")) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if let Some(n) = name
                    .to_string_lossy()
                    .strip_suffix(".dbrec")
                    .and_then(|n| n.parse::<u64>().ok())
                    && n < below
                {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }
}

/// The highest generation number on file in `dir/generations`.
fn highest_generation(dir: &Path) -> Result<Option<u64>, String> {
    let mut highest: Option<u64> = None;
    for entry in fs::read_dir(dir.join("generations")).map_err(|e| e.to_string())? {
        let name = entry.map_err(|e| e.to_string())?.file_name();
        if let Some(n) = name
            .to_string_lossy()
            .strip_suffix(".dbrec")
            .and_then(|n| n.parse::<u64>().ok())
        {
            highest = highest.max(Some(n));
        }
    }
    Ok(highest)
}

/// Tests only: the current generation's file and record, validated against
/// the owner's rule; `None` before `init`.
#[cfg(test)]
pub fn current_record(dir: &Path) -> Result<Option<(PathBuf, Record)>, String> {
    Ok(current_record_and_generation(dir)?.map(|(path, record, _)| (path, record)))
}

/// The current generation's record and the generation it decodes to, from
/// one read of the file.
pub fn current_record_and_generation(
    dir: &Path,
) -> Result<Option<(PathBuf, Record, Generation)>, String> {
    let Some(n) = highest_generation(dir)? else {
        return Ok(None);
    };
    let path = dir.join("generations").join(format!("{n:020}.dbrec"));
    let bytes = read_private(&path, generations_owner(dir)?).map_err(|e| e.to_string())?;
    let g = Generation::decode(&bytes)?;
    if g.generation != n {
        return Err(format!("generation file {n} holds another generation"));
    }
    let record = decode_generation(&bytes)?;
    Ok(Some((path, record, g)))
}
