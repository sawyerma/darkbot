#![allow(clippy::all, clippy::pedantic)]
pub mod control {
    include!(concat!(env!("OUT_DIR"), "/darkbot.control.v1.rs"));
}
pub mod project0 {
    include!(concat!(env!("OUT_DIR"), "/darkbot.project0.v1.rs"));
}
