//! The one shared authority of a G0.7 attempt (BSC-005, BSC_Bug_V2 P3-F4):
//! a ref on the main checkout's Git remote, `refs/darkbot/g07-attempt-
//! authority/<attempt_id>`, the only state every checkout and host shares.
//!
//! - `grant` (the user's explicit act) creates the ref, create-only, at the
//!   deterministic AUTHORIZED commit of one binding: attempt ID, approved v5
//!   SHA-256 and both signed transaction hashes, carried only as a digest.
//! - `consume` (the runner, before its local latch) moves it from exactly
//!   that AUTHORIZED commit to a new CONSUMED commit in one compare-and-swap
//!   of the push protocol (`--force-with-lease`). A missing, foreign or
//!   already consumed ref, a lost or broken authority and an unreachable
//!   remote all stop the run: a fresh checkout, a second host, an old armed
//!   binary, a crash, UNKNOWN or a restore of local state never authorize
//!   again.
//! - `Claim::return_unsent` gives it back (CONSUMED -> AUTHORIZED, again a
//!   compare-and-swap from this claim) only where the run provably wrote no
//!   bundle byte (T5): it consumes the claim, and `Claim::release` consumes
//!   it at the release barrier, so no return compiles after the release, and
//!   a returned claim has no guard proof left. A crash, kill, panic, UNKNOWN
//!   or any other path keeps it CONSUMED: nothing gives it back on drop.
//!
//! Git runs as a child with a cleared environment (PATH, HOME, an SSH agent
//! socket), no hooks, no prompt and a hard deadline. Commits are empty-tree
//! Git metadata, not evidence; a new grant after a loss is a new user
//! authorization. `valid_until`/disarm stay additional barriers only.

use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const REF_PREFIX: &str = "refs/darkbot/g07-attempt-authority/";
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
const IDENTITY: &str = "darkbot-g07 <g07-authority@darkbot.invalid> 0 +0000";
const DOMAIN: &[u8] = b"darkbot.g07.attempt-authority.v1";

/// What one authority authorizes: exactly one attempt of exactly these bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub attempt_id: String,
    pub v5_sha256: [u8; 32],
    pub launch_tx_hash: [u8; 32],
    pub buy_tx_hash: [u8; 32],
}

impl Binding {
    /// The digest the commits carry (no hash or ID of the bytes in Git).
    pub fn digest(&self) -> String {
        let mut h = Sha256::new();
        h.update(DOMAIN);
        h.update((self.attempt_id.len() as u64).to_be_bytes());
        h.update(self.attempt_id.as_bytes());
        h.update(self.v5_sha256);
        h.update(self.launch_tx_hash);
        h.update(self.buy_tx_hash);
        hex::encode(h.finalize())
    }

    fn reference(&self) -> Result<String, String> {
        let id = &self.attempt_id;
        let safe = !id.is_empty()
            && id.len() <= 128
            && !id.starts_with('.')
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
            && !id.contains("..");
        if !safe {
            return Err("the attempt ID is not a safe ref component".into());
        }
        Ok(format!("{REF_PREFIX}{id}"))
    }

    fn message(&self, state: &str) -> String {
        format!(
            "darkbot g07 attempt authority\n\nstate={state}\nattempt_id={}\nbinding_sha256={}\n",
            self.attempt_id,
            self.digest()
        )
    }
}

/// `git -C <repo>` against one remote, as a bounded child process.
pub struct Git {
    pub program: PathBuf,
    pub repo: PathBuf,
    pub remote: String,
    pub timeout: Duration,
}

impl Git {
    /// `repo` and its `remote`, 30 s per Git call.
    pub fn new(repo: impl Into<PathBuf>, remote: &str) -> Self {
        Self {
            program: PathBuf::from("git"),
            repo: repo.into(),
            remote: remote.into(),
            timeout: Duration::from_secs(30),
        }
    }

    /// The main checkout at `.` and its `origin`.
    pub fn origin() -> Self {
        Self::new(".", "origin")
    }

