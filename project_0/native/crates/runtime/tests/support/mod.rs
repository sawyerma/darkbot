//! Process support shared by the p0ctl integration tests that run a copied binary.
use std::process::{Command, Output, Stdio};
use std::sync::{Mutex, MutexGuard};

/// Within one test binary, copying an executable and spawning a process never overlap: a fork
/// while another test thread still holds a fresh copy open for writing makes that copy's exec
/// fail with ETXTBSY ("Text file busy", seen on the Linux CI runners).
static EXEC: Mutex<()> = Mutex::new(());

fn locked() -> MutexGuard<'static, ()> {
    EXEC.lock().unwrap_or_else(|e| e.into_inner())
}

/// Copies the executable `from` to `to`, never while a process of this binary is spawned.
// Shared by several test binaries: in some feature sets a binary has no copying test.
#[allow(dead_code)]
pub fn install(from: &str, to: impl AsRef<std::path::Path>) {
    let _exec = locked();
    std::fs::copy(from, to.as_ref()).unwrap();
}

/// `Command::output` with only the spawn (until the child's exec) under the lock.
pub trait Serialized {
    fn serialized_output(&mut self) -> std::io::Result<Output>;
}
impl Serialized for Command {
    fn serialized_output(&mut self) -> std::io::Result<Output> {
        let child = {
            let _exec = locked();
            self.stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?
        };
        child.wait_with_output()
    }
}
