//! The wire vocabulary shared by every Fast-Lane record: the record kinds
//! and the fixed field lengths. Wire V1 defined them; Wire V2 keeps the same
//! numbers and lengths. The V1 ring itself had no production caller left on
//! either side (the overlay's hooks are V2 only, `c1_apply_geth_overlay.py`
//! refuses any V1 hook) and was removed (BSC-045).

use std::fmt;

pub const SOURCE_LEN: usize = 16;
pub const PEER_LEN: usize = 32;
pub const HASH_LEN: usize = 32;
pub const AUX_LEN: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum RecordKind {
    EarlyTxSeen = 1,
    TxpoolAccepted = 2,
    CanonicalHead = 3,
    Reorg = 4,
    OutboundSignedTx = 5,
    OutboundPeerHandoff = 6,
    Gap = 7,
    Shutdown = 8,
}

/// A record kind number no wire version defines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownRecordKind(pub u16);

impl fmt::Display for UnknownRecordKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "fastlane: unknown record kind {}", self.0)
    }
}

impl std::error::Error for UnknownRecordKind {}

impl TryFrom<u16> for RecordKind {
    type Error = UnknownRecordKind;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::EarlyTxSeen),
            2 => Ok(Self::TxpoolAccepted),
            3 => Ok(Self::CanonicalHead),
            4 => Ok(Self::Reorg),
            5 => Ok(Self::OutboundSignedTx),
            6 => Ok(Self::OutboundPeerHandoff),
            7 => Ok(Self::Gap),
            8 => Ok(Self::Shutdown),
            _ => Err(UnknownRecordKind(value)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The numbers are wire format: every kind round-trips, nothing else decodes.
    #[test]
    fn record_kinds_are_the_wire_numbers() {
        for n in 1..=8u16 {
            assert_eq!(RecordKind::try_from(n).map(|k| k as u16), Ok(n));
        }
        for n in [0u16, 9, u16::MAX] {
            assert_eq!(RecordKind::try_from(n), Err(UnknownRecordKind(n)));
        }
    }
}