    fn run(&self, args: &[&str], stdin: &[u8]) -> Result<String, String> {
        use std::os::unix::process::CommandExt;
        let mut cmd = Command::new(&self.program);
        // Its own process group: a timeout kills git and every helper.
        cmd.process_group(0);
        cmd.env_clear();
        for name in ["PATH", "HOME", "SSH_AUTH_SOCK"] {
            if let Some(value) = std::env::var_os(name) {
                cmd.env(name, value);
            }
        }
        cmd.env("GIT_TERMINAL_PROMPT", "0")
            .env("LC_ALL", "C")
            .arg("-C")
            .arg(&self.repo)
            .args(["-c", "core.hooksPath=/dev/null"])
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| format!("git: {e}"))?;
        let deadline = Instant::now() + self.timeout;
        // Output is read on threads, so a helper that keeps a pipe open
        // (an SSH ControlPersist master) can never block past the deadline.
        let reader = |pipe: Option<Box<dyn Read + Send>>| {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let mut text = String::new();
                if let Some(mut p) = pipe {
                    let _ = p.read_to_string(&mut text);
                }
                let _ = tx.send(text);
            });
            rx
        };
        let out_rx = reader(
            child
                .stdout
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>),
        );
        let err_rx = reader(
            child
                .stderr
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>),
        );
        let kill_group = |child: &mut std::process::Child| {
            let _ = Command::new("/bin/kill")
                .args(["-KILL", "--", &format!("-{}", child.id())])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            let _ = child.kill();
            let _ = child.wait();
        };
        let mut input = child.stdin.take().ok_or("git stdin")?;
        let written = input.write_all(stdin);
        drop(input);
        if let Err(e) = written {
            kill_group(&mut child);
            return Err(format!("git stdin: {e}"));
        }
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|e| format!("git: {e}"))? {
                break status;
            }
            if Instant::now() > deadline {
                kill_group(&mut child);
                return Err(format!("git {} timed out", args[0]));
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let remaining = || {
            deadline
                .saturating_duration_since(Instant::now())
                .max(Duration::from_millis(100))
        };
        let (Ok(out), Ok(err)) = (
            out_rx.recv_timeout(remaining()),
            err_rx.recv_timeout(remaining()),
        ) else {
            kill_group(&mut child);
            return Err(format!(
                "git {}: a helper kept its output open past the deadline",
                args[0]
            ));
        };
        if !status.success() {
            let first = err.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
            return Err(format!("git {} failed: {first}", args[0]));
        }
        Ok(out)
    }

    /// Writes an empty-tree commit with `message` and returns its ID.
    fn commit(&self, message: &str) -> Result<String, String> {
        self.run(&["hash-object", "-w", "-t", "tree", "--stdin"], b"")?;
        let body =
            format!("tree {EMPTY_TREE}\nauthor {IDENTITY}\ncommitter {IDENTITY}\n\n{message}");
        let id = self.run(
            &["hash-object", "-w", "-t", "commit", "--stdin"],
            body.as_bytes(),
        )?;
        Ok(id.trim().to_string())
    }

    /// The remote's commit for `reference`, `None` when it does not exist.
    fn remote_head(&self, reference: &str) -> Result<Option<String>, String> {
        let out = self.run(&["ls-remote", "--refs", &self.remote, reference], b"")?;
        let mut heads = out.lines().filter_map(|l| {
            l.split_once('\t')
                .filter(|(_, r)| *r == reference)
                .map(|(id, _)| id.to_string())
        });
        let head = heads.next();
        if heads.next().is_some() {
            return Err("the remote lists the authority twice".into());
        }
        Ok(head)
    }

    /// Deletes `reference` only while it still holds `expected`.
    fn delete(&self, reference: &str, expected: &str) -> Result<(), String> {
        self.push(reference, expected, &format!(":{reference}"), &['-'])
    }

    /// One compare-and-swap of `reference` from `expected` (`""`: absent).
    /// Only a real update counts: a push of the value the remote already
    /// holds is a Git no-op reported as success (`=`), never a won swap.
    fn swap(&self, reference: &str, expected: &str, new: &str) -> Result<(), String> {
        let accepted: &[char] = if expected.is_empty() {
            &['*']
        } else {
            &['+', ' ']
        };
        self.push(reference, expected, &format!("{new}:{reference}"), accepted)
    }

    /// A leased push whose porcelain status for `reference` must be one of
    /// `accepted` (`*` new, `+` forced, ` ` fast-forward, `-` deleted).
    fn push(
        &self,
        reference: &str,
        expected: &str,
        refspec: &str,
        accepted: &[char],
    ) -> Result<(), String> {
        let lease = format!("--force-with-lease={reference}:{expected}");
        let out = self.run(&["push", "--porcelain", &lease, &self.remote, refspec], b"")?;
        let flag = out.lines().find_map(|l| {
            let mut parts = l.split('\t');
            let flag = parts.next()?.chars().next()?;
            let (_, dst) = parts.next()?.split_once(':')?;
            (dst == reference).then_some(flag)
        });
        match flag {
            Some(f) if accepted.contains(&f) => Ok(()),
            Some('=') => Err("the remote already held this value (no-op push, not a swap)".into()),
            Some(f) => Err(format!("push status {f:?} for the authority")),
            None => Err("no push status for the authority".into()),
        }
    }
}

