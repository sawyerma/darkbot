//! What the C1 qualification tools share (C1.9 `c19_report`, C1.9b
//! `c19b_report`, C1.10 `c1_freeze`): per-sample statistics, file and tree
//! hashes, the Git identity, file output and a small argument parser.
//!
//! Every statistic is over directly measured per-sample differences of one
//! clock. No percentile is ever added to another, and no percentile stands in
//! for the maximum.

use darkbot_bsc_record::schema::c1_qualification::latency_stats as ls;
use darkbot_bsc_record::{Draft, Value};
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

/// A miss is a value of at least one millisecond.
pub const MISS_NS: i64 = 1_000_000;
/// Band edges: below 200 µs, below 500 µs, below 1 ms, the rest.
pub const BANDS_NS: [i64; 3] = [200_000, 500_000, 1_000_000];
/// p99.9 needs at least this many samples.
pub const P999_MIN_SAMPLES: usize = 1_000;
/// p99.99 needs at least this many samples.
pub const P9999_MIN_SAMPLES: usize = 10_000;

/// Statistics of one directly measured per-sample quantity, in ns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub samples: usize,
    pub min_ns: Option<i64>,
    /// The exact mean, rounded towards negative infinity.
    pub mean_ns_floor: Option<i64>,
    pub p50_ns: Option<i64>,
    pub p95_ns: Option<i64>,
    pub p99_ns: Option<i64>,
    pub p99_9_ns: Option<i64>,
    pub p99_99_ns: Option<i64>,
    pub max_ns: Option<i64>,
    pub misses_ge_1ms: u64,
    /// `< 200 µs`, `200 µs ..< 500 µs`, `500 µs ..< 1 ms`, `>= 1 ms`.
    pub bands: [u64; 4],
}

/// The nearest-rank percentile `numerator / denominator` of an ascending,
/// non-empty list: the value of rank `max(1, ceil(q * n))`, computed exactly.
pub fn percentile(ordered: &[i64], numerator: u64, denominator: u64) -> Option<i64> {
    let n = ordered.len() as u128;
    let rank = (n * u128::from(numerator))
        .div_ceil(u128::from(denominator))
        .max(1);
    ordered.get(usize::try_from(rank - 1).ok()?).copied()
}

/// Statistics of `values`. With no values only the count (zero) is set.
pub fn describe(values: &[i64]) -> Stats {
    if values.is_empty() {
        return Stats::default();
    }
    let mut ordered = values.to_vec();
    ordered.sort_unstable();
    let count = ordered.len();
    let sum: i128 = ordered.iter().map(|v| i128::from(*v)).sum();
    // The mean lies between min and max, so it fits.
    let mean = i64::try_from(sum.div_euclid(count as i128)).ok();
    let mut bands = [0u64; 4];
    for value in &ordered {
        let band = BANDS_NS
            .iter()
            .position(|edge| value < edge)
            .unwrap_or(BANDS_NS.len());
        bands[band] += 1;
    }
    Stats {
        samples: count,
        min_ns: ordered.first().copied(),
        mean_ns_floor: mean,
        p50_ns: percentile(&ordered, 50, 100),
        p95_ns: percentile(&ordered, 95, 100),
        p99_ns: percentile(&ordered, 99, 100),
        p99_9_ns: (count >= P999_MIN_SAMPLES)
            .then(|| percentile(&ordered, 999, 1000))
            .flatten(),
        p99_99_ns: (count >= P9999_MIN_SAMPLES)
            .then(|| percentile(&ordered, 9999, 10_000))
            .flatten(),
        max_ns: ordered.last().copied(),
        misses_ge_1ms: ordered.iter().filter(|v| **v >= MISS_NS).count() as u64,
        bands,
    }
}

