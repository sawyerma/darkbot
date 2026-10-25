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
//!   bundle byte; nothing after the release returns it.
//!
//! Git runs as a child with a cleared environment (PATH, HOME, an SSH agent
//! socket), no hooks, no prompt and a hard deadline. Commits are empty-tree
//! Git metadata, not evidence; a new grant after a loss is a new user
//! authorization. `valid_until`/disarm stay additional barriers only.

use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::PathBuf;
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
    pub repo: PathBuf,
    pub remote: String,
    pub timeout: Duration,
}

impl Git {
    /// The main checkout at `.` and its `origin`, 30 s per Git call.
    pub fn origin() -> Self {
        Self {
            repo: PathBuf::from("."),
            remote: "origin".into(),
            timeout: Duration::from_secs(30),
        }
    }

    fn run(&self, args: &[&str], stdin: &[u8]) -> Result<String, String> {
        let mut cmd = Command::new("git");
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
        let mut input = child.stdin.take().ok_or("git stdin")?;
        input
            .write_all(stdin)
            .map_err(|e| format!("git stdin: {e}"))?;
        drop(input);
        let deadline = Instant::now() + self.timeout;
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|e| format!("git: {e}"))? {
                break status;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("git {} timed out", args[0]));
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let mut out = String::new();
        let mut err = String::new();
        let _ = child.stdout.take().map(|mut o| o.read_to_string(&mut out));
        let _ = child.stderr.take().map(|mut e| e.read_to_string(&mut err));
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
        let lease = format!("--force-with-lease={reference}:{expected}");
        let refspec = format!(":{reference}");
        self.run(
            &["push", "--porcelain", &lease, &self.remote, &refspec],
            b"",
        )
        .map(|_| ())
    }

    /// One compare-and-swap of `reference` from `expected` (`""`: absent).
    fn swap(&self, reference: &str, expected: &str, new: &str) -> Result<(), String> {
        let lease = format!("--force-with-lease={reference}:{expected}");
        let refspec = format!("{new}:{reference}");
        self.run(
            &["push", "--porcelain", &lease, &self.remote, &refspec],
            b"",
        )
        .map(|_| ())
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
/// claim that cannot return it, return and re-consume, and a deletion under
/// lease that leaves nothing behind. Every step is reported.
pub fn qualify_remote(git: &Git, nonce: &str) -> Result<Vec<String>, String> {
    let binding = Binding {
        attempt_id: format!("selftest-{nonce}"),
        v5_sha256: [0x5e; 32],
        launch_tx_hash: [0x1a; 32],
        buy_tx_hash: [0x2b; 32],
    };
    let reference = binding.reference()?;
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
        let first = &winners[0];
        step(
            "return_unsent",
            first.return_unsent(git).contains("returned"),
        )?;
        let second = consume(git, &binding, "again").map_err(|e| format!("re-consume: {e}"))?;
        step(
            "stale_claim_cannot_return",
            first.return_unsent(git).contains("stays consumed"),
        )?;
        step(
            "delete_under_lease",
            git.delete(&reference, &second.consumed).is_ok(),
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

/// A consumed authority, held by the run that consumed it.
#[derive(Debug)]
pub struct Claim {
    reference: String,
    authorized: String,
    consumed: String,
    proof: crate::acceptance_guard::AttemptAuthorityProof,
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
    let consumed = git.commit(&format!(
        "{}authorized={authorized}\nclaim={note}\n",
        binding.message("CONSUMED")
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
    /// The guard's proof that this run holds exactly this authority.
    pub fn proof(&self) -> crate::acceptance_guard::AttemptAuthorityProof {
        self.proof.clone()
    }

    /// Gives the authority back after a provably unsent run: CONSUMED (this
    /// claim) -> AUTHORIZED. Fails closed when anything else moved it.
    pub fn return_unsent(&self, git: &Git) -> String {
        match git.remote_head(&self.reference) {
            Ok(Some(head)) if head == self.consumed => {}
            Ok(_) => return "attempt authority stays consumed (no longer this claim's)".into(),
            Err(e) => return format!("attempt authority stays consumed ({e})"),
        }
        match git.swap(&self.reference, &self.consumed, &self.authorized) {
            Ok(()) => "attempt authority returned (AUTHORIZED again)".into(),
            Err(e) => format!("attempt authority stays consumed ({e})"),
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
                Git {
                    repo: dir,
                    remote: "origin".into(),
                    timeout: Duration::from_secs(20),
                }
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

    #[test]
    fn only_one_checkout_or_host_ever_consumes_a_granted_authority() {
        let (_, git) = remote("once", 3);
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
        // Provably unsent: this claim gives it back exactly once.
        assert!(claim.return_unsent(&git[1]).contains("returned"));
        assert!(claim.return_unsent(&git[1]).contains("stays consumed"));
        let second = consume(&git[2], &b, "host2").unwrap();
        assert!(
            claim.return_unsent(&git[0]).contains("stays consumed"),
            "a stale claim"
        );
        assert!(second.return_unsent(&git[2]).contains("returned"));
    }

    #[test]
    fn the_remote_qualification_runs_the_production_operations_and_leaves_nothing() {
        let (_, git) = remote("qualify", 1);
        let steps = qualify_remote(&git[0], "unit").unwrap();
        assert_eq!(steps.len(), 10, "{steps:?}");
        assert!(steps.iter().all(|s| s.ends_with("=OK")), "{steps:?}");
        assert_eq!(existing(&git[0], "selftest-unit"), Ok(None));
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
        let gone = Git {
            repo: git[0].repo.clone(),
            remote: base.join("no-such-remote.git").display().to_string(),
            timeout: Duration::from_secs(20),
        };
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