/// The authority's state on the remote for one binding.
#[derive(Debug, PartialEq, Eq)]
pub enum State {
    Missing,
    Authorized,
    /// Consumed, or anything else: never an authority for this binding.
    Other(String),
}

pub fn state(git: &Git, binding: &Binding) -> Result<State, String> {
    let authorized = git.commit(&binding.message("AUTHORIZED"))?;
    Ok(match git.remote_head(&binding.reference()?)? {
        None => State::Missing,
        Some(head) if head == authorized => State::Authorized,
        Some(head) => State::Other(head),
    })
}

/// Whatever the remote holds for this attempt ID (any binding, any state);
/// readiness, before approval, requires that nothing exists yet.
pub fn existing(git: &Git, attempt_id: &str) -> Result<Option<String>, String> {
    let probe = Binding {
        attempt_id: attempt_id.to_string(),
        v5_sha256: [0; 32],
        launch_tx_hash: [0; 32],
        buy_tx_hash: [0; 32],
    };
    git.remote_head(&probe.reference()?)
}

/// Qualifies the remote as the authority medium with exactly the production
/// operations, on a self-test attempt ID of its own (`selftest-<nonce>`):
/// create-only grant, concurrent consumers with exactly one winner, a stale
/// claim that cannot return it, a RELEASED marker that keeps it consumed, a
/// return of an unsent claim, and a deletion under lease that leaves nothing
/// behind. Every step is reported.
pub fn qualify_remote(git: &Git, nonce: &str) -> Result<Vec<String>, String> {
    let binding = Binding {
        attempt_id: format!("selftest-{nonce}"),
        v5_sha256: [0x5e; 32],
        launch_tx_hash: [0x1a; 32],
        buy_tx_hash: [0x2b; 32],
    };
    let reference = binding.reference()?;
    // A RELEASED marker of this self-test, and a path that provably is none.
    // A fresh private directory (never an existing or shared one).
    let scratch = std::env::temp_dir().join(format!("darkbot-authority-selftest-{nonce}"));
    let absent_marker = scratch.join("absent").join("RELEASED");
    let marker = scratch.join("RELEASED");
    std::os::unix::fs::DirBuilderExt::mode(&mut std::fs::DirBuilder::new(), 0o700)
        .create(&scratch)
        .and_then(|()| {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&marker)
                .and_then(|mut f| f.write_all(b"selftest"))
        })
        .map_err(|e| format!("self-test marker: {e}"))?;
    let mut steps = Vec::new();
    let result = (|| -> Result<(), String> {
        let mut step = |name: &str, ok: bool| {
            steps.push(format!("{name}={}", if ok { "OK" } else { "FAILED" }));
            if ok {
                Ok(())
            } else {
                Err(format!("remote qualification failed at {name}"))
            }
        };
        step("absent_before", state(git, &binding)? == State::Missing)?;
        step("grant", grant(git, &binding).is_ok())?;
        step("grant_is_create_only", grant(git, &binding).is_err())?;
        step("granted", state(git, &binding)? == State::Authorized)?;
        // The compare-and-swap of the push itself, without the ls-remote
        // pre-read: four pushes from the same AUTHORIZED value, one wins.
        let authorized = git.commit(&binding.message("AUTHORIZED"))?;
        let racers = (0..4)
            .map(|i| git.commit(&format!("{}race={i}\n", binding.message("RACE"))))
            .collect::<Result<Vec<_>, _>>()?;
        let pushed: Vec<&String> = std::thread::scope(|s| {
            let runs: Vec<_> = racers
                .iter()
                .map(|c| {
                    let (reference, authorized) = (&reference, &authorized);
                    s.spawn(move || git.swap(reference, authorized, c).is_ok().then_some(c))
                })
                .collect();
            runs.into_iter()
                .filter_map(|r| r.join().ok().flatten())
                .collect()
        });
        step("push_cas_exactly_one_winner", pushed.len() == 1)?;
        step(
            "push_cas_restored",
            git.swap(&reference, pushed[0], &authorized).is_ok(),
        )?;
        let winners: Vec<Claim> = std::thread::scope(|s| {
            let runs: Vec<_> = (0..4)
                .map(|i| {
                    let binding = binding.clone();
                    s.spawn(move || consume(git, &binding, &format!("selftest-racer-{i}")).ok())
                })
                .collect();
            runs.into_iter()
                .filter_map(|r| r.join().ok().flatten())
                .collect()
        });
        step(
            "concurrent_consumers_exactly_one_winner",
            winners.len() == 1,
        )?;
        step(
            "second_consume_refused",
            consume(git, &binding, "late").is_err(),
        )?;
        // A claim the remote no longer holds (moved back by another writer,
        // then taken by another run) never returns the newer one.
        let stale = winners.into_iter().next().ok_or("no winner")?;
        git.swap(&reference, &stale.consumed, &authorized)?;
        let current = consume(git, &binding, "current").map_err(|e| format!("re-consume: {e}"))?;
        step(
            "stale_claim_cannot_return",
            stale.return_unsent(git, &absent_marker).is_err()
                && git.remote_head(&reference)?.as_deref() == Some(current.consumed.as_str()),
        )?;
        // The durable RELEASED marker keeps it consumed.
        let held = current.consumed.clone();
        step(
            "released_marker_keeps_consumed",
            current.return_unsent(git, &marker).is_err()
                && git.remote_head(&reference)?.as_deref() == Some(held.as_str()),
        )?;
        git.swap(&reference, &held, &authorized)?;
        let unsent = consume(git, &binding, "unsent").map_err(|e| format!("re-consume: {e}"))?;
        step(
            "return_unsent",
            unsent.return_unsent(git, &absent_marker).is_ok()
                && state(git, &binding)? == State::Authorized,
        )?;
        step(
            "delete_under_lease",
            git.delete(&reference, &authorized).is_ok(),
        )?;
        step("absent_after", state(git, &binding)? == State::Missing)?;
        Ok(())
    })();
    if result.is_err() {
        // Best effort: never leave a self-test authority behind.
        if let Ok(Some(head)) = git.remote_head(&reference) {
            let _ = git.delete(&reference, &head);
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);
    result.map(|()| steps)
}