impl Stats {
    /// The statistics as a nested `darkbot.c1.latency-stats` value.
    pub fn to_value(&self, segment: &str) -> Value {
        let mut draft = Draft::new(&ls::SCHEMA);
        draft
            .set(ls::SEGMENT, segment)
            .set(ls::SAMPLES, self.samples as u64);
        if self.samples == 0 {
            draft.set(ls::NOTE, "no samples");
            return draft.into_nested();
        }
        draft
            .set_opt(ls::MIN_NS, self.min_ns)
            .set_opt(ls::MEAN_NS_FLOOR, self.mean_ns_floor)
            .set_opt(ls::P50_NS, self.p50_ns)
            .set_opt(ls::P95_NS, self.p95_ns)
            .set_opt(ls::P99_NS, self.p99_ns)
            .set_opt(ls::P99_9_NS, self.p99_9_ns)
            .set_opt(ls::P99_99_NS, self.p99_99_ns)
            .set_opt(ls::MAX_NS, self.max_ns)
            .set(ls::MISSES_GE_1MS, self.misses_ge_1ms)
            .set(ls::BAND_LT_200US, self.bands[0])
            .set(ls::BAND_200US_TO_LT_500US, self.bands[1])
            .set(ls::BAND_500US_TO_LT_1MS, self.bands[2])
            .set(ls::BAND_GE_1MS, self.bands[3]);
        if self.samples < P999_MIN_SAMPLES {
            draft.set(
                ls::P99_9_NOTE,
                format!(
                    "not reported: {} samples < {P999_MIN_SAMPLES}",
                    self.samples
                ),
            );
        }
        if self.samples < P9999_MIN_SAMPLES {
            draft.set(
                ls::P99_99_NOTE,
                format!(
                    "not reported: {} samples < {P9999_MIN_SAMPLES}",
                    self.samples
                ),
            );
        }
        draft.into_nested()
    }

    /// One line of the derived text view.
    pub fn line(&self, segment: &str) -> String {
        let show = |value: Option<i64>| value.map_or_else(|| "-".to_string(), |v| v.to_string());
        format!(
            "{segment}: n={} p50={} p95={} p99={} p99.9={} p99.99={} max={} misses_ge_1ms={}",
            self.samples,
            show(self.p50_ns),
            show(self.p95_ns),
            show(self.p99_ns),
            show(self.p99_9_ns),
            show(self.p99_99_ns),
            show(self.max_ns),
            self.misses_ge_1ms
        )
    }
}

/// `a - b` of two clock readings, exact in `i128` and clamped to `i64`
/// (a monotonic clock never comes near the bound).
pub fn diff(a: u64, b: u64) -> i64 {
    let value = i128::from(a) - i128::from(b);
    value.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

// ---------------------------------------------------------------------------
// Hashes
// ---------------------------------------------------------------------------

/// SHA-256 of a file, streamed.
pub fn sha256_file(path: &Path) -> io::Result<[u8; 32]> {
    let mut file = File::open(path)?;
    let mut context = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        context.update(&buffer[..read]);
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(context.finish().as_ref());
    Ok(out)
}

/// SHA-256 of a byte string.
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    darkbot_bsc_record::sha256(bytes)
}

