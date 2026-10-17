#![allow(clippy::all, clippy::pedantic)]

pub mod darkbot {
    pub mod control {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/darkbot.control.v1.rs"));
        }
    }
}

pub use darkbot::control::v1::*;

/// `Error` kollidiert mit `std::error::Error`; im Gateway heißt die Wire-Message WireError.
pub use darkbot::control::v1::Error as WireError;