/// The user's grant: creates the authority, never over an existing one.
/// (A push of the commit a ref already holds is a no-op that Git reports as
/// success, so the existing state is read first; the lease still decides.)
pub fn grant(git: &Git, binding: &Binding) -> Result<String, String> {
    let reference = binding.reference()?;
    let authorized = git.commit(&binding.message("AUTHORIZED"))?;
    if git.remote_head(&reference)?.is_some() {
        return Err(
            "the authority of this attempt exists already; a grant never replaces it".into(),
        );
    }
    git.swap(&reference, "", &authorized).map_err(|e| {
        format!("the authority was not created (it exists, or the remote refused): {e}")
    })?;
    Ok(authorized)
}

/// A consumed authority, held by the run that consumed it until the release
/// barrier. Neither `Clone` nor `Drop`: whatever ends the run without
/// `return_unsent` (crash, kill, panic, UNKNOWN, an unclear latch, journal
/// or remote) leaves it CONSUMED. `release` consumes it at the barrier, so a
/// return after the release does not compile:
///
/// ```compile_fail,E0382
/// # use darkbot_bsc_execution::attempt_authority::{Claim, Git};
/// fn after_the_release(claim: Claim, git: &Git, marker: &std::path::Path) {
///     let _released = claim.release();
///     let _ = claim.return_unsent(git, marker);
/// }
/// ```
///
/// ```compile_fail,E0599
/// # use darkbot_bsc_execution::attempt_authority::{Git, Released};
/// fn released(released: Released, git: &Git, marker: &std::path::Path) {
///     let _ = released.return_unsent(git, marker);
/// }
/// ```
///
/// Before the release both compile; this is the order the runner uses:
///
/// ```
/// # use darkbot_bsc_execution::attempt_authority::{Claim, Git, Released};
/// fn before_the_release(claim: Claim, git: &Git, marker: &std::path::Path) -> Result<(), String> {
///     let _proof = claim.proof();
///     claim.return_unsent(git, marker)
/// }
/// fn at_the_release(claim: Claim) -> Released {
///     claim.release()
/// }
/// ```
///
/// A returned claim has no guard proof left, and no proof outlives it:
///
/// ```compile_fail,E0382
/// # use darkbot_bsc_execution::attempt_authority::{Claim, Git};
/// fn returned(claim: Claim, git: &Git, marker: &std::path::Path) {
///     let _ = claim.return_unsent(git, marker);
///     let _ = claim.proof();
/// }
/// ```
///
/// ```compile_fail,E0505
/// # use darkbot_bsc_execution::attempt_authority::{Claim, Git};
/// fn kept(claim: Claim, git: &Git, marker: &std::path::Path) {
///     let proof = claim.proof();
///     let _ = claim.return_unsent(git, marker);
///     let _ = proof;
/// }
/// ```
#[derive(Debug)]
pub struct Claim {
    reference: String,
    authorized: String,
    consumed: String,
    proof: crate::acceptance_guard::AttemptAuthorityProof,
}