/// Every file below `root` (directory symlinks are not followed) whose name
/// ends with `suffix`, as (path components relative to `root`, path), in
/// component-wise order: the order Python's `sorted(Path.rglob(...))` gave.
pub fn files_below(root: &Path, suffix: &str) -> io::Result<Vec<(Vec<String>, PathBuf)>> {
    fn walk(
        dir: &Path,
        prefix: &[String],
        suffix: &str,
        out: &mut Vec<(Vec<String>, PathBuf)>,
    ) -> io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().into_string().map_err(|name| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("non-UTF-8 file name {name:?}"),
                )
            })?;
            let path = entry.path();
            let mut parts = prefix.to_vec();
            parts.push(name.clone());
            let kind = entry.file_type()?;
            if kind.is_dir() {
                walk(&path, &parts, suffix, out)?;
            } else if name.ends_with(suffix) && fs::metadata(&path)?.is_file() {
                out.push((parts, path));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    if root.is_dir() {
        walk(root, &[], suffix, &mut out)?;
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// SHA-256 over every `suffix` file below `root`: per file its relative
/// path, a NUL and its bytes.
pub fn sha256_tree(root: &Path, suffix: &str) -> io::Result<[u8; 32]> {
    let mut context = ring::digest::Context::new(&ring::digest::SHA256);
    for (parts, path) in files_below(root, suffix)? {
        context.update(parts.join("/").as_bytes());
        context.update(b"\0");
        context.update(&fs::read(path)?);
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(context.finish().as_ref());
    Ok(out)
}

/// SHA-256 of a file, or of every file below a directory: per file its
/// relative path, a NUL and the lowercase hex SHA-256 of its bytes.
pub fn sha256_path(path: &Path) -> io::Result<[u8; 32]> {
    if fs::metadata(path)?.is_file() {
        return sha256_file(path);
    }
    let mut context = ring::digest::Context::new(&ring::digest::SHA256);
    for (parts, file) in files_below(path, "")? {
        context.update(parts.join("/").as_bytes());
        context.update(b"\0");
        context.update(hex(&sha256_file(&file)?).as_bytes());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(context.finish().as_ref());
    Ok(out)
}

/// SHA-256 of the running executable (the tool's own identity).
pub fn own_sha256() -> Result<[u8; 32], String> {
    let exe = std::env::current_exe().map_err(|e| format!("own executable: {e}"))?;
    sha256_file(&exe).map_err(|e| format!("{}: {e}", exe.display()))
}

pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// 32 bytes from 64 hex digits, with or without `0x`, either case.
pub fn parse_b256(text: &str) -> Option<[u8; 32]> {
    let digits = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .unwrap_or(text);
    if digits.len() != 64 || !digits.is_ascii() {
        return None;
    }
    let mut out = [0u8; 32];
    for (index, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&digits[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Git
// ---------------------------------------------------------------------------

/// The output of `git -C repo args`; an error when Git fails.
pub fn git(repo: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

/// HEAD of the repository at `path`, if Git can tell.
pub fn git_head(path: &Path) -> Option<String> {
    let out = git(path, &["rev-parse", "HEAD"]).ok()?;
    let head = String::from_utf8(out).ok()?.trim().to_string();
    (!head.is_empty()).then_some(head)
}

/// HEAD of the repository whose root is `path`; `None` when `path` is not
/// a repository root (e.g. a plain directory inside another repository,
/// where `git -C` would answer for the enclosing one).
pub fn repo_root_head(path: &Path) -> Option<String> {
    let top = git(path, &["rev-parse", "--show-toplevel"]).ok()?;
    let top = String::from_utf8(top).ok()?;
    let same = fs::canonicalize(top.trim()).ok()? == fs::canonicalize(path).ok()?;
    same.then(|| git_head(path)).flatten()
}

// ---------------------------------------------------------------------------
// Files and arguments
// ---------------------------------------------------------------------------

/// Writes a new file; an existing one is never replaced.
pub fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|mut file| file.write_all(bytes).and_then(|()| file.sync_all()))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// Replaces `path` atomically: a staging file beside it, synced, renamed.
pub fn write_replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let name = path
        .file_name()
        .ok_or_else(|| format!("{} names no file", path.display()))?;
    let staging = path.with_file_name(format!(
        ".{}.tmp-{}",
        name.to_string_lossy(),
        std::process::id()
    ));
    let staged = write_new(&staging, bytes)
        .and_then(|()| fs::rename(&staging, path).map_err(|e| e.to_string()));
    if let Err(e) = staged {
        let _ = fs::remove_file(&staging);
        return Err(format!("cannot replace {}: {e}", path.display()));
    }
    Ok(())
}

pub fn read(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Command-line arguments: leading positionals, then `--flag value...`
/// groups (a flag takes every following token up to the next `--flag`).
#[derive(Debug, Default)]
pub struct Args {
    pub positional: Vec<String>,
    flags: Vec<(String, Vec<String>)>,
}

impl Args {
    pub fn parse(tokens: impl IntoIterator<Item = String>) -> Self {
        let mut args = Self::default();
        for token in tokens {
            if let Some(name) = token.strip_prefix("--") {
                args.flags.push((name.to_string(), Vec::new()));
            } else if let Some((_, values)) = args.flags.last_mut() {
                values.push(token);
            } else {
                args.positional.push(token);
            }
        }
        args
    }

    /// Refuses a flag outside `known`.
    pub fn only(&self, known: &[&str]) -> Result<(), String> {
        match self
            .flags
            .iter()
            .find(|(name, _)| !known.contains(&name.as_str()))
        {
            Some((name, _)) => Err(format!("unknown flag --{name}")),
            None => Ok(()),
        }
    }

    fn occurrences(&self, name: &str) -> Vec<&[String]> {
        self.flags
            .iter()
            .filter(|(flag, _)| flag == name)
            .map(|(_, values)| values.as_slice())
            .collect()
    }

    /// The value of a flag given at most once with exactly one value.
    pub fn opt(&self, name: &str) -> Result<Option<&str>, String> {
        match self.occurrences(name).as_slice() {
            [] => Ok(None),
            [[value]] => Ok(Some(value.as_str())),
            _ => Err(format!("--{name} takes exactly one value, once")),
        }
    }

    pub fn one(&self, name: &str) -> Result<&str, String> {
        self.opt(name)?
            .ok_or_else(|| format!("--{name} is required"))
    }

    /// Every value of a flag that may be given several times.
    pub fn many(&self, name: &str) -> Vec<&str> {
        self.occurrences(name)
            .into_iter()
            .flatten()
            .map(String::as_str)
            .collect()
    }

    /// A flag without values.
    pub fn switch(&self, name: &str) -> Result<bool, String> {
        match self.occurrences(name).as_slice() {
            [] => Ok(false),
            [[]] => Ok(true),
            _ => Err(format!("--{name} takes no value, once")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ported from bsc/tools/test_c1_qualification_tools.py (DescribeTest).

    #[test]
    fn max_is_reported_and_a_miss_starts_at_exactly_one_millisecond() {
        let stats = describe(&[10, 999_999, 1_000_000, 5]);
        assert_eq!(
            (stats.min_ns, stats.max_ns, stats.samples),
            (Some(5), Some(1_000_000), 4)
        );
        assert_eq!(stats.misses_ge_1ms, 1);
        assert_eq!(stats.bands, [2, 0, 1, 1]);
    }

    #[test]
    fn extreme_percentiles_need_enough_samples() {
        let small: Vec<i64> = (1..1000).collect();
        let small = describe(&small);
        assert_eq!((small.p99_9_ns, small.p99_99_ns), (None, None));
        let value = small.to_value("x");
        let Value::Nested(fields) = value else {
            panic!("nested");
        };
        assert!(fields.iter().any(|(tag, _)| *tag == ls::P99_9_NOTE));
        let medium: Vec<i64> = (1..=1000).collect();
        let medium = describe(&medium);
        assert_eq!((medium.p99_9_ns, medium.p99_99_ns), (Some(999), None));
        let large: Vec<i64> = (1..=10_000).collect();
        let large = describe(&large);
        assert_eq!((large.p99_99_ns, large.max_ns), (Some(9_999), Some(10_000)));
        assert_eq!((large.p50_ns, large.p99_ns), (Some(5_000), Some(9_900)));
        assert_eq!(large.mean_ns_floor, Some(5_000));
    }

    #[test]
    fn no_samples_is_stated() {
        let stats = describe(&[]);
        assert_eq!(stats.samples, 0);
        assert_eq!(stats.max_ns, None);
        let Value::Nested(fields) = stats.to_value("empty") else {
            panic!("nested");
        };
        assert!(fields.contains(&(ls::NOTE, Value::Text("no samples".into()))));
        // A negative difference stays negative; the mean is floored.
        assert_eq!(describe(&[-3, 0]).mean_ns_floor, Some(-2));
    }

    #[test]
    fn arguments_and_hex_parse_strictly() {
        let args = Args::parse(
            ["accept-gate", "C1.5", "--evidence", "a", "b", "--freeze"].map(String::from),
        );
        assert_eq!(args.positional, ["accept-gate", "C1.5"]);
        assert_eq!(args.many("evidence"), ["a", "b"]);
        assert!(args.switch("freeze").unwrap());
        assert!(args.opt("evidence").is_err());
        assert!(args.only(&["evidence"]).is_err());
        assert_eq!(
            parse_b256(&format!("0x{}", "Ab".repeat(32))),
            Some([0xab; 32])
        );
        assert_eq!(parse_b256("ab"), None);
    }
}
