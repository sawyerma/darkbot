//! Single-sender cold-start fence. Never deletes or automatically reuses a fence.
//! File IO completes before admission; no filesystem work occurs in lane methods.
//! This provides fail-closed restart, NOT canonical reconciliation or automatic resume.
use crate::intent_lane::{Error, IntentLane, Ticket};
use alloy_primitives::{Address, B256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub enum OpenError {
    Quarantined(PathBuf),
    Io(io::Error),
}
impl From<io::Error> for OpenError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

pub struct FencedLane {
    lane: IntentLane,
}
impl FencedLane {
    /// `root` is the stable configured registry across every restart, not a new
    /// per-run folder. Sender nonce/checkpoint must be independently reconciled.
    /// Existing, truncated or obsolete fences all require external review.
    pub fn arm(
        root: &Path,
        chain: u64,
        sender: Address,
        nonce: u64,
        capacity: usize,
        checkpoint: B256,
    ) -> Result<Self, OpenError> {
        fs::create_dir_all(root)?;
        // Persist the registry directory itself before returning an armed lane.
        if let Some(parent) = root.parent() {
            File::open(parent)?.sync_all()?;
        }
        let path = root.join(format!("{chain}-{}.fence", hex::encode(sender.as_slice())));
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                return Err(OpenError::Quarantined(path));
            }
            Err(e) => return Err(e.into()),
        };
        // Never remove the fence on write/sync error: ambiguous arming stays closed.
        let record = serde_json::json!({"version":1,"chain_id":chain,"sender":sender.to_string(),
            "first_nonce":nonce,"capacity":capacity,"checkpoint":checkpoint.to_string(),
            "restart_action":"QUARANTINE","automatic_resume":false});
        file.write_all(record.to_string().as_bytes())?;
        file.sync_all()?;
        File::open(root)?.sync_all()?;
        Ok(Self {
            lane: IntentLane::new(chain, sender, nonce, capacity),
        })
    }
    /// Borrow the existing lane for the existing guarded builder adapter.
    /// The durable fence remains in force after this borrow and after Drop.
    pub fn for_transport(&mut self) -> &mut IntentLane {
        &mut self.lane
    }
    pub fn reserve(&mut self, token: Address) -> Result<Ticket, Error> {
        self.lane.reserve(token)
    }
    pub fn prepare(&mut self, t: Ticket, h: B256) -> Result<(), Error> {
        self.lane.prepare(t, h)
    }
    pub fn begin_send(&mut self) -> Result<(Ticket, B256), Error> {
        self.lane.begin_send()
    }
    /// Caller must supply independently reconciled canonical evidence; never an ACK.
    pub fn confirm(&mut self, t: Ticket, h: B256) -> Result<(), Error> {
        self.lane.confirm(t, h)
    }
    pub fn halt(&mut self) {
        self.lane.halt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static ID: AtomicU64 = AtomicU64::new(0);
    fn root() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "darkbot-fence-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            ID.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir(&p).unwrap();
        p
    }
    fn arm(p: &Path) -> Result<FencedLane, OpenError> {
        FencedLane::arm(p, 56, Address::repeat_byte(7), 814, 2, B256::repeat_byte(1))
    }
    #[test]
    fn every_restart_state_and_second_owner_is_quarantined() {
        for phase in ["Reserved", "Prepared", "Sending", "Confirmed"] {
            let p = root();
            let mut lane = arm(&p).unwrap();
            assert!(
                matches!(arm(&p), Err(OpenError::Quarantined(_))),
                "second owner"
            );
            let t = lane.reserve(Address::repeat_byte(2)).unwrap();
            let hash = B256::repeat_byte(3);
            if phase != "Reserved" {
                lane.prepare(t, hash).unwrap();
            }
            if phase == "Sending" || phase == "Confirmed" {
                lane.begin_send().unwrap();
            }
            if phase == "Confirmed" {
                lane.confirm(t, hash).unwrap();
            }
            drop(lane);
            assert!(matches!(arm(&p), Err(OpenError::Quarantined(_))));
            println!("G06_FENCE {phase}: restart QUARANTINED, no nonce or intent reused");
            fs::remove_dir_all(p).unwrap();
        }
    }
    #[test]
    fn corrupt_or_empty_fence_never_rearms() {
        for contents in ["", "{truncated"] {
            let p = root();
            let lane = arm(&p).unwrap();
            drop(lane);
            let path = fs::read_dir(&p).unwrap().next().unwrap().unwrap().path();
            fs::write(path, contents).unwrap();
            assert!(matches!(arm(&p), Err(OpenError::Quarantined(_))));
            fs::remove_dir_all(p).unwrap();
        }
    }
    #[test]
    fn filesystem_failure_cannot_return_armed_lane() {
        let p = root();
        let file = p.join("not-a-directory");
        fs::write(&file, b"x").unwrap();
        assert!(matches!(arm(&file), Err(OpenError::Io(_))));
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn armed_lane_retains_dedup_and_unknown_send_halt() {
        let p = root();
        let mut l = arm(&p).unwrap();
        let token = Address::repeat_byte(2);
        let t = l.reserve(token).unwrap();
        assert_eq!(l.reserve(token), Err(Error::Duplicate));
        l.prepare(t, B256::repeat_byte(3)).unwrap();
        l.begin_send().unwrap();
        l.halt();
        assert_eq!(l.begin_send(), Err(Error::Halted));
        drop(l);
        assert!(matches!(arm(&p), Err(OpenError::Quarantined(_))));
        fs::remove_dir_all(p).unwrap();
    }
}
