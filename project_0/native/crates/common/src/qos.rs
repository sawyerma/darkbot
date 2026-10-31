//! P6-MT.12: the QoS class of a thread by its role (PHASENPLAN: Owner/Ingress/FAST/Remote
//! USER_INITIATED, Evidence UTILITY; no claimed P-/E-core affinity). Every thread of a role
//! enters it at its start — in every crate, tokio workers through `on_thread_start` — and the
//! platform hook a production binary installs once, first thing in `main` (`install`: the
//! runtime's, over the kernel's QoS call), sets the class; without a hook (a tool, a test
//! process) a thread keeps its default. This crate stays without dependencies: the hook is the
//! platform's. A refusal is counted (`refusals`).
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

/// The role of a thread. No FAST pool exists yet (route, fee and simulation run on the owner).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadRole {
    Owner,
    Ingress,
    Remote,
    Evidence,
}

/// The platform hook: sets the calling thread's class for a role; `false`: refused.
pub type Hook = fn(ThreadRole) -> bool;

static HOOK: OnceLock<Hook> = OnceLock::new();
static REFUSED: AtomicU64 = AtomicU64::new(0);

/// Installs the process's platform hook (the first installation stays).
pub fn install(hook: Hook) {
    let _ = HOOK.set(hook);
}

/// The calling thread enters `role`: its class is set through the installed hook (none: kept).
pub fn enter(role: ThreadRole) {
    if let Some(hook) = HOOK.get() {
        if !hook(role) {
            REFUSED.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// The threads whose class the platform refused so far.
pub fn refusals() -> u64 {
    REFUSED.load(Ordering::Relaxed)
}