/// The authority of a run past its release barrier: spent for good. It
/// names the consumed commit and has no way back.
#[derive(Debug)]
#[must_use]
pub struct Released {
    consumed: String,
}

impl Released {
    pub fn consumed_commit(&self) -> &str {
        &self.consumed
    }
}

/// Takes the authority: AUTHORIZED -> CONSUMED in one compare-and-swap.
/// `note` (boot ID, pid, time) makes this claim's commit unique.
pub fn consume(git: &Git, binding: &Binding, note: &str) -> Result<Claim, String> {
    let reference = binding.reference()?;
    let authorized = git.commit(&binding.message("AUTHORIZED"))?;
    match git.remote_head(&reference)? {
        Some(head) if head == authorized => {}
        None => {
            return Err(
                "no authority for this attempt on the remote (never granted or lost)".into(),
            );
        }
        Some(_) => {
            return Err(
                "the remote authority of this attempt is consumed or not of these bytes".into(),
            );
        }
    }
    // A random nonce: no two claims ever build the same commit.
    let mut nonce = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut nonce))
        .map_err(|e| format!("claim nonce: {e}"))?;
    let consumed = git.commit(&format!(
        "{}authorized={authorized}\nclaim={note}\nnonce={}\n",
        binding.message("CONSUMED"),
        hex::encode(nonce)
    ))?;
    git.swap(&reference, &authorized, &consumed).map_err(|e| {
        format!("the authority was not consumed (another run took it, or the remote refused): {e}")
    })?;
    Ok(Claim {
        reference,
        authorized,
        consumed,
        proof: crate::acceptance_guard::AttemptAuthorityProof::new(
            &binding.attempt_id,
            binding.v5_sha256,
        ),
    })
}

impl Claim {
    /// The guard's proof that this run holds exactly this authority, borrowed
    /// from the claim: it cannot outlive a return or the release.
    pub fn proof(&self) -> &crate::acceptance_guard::AttemptAuthorityProof {
        &self.proof
    }

    /// The CONSUMED commit of this claim, for the attempt's journal.
    pub fn consumed_commit(&self) -> &str {
        &self.consumed
    }

    /// The release barrier: the claim is spent, nothing returns it any more.
    pub fn release(self) -> Released {
        Released {
            consumed: self.consumed,
        }
    }

