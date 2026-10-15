//! Binds the build of the qualification harness to the source it was built
//! from. Every value is passed to the compiler as an environment variable
//! and written into the evidence; whatever cannot be determined is
//! "unknown", which the evaluator treats as incomplete evidence.

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

/// The baseline the frozen production code is pinned to.
const BASELINE: &str = "92fa5204777e566acbaccf111721d9295ffb3a90";

/// The frozen production sources, relative to this crate.
const FROZEN: [&str; 3] = ["../core/src", "../wire/src", "../runtime/src"];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../Cargo.lock");
    println!("cargo:rerun-if-env-changed=RUSTC");
    for path in FROZEN {
        println!("cargo:rerun-if-changed={path}");
    }
    for name in ["HEAD", "index"] {
        if let Some(path) = git(&["rev-parse", "--git-path", name]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }

    let head = git(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());
    let dirty = match git(&["status", "--porcelain", "--untracked-files=no"]) {
        Some(status) if status.is_empty() => "false",
        Some(_) => "true",
        None => "unknown",
    };
    let ancestor = match Command::new("git")
        .args(["merge-base", "--is-ancestor", BASELINE, "HEAD"])
        .status()
    {
        Ok(status) if status.success() => "true",
        Ok(status) if status.code() == Some(1) => "false",
        _ => "unknown",
    };
    // Unchanged since the baseline: no committed change and no change in
    // the working tree.
    let mut committed = vec!["diff", "--name-only", BASELINE, "HEAD", "--"];
    committed.extend(FROZEN);
    let mut working = vec!["diff", "--name-only", "HEAD", "--"];
    working.extend(FROZEN);
    let frozen = match (git(&committed), git(&working)) {
        (Some(a), Some(b)) if a.is_empty() && b.is_empty() => "true",
        (Some(_), Some(_)) => "false",
        _ => "unknown",
    };

    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned());
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let lock = fs::read_to_string(Path::new("../../Cargo.lock")).unwrap_or_default();

    set("Q5A_BUILD_BASELINE", BASELINE);
    set("Q5A_BUILD_GIT_HEAD", &head);
    set("Q5A_BUILD_GIT_DIRTY", dirty);
    set("Q5A_BUILD_BASELINE_ANCESTOR", ancestor);
    set("Q5A_BUILD_FROZEN_UNCHANGED", frozen);
    set("Q5A_BUILD_RUSTC", &version_of(&rustc));
    set("Q5A_BUILD_CARGO", &version_of(&cargo));
    set("Q5A_BUILD_RUSTLS", &locked(&lock, "rustls"));
    set("Q5A_BUILD_RING", &locked(&lock, "ring"));
    set(
        "Q5A_BUILD_PROFILE",
        &env::var("PROFILE").unwrap_or_else(|_| "unknown".to_owned()),
    );
}

fn set(name: &str, value: &str) {
    // A newline would end the directive.
    let value = value.replace(['\n', '\r'], " ");
    println!("cargo:rustc-env={name}={value}");
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn version_of(program: &str) -> String {
    Command::new(program)
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".to_owned())
}

/// The version of `name` in the lock file, if it is there exactly once.
fn locked(lock: &str, name: &str) -> String {
    let marker = format!("[[package]]\nname = \"{name}\"\nversion = \"");
    let mut found = lock.match_indices(&marker).map(|(at, _)| {
        let rest = &lock[at + marker.len()..];
        rest.split('"').next().unwrap_or("").to_owned()
    });
    match (found.next(), found.next()) {
        (Some(version), None) if !version.is_empty() => version,
        _ => "unknown".to_owned(),
    }
}
