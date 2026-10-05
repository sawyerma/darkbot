//! Aggregation of measured durations.
//!
//! Percentiles are nearest-rank: the p-th percentile of n sorted values is
//! the value at rank ceil(p * n / 100), counting from one. The maximum and
//! the number of values at or above the limit are reported beside them, so
//! that no percentile can hide a miss.

/// Summary of a set of durations, in nanoseconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Summary {
    /// Number of values.
    pub count: usize,
    /// Median, nearest-rank.
    pub p50: Option<u64>,
    /// 95th percentile, nearest-rank.
    pub p95: Option<u64>,
    /// 99th percentile, nearest-rank.
    pub p99: Option<u64>,
    /// Largest value.
    pub max: Option<u64>,
    /// The limit the next two fields count against.
    pub limit: u64,
    /// Values at or above the limit: misses.
    pub at_or_above_limit: usize,
    /// Fraction of the values at or above the limit; zero for no values.
    pub fraction_at_or_above_limit: f64,
}

/// The value at the nearest rank for `percent` of `sorted`, which must be
/// sorted ascending.
pub fn nearest_rank(sorted: &[u64], percent: usize) -> Option<u64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = (percent * sorted.len())
        .div_ceil(100)
        .clamp(1, sorted.len());
    Some(sorted[rank - 1])
}

/// Summarizes `values` against `limit`: a value equal to the limit is a
/// miss.
pub fn summarize(values: &[u64], limit: u64) -> Summary {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let at_or_above_limit = sorted.iter().filter(|value| **value >= limit).count();
    let fraction_at_or_above_limit = if sorted.is_empty() {
        0.0
    } else {
        at_or_above_limit as f64 / sorted.len() as f64
    };
    Summary {
        count: sorted.len(),
        p50: nearest_rank(&sorted, 50),
        p95: nearest_rank(&sorted, 95),
        p99: nearest_rank(&sorted, 99),
        max: sorted.last().copied(),
        limit,
        at_or_above_limit,
        fraction_at_or_above_limit,
    }
}
