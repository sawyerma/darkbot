//! Public read-only evidence projection. No signer, RPC client or builder submission.
pub mod census;
pub mod projection;
pub mod publisher;
pub mod wire {
    pub mod control {
        include!(concat!(env!("OUT_DIR"), "/darkbot.control.v1.rs"));
    }
    pub mod bsc {
        include!(concat!(env!("OUT_DIR"), "/darkbot.bsc.v1.rs"));
    }
}

#[cfg(test)]
mod tests;
