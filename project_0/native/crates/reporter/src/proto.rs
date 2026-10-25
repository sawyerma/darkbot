#![allow(clippy::all, clippy::pedantic)]
//! Generated protobuf modules, nested by package (the generated code refers across packages by
//! that path); `control` and `project0` name the two packages.
pub mod darkbot {
    pub mod control {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/darkbot.control.v1.rs"));
        }
    }
    pub mod project0 {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/darkbot.project0.v1.rs"));
        }
    }
}
pub use darkbot::control::v1 as control;
pub use darkbot::project0::v1 as project0;