    /// Gives the authority back after a provably byte-free stop before the
    /// release barrier (T5): CONSUMED (exactly this claim) -> AUTHORIZED, in
    /// one compare-and-swap. `released_marker` is the attempt's durable
    /// RELEASED marker: only its proven absence (not found) allows it. Any
    /// marker, an unclear marker or remote, or a remote that holds anything
    /// else keeps it consumed (`Err` names why). Takes the claim either way.
    pub fn return_unsent(self, git: &Git, released_marker: &Path) -> Result<(), String> {
        match std::fs::symlink_metadata(released_marker) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) => return Err("the RELEASED marker exists: bytes may have left".into()),
            Err(e) => return Err(format!("the RELEASED marker is unclear ({e})")),
        }
        match git.remote_head(&self.reference)? {
            Some(head) if head == self.consumed => {}
            _ => return Err("the remote authority is no longer this claim's".into()),
        }
        let Err(e) = git.swap(&self.reference, &self.consumed, &self.authorized) else {
            return Ok(());
        };
        // A failed push may still have landed (a deadline after the remote
        // took it): the remote decides, and anything unclear says so.
        match git.remote_head(&self.reference) {
            Ok(Some(head)) if head == self.authorized => Ok(()),
            Ok(Some(head)) if head == self.consumed => Err(format!(
                "the return failed, the authority stays consumed: {e}"
            )),
            Ok(other) => Err(format!(
                "UNCLEAR: after a failed return the remote holds {other:?}: {e}"
            )),
            Err(read) => Err(format!(
                "UNCLEAR: the remote is unreadable after a failed return ({read}): {e}"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// A bare origin and `n` independent clones (checkouts / hosts).
    fn remote(tag: &str, n: usize) -> (PathBuf, Vec<Git>) {
        let base = std::env::temp_dir().join(format!("g07-authority-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let origin = base.join("origin.git");
        let ok = |args: &[&str], at: &Path| {
            let out = Command::new("git")
                .current_dir(at)
                .args(args)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        ok(&["init", "-q", "--bare", origin.to_str().unwrap()], &base);
        let clones = (0..n)
            .map(|i| {
                let dir = base.join(format!("checkout{i}"));
                ok(&["init", "-q", dir.to_str().unwrap()], &base);
                ok(&["remote", "add", "origin", origin.to_str().unwrap()], &dir);
                Git::new(dir, "origin")
            })
            .collect();
        (base, clones)
    }

    fn binding(v5: u8) -> Binding {
        Binding {
            attempt_id: "g07-mainnet-attempt-1-test".into(),
            v5_sha256: [v5; 32],
            launch_tx_hash: [0xaa; 32],
            buy_tx_hash: [0xbb; 32],
        }
    }

    /// A RELEASED marker path that provably does not exist.
    fn no_marker(base: &Path) -> PathBuf {
        base.join("no-attempt-dir").join("RELEASED")
    }

    #[test]
    fn only_one_checkout_or_host_ever_consumes_a_granted_authority() {
        let (base, git) = remote("once", 3);
        let b = binding(1);
        // Never granted (or lost): blocked.
        assert_eq!(state(&git[0], &b), Ok(State::Missing));
        assert!(
            consume(&git[0], &b, "a")
                .unwrap_err()
                .contains("never granted or lost")
        );
        grant(&git[0], &b).unwrap();
        assert!(grant(&git[1], &b).is_err(), "create-only");
        assert_eq!(state(&git[2], &b), Ok(State::Authorized));
        // Another binding (other v5 or bytes) is never authorized by it.
        assert!(consume(&git[1], &binding(2), "x").is_err());
        let claim = consume(&git[1], &b, "host1").unwrap();
        // The same checkout again, a fresh checkout, a second host: blocked.
        for (i, g) in git.iter().enumerate() {
            let e = consume(g, &b, "again").unwrap_err();
            assert!(e.contains("consumed or not of these bytes"), "{i}: {e}");
            assert!(matches!(state(g, &b), Ok(State::Other(_))));
        }
        assert!(
            grant(&git[2], &b).is_err(),
            "never re-granted over a consumed one"
        );
        // Provably unsent: this claim gives it back (and is spent doing so).
        claim.return_unsent(&git[1], &no_marker(&base)).unwrap();
        assert_eq!(state(&git[0], &b), Ok(State::Authorized));
        let second = consume(&git[2], &b, "host2").unwrap();
        second.return_unsent(&git[2], &no_marker(&base)).unwrap();
    }

    /// T5: only this claim, no RELEASED marker and a clear remote return it;
    /// a stale claim, a marker, an unclear marker or an unreachable remote
    /// keep it CONSUMED.
    #[test]
    fn a_return_needs_this_claim_no_marker_and_a_clear_remote() {
        let (base, git) = remote("return", 2);
        let b = binding(6);
        let reference = b.reference().unwrap();
        let authorized = grant(&git[0], &b).unwrap();
        // The deterministic AUTHORIZED commit, built in the second checkout too.
        assert_eq!(git[1].commit(&b.message("AUTHORIZED")).unwrap(), authorized);
        let held = |g: &Git| g.remote_head(&reference).unwrap();

        // A stale claim: another writer moved the ref, another run took it.
        let stale = consume(&git[0], &b, "stale").unwrap();
        git[1]
            .swap(&reference, &stale.consumed, &authorized)
            .unwrap();
        let current = consume(&git[1], &b, "current").unwrap();
        let e = stale.return_unsent(&git[0], &no_marker(&base)).unwrap_err();
        assert!(e.contains("no longer this claim's"), "{e}");
        assert_eq!(held(&git[0]).as_deref(), Some(current.consumed.as_str()));

        // The durable RELEASED marker: bytes may have left.
        let attempt = base.join("attempt");
        std::fs::create_dir_all(&attempt).unwrap();
        std::fs::write(attempt.join("RELEASED"), b"released").unwrap();
        let consumed = current.consumed.clone();
        let e = current
            .return_unsent(&git[1], &attempt.join("RELEASED"))
            .unwrap_err();
        assert!(e.contains("RELEASED marker exists"), "{e}");
        assert_eq!(held(&git[0]), Some(consumed.clone()));

        // A marker that cannot be inspected is not an absent marker.
        git[1].swap(&reference, &consumed, &authorized).unwrap();
        let claim = consume(&git[1], &b, "unclear").unwrap();
        let sealed = base.join("sealed");
        std::fs::create_dir_all(&sealed).unwrap();
        std::fs::set_permissions(&sealed, std::os::unix::fs::PermissionsExt::from_mode(0o000))
            .unwrap();
        let consumed = claim.consumed.clone();
        let e = claim
            .return_unsent(&git[1], &sealed.join("RELEASED"))
            .unwrap_err();
        std::fs::set_permissions(&sealed, std::os::unix::fs::PermissionsExt::from_mode(0o700))
            .unwrap();
        assert!(e.contains("unclear"), "{e}");
        assert_eq!(held(&git[0]), Some(consumed.clone()));

        // An unreachable remote: unclear, so it stays consumed.
        git[1].swap(&reference, &consumed, &authorized).unwrap();
        let claim = consume(&git[1], &b, "offline").unwrap();
        let consumed = claim.consumed.clone();
        let offline = Git::new(
            git[1].repo.clone(),
            &base.join("no-such-remote.git").display().to_string(),
        );
        assert!(claim.return_unsent(&offline, &no_marker(&base)).is_err());
        assert_eq!(held(&git[0]), Some(consumed));
    }

    /// Review LOW-2: a return whose push reports failure is settled by the
    /// remote: landed (the deadline hit after the remote took it) is
    /// returned, not landed stays consumed; it is never guessed.
    #[test]
    fn a_failed_return_push_is_settled_by_what_the_remote_holds() {
        let (base, git) = remote("unclear", 1);
        let b = binding(9);
        grant(&git[0], &b).unwrap();
        let wrapper = |name: &str, push: &str| {
            let path = base.join(name);
            std::fs::write(
                &path,
                format!(
                    "#!/bin/sh\ncase \"$*\" in *push*) {push}; exit 1;; esac\nexec git \"$@\"\n"
                ),
            )
            .unwrap();
            std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
                .unwrap();
            Git {
                program: path,
                ..Git::new(git[0].repo.clone(), "origin")
            }
        };
        // The push lands, then reports failure: the authority is returned.
        let claim = consume(&git[0], &b, "landed").unwrap();
        let landed = wrapper("git-landed", "git \"$@\" >/dev/null 2>&1");
        assert_eq!(claim.return_unsent(&landed, &no_marker(&base)), Ok(()));
        assert_eq!(state(&git[0], &b), Ok(State::Authorized));
        // The push never lands: it stays consumed, and says so.
        let claim = consume(&git[0], &b, "lost").unwrap();
        let consumed = claim.consumed.clone();
        let lost = wrapper("git-lost", ":");
        let e = claim.return_unsent(&lost, &no_marker(&base)).unwrap_err();
        assert!(
            e.contains("stays consumed") && !e.starts_with("UNCLEAR"),
            "{e}"
        );
        assert_eq!(state(&git[0], &b), Ok(State::Other(consumed)));
    }

    /// A run that consumed the authority and ends without returning it (a
    /// panic unwinding through it here; SIGKILL in the next test) leaves it
    /// CONSUMED: a restart neither consumes nor re-grants it.
    #[test]
    fn a_panic_after_the_cas_keeps_it_consumed() {
        let (_, git) = remote("panic", 1);
        let b = binding(7);
        grant(&git[0], &b).unwrap();
        let unwound = std::panic::catch_unwind(|| {
            let _claim = consume(&git[0], &b, "panics").unwrap();
            panic!("stop between the CAS and the barrier");
        });
        assert!(unwound.is_err());
        assert!(matches!(state(&git[0], &b), Ok(State::Other(_))));
        assert!(consume(&git[0], &b, "restart").is_err());
        assert!(grant(&git[0], &b).is_err());
    }

    const CHILD_REPO: &str = "DARKBOT_AUTHORITY_TEST_CHILD_REPO";

    /// The child of the kill test: consumes, reports, then waits to be killed.
    #[test]
    #[ignore = "run only as the child process of the kill test"]
    fn claim_holder_child() {
        let Ok(repo) = std::env::var(CHILD_REPO) else {
            return;
        };
        let git = Git::new(PathBuf::from(&repo), "origin");
        let claim = consume(&git, &binding(8), "child").unwrap();
        std::fs::write(Path::new(&repo).join("CONSUMED"), claim.consumed_commit()).unwrap();
        std::thread::sleep(Duration::from_secs(60));
    }

    /// SIGKILL between the CAS and the release barrier, then a restart.
    #[test]
    fn a_kill_between_the_cas_and_the_barrier_keeps_it_consumed() {
        let (_, git) = remote("kill", 2);
        let b = binding(8);
        grant(&git[0], &b).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "attempt_authority::tests::claim_holder_child",
            ])
            .env(CHILD_REPO, &git[1].repo)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let reported = git[1].repo.join("CONSUMED");
        let started = Instant::now();
        while !reported.exists() {
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "child never consumed"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        child.kill().unwrap();
        child.wait().unwrap();
        let consumed = std::fs::read_to_string(&reported).unwrap();
        assert_eq!(state(&git[0], &b), Ok(State::Other(consumed)));
        // The restart (any checkout) finds it consumed; no grant replaces it.
        for g in &git {
            assert!(consume(g, &b, "restart").is_err());
        }
        assert!(grant(&git[0], &b).is_err());
    }

    #[test]
    fn the_remote_qualification_runs_the_production_operations_and_leaves_nothing() {
        let (_, git) = remote("qualify", 1);
        let steps = qualify_remote(&git[0], "unit").unwrap();
        assert_eq!(steps.len(), 13, "{steps:?}");
        assert!(steps.iter().all(|s| s.ends_with("=OK")), "{steps:?}");
        assert_eq!(existing(&git[0], "selftest-unit"), Ok(None));
    }

    /// Review 25.10. #1: a push of the value the remote already holds is a
    /// Git no-op reported as success; it never counts as a won swap, and
    /// two claims of one binding never build the same commit.
    #[test]
    fn a_no_op_push_never_counts_as_a_won_swap_and_claims_are_unique() {
        let (base, git) = remote("noop", 2);
        let b = binding(5);
        let authorized = grant(&git[0], &b).unwrap();
        let reference = b.reference().unwrap();
        // The remote holds `authorized`; a stale lease with the same value.
        assert!(
            git[0]
                .swap(&reference, &"1".repeat(40), &authorized)
                .is_err()
        );
        let claim = consume(&git[0], &b, "same-note").unwrap();
        let first = claim.consumed.clone();
        claim.return_unsent(&git[0], &no_marker(&base)).unwrap();
        let again = consume(&git[1], &b, "same-note").unwrap();
        assert_ne!(first, again.consumed, "a claim commit must be unique");
        // The proof is exactly this attempt's and v5's (a returned claim has
        // none: `return_unsent` takes it, see the compile_fail examples).
        assert_eq!(
            again.proof(),
            &crate::acceptance_guard::AttemptAuthorityProof::new(&b.attempt_id, b.v5_sha256)
        );
        // The barrier spends it: the remote keeps this claim's CONSUMED commit.
        let released = again.release();
        assert_eq!(
            git[0].remote_head(&reference).unwrap().as_deref(),
            Some(released.consumed_commit())
        );
    }

    /// Review 25.10. #4: a Git child whose helper keeps the output open
    /// ends at the deadline, the whole process group killed.
    #[test]
    fn a_git_child_holding_its_output_open_ends_at_the_deadline() {
        let dir = std::env::temp_dir().join(format!("g07-authority-hang-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("git");
        std::fs::write(&fake, "#!/bin/sh\nsleep 30 &\nexit 0\n").unwrap();
        std::fs::set_permissions(&fake, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        let git = Git {
            program: fake,
            repo: dir.clone(),
            remote: "origin".into(),
            timeout: Duration::from_millis(500),
        };
        let started = Instant::now();
        assert!(git.run(&["status"], b"").is_err());
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn concurrent_consumers_have_exactly_one_winner() {
        let (_, git) = remote("race", 4);
        let b = binding(3);
        grant(&git[0], &b).unwrap();
        let wins = std::thread::scope(|s| {
            let runs: Vec<_> = git
                .iter()
                .enumerate()
                .map(|(i, g)| {
                    let b = b.clone();
                    s.spawn(move || consume(g, &b, &format!("racer{i}")).is_ok())
                })
                .collect();
            runs.into_iter()
                .filter_map(|r| r.join().ok())
                .filter(|won| *won)
                .count()
        });
        assert_eq!(wins, 1);
    }

    #[test]
    fn an_unreachable_or_lost_authority_blocks() {
        let (base, git) = remote("lost", 1);
        let b = binding(4);
        grant(&git[0], &b).unwrap();
        // The remote lost the ref: as never granted.
        let out = Command::new("git")
            .args([
                "-C",
                base.join("origin.git").to_str().unwrap(),
                "update-ref",
                "-d",
            ])
            .arg(format!("{REF_PREFIX}{}", b.attempt_id))
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(consume(&git[0], &b, "a").is_err());
        let gone = Git::new(
            git[0].repo.clone(),
            &base.join("no-such-remote.git").display().to_string(),
        );
        assert!(
            consume(&gone, &b, "a")
                .unwrap_err()
                .contains("git ls-remote failed")
        );
        let unsafe_id = Binding {
            attempt_id: "../heads/main".into(),
            ..b
        };
        assert!(consume(&git[0], &unsafe_id, "a").is_err());
    }
}
