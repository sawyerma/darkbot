//! Nonblocking reconciliation dispatch.
use super::*;

impl ShadowOwner {
    pub(super) fn dispatch(&mut self, worker_now_s: u64) -> Result<(), Box<dyn std::error::Error>> {
        if !self.pending.is_empty() {
            let mut keep = Vec::with_capacity(self.pending.len());
            for p in self.pending.drain(..) {
                if worker_now_s < p.next_check_unix_s {
                    keep.push(p);
                    continue;
                }
                match self.remote.reconcile_jobs.try_push(p) {
                    Ok(()) => {}
                    Err(ChannelError::Full(p)) => keep.push(p),
                    Err(ChannelError::Disconnected(_)) => {
                        return Err("reconcile RPC worker disconnected".into());
                    }
                }
            }
            self.pending = keep;
        }

        Ok(())
    }
}
