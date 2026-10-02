//! Allokationsfreies Latenz-Histogramm (S0-03).
//!
//! Log-lineare Buckets: pro Zweierpotenz 8 Sub-Buckets (relativer Fehler ≤ 12,5 %).
//! Perzentile werden als **obere** Bucket-Grenze ausgegeben (konservativ, nie zu
//! klein). `max`, `min`, `count` und `sum` sind exakt.
//!
//! Für das Ledger: p50 / p95 / p99 / max / count.

const SUB_BITS: u32 = 3;
const SUB: usize = 1 << SUB_BITS; // 8
const BUCKETS: usize = SUB + (u64::BITS as usize - SUB_BITS as usize) * SUB; // [FIX-56] 496; exakt alle erreichbaren u64-Buckets

#[derive(Clone, Debug)]
pub struct Histogram {
    counts: [u64; BUCKETS],
    count: u64,
    sum: u128,
    min: u64,
    max: u64,
}

impl Default for Histogram {
    fn default() -> Self {
        Self::new()
    }
}

impl Histogram {
    pub const fn new() -> Self {
        Histogram {
            counts: [0; BUCKETS],
            count: 0,
            sum: 0,
            min: u64::MAX,
            max: 0,
        }
    }

    #[inline]
    fn index(v: u64) -> usize {
        if v < SUB as u64 {
            return v as usize;
        }
        let msb = 63 - v.leading_zeros(); // >= SUB_BITS
        let shift = msb - SUB_BITS;
        let sub = ((v >> shift) & ((SUB as u64) - 1)) as usize;
        ((msb - SUB_BITS + 1) as usize) * SUB + sub
    }

    /// Obere Grenze (inklusive) eines Buckets.
    fn upper(index: usize) -> u64 {
        if index < SUB {
            return index as u64;
        }
        let k = (index / SUB) as u32; // >= 1
        let sub = (index % SUB) as u64;
        let msb = k + SUB_BITS - 1;
        let width = 1u64 << (msb - SUB_BITS);
        let lower = (1u64 << msb) + sub * width;
        lower + (width - 1) // [FIX-56] vermeidet Overflow des Zwischenschritts bei u64::MAX
    }

    #[inline]
    pub fn record(&mut self, v: u64) {
        self.counts[Self::index(v)] += 1;
        self.count += 1;
        self.sum += v as u128;
        if v < self.min {
            self.min = v;
        }
        if v > self.max {
            self.max = v;
        }
    }

    pub fn count(&self) -> u64 {
        self.count
    }

    /// Exaktes Maximum. `None` ohne Samples (nie 0 erfinden).
    pub fn max(&self) -> Option<u64> {
        if self.count == 0 {
            None
        } else {
            Some(self.max)
        }
    }

    pub fn min(&self) -> Option<u64> {
        if self.count == 0 {
            None
        } else {
            Some(self.min)
        }
    }

    pub fn mean(&self) -> Option<f64> {
        if self.count == 0 {
            None
        } else {
            Some(self.sum as f64 / self.count as f64)
        }
    }

    /// Perzentil `p` in [0.0, 1.0]; obere Bucket-Grenze, aber nie über `max`.
    pub fn percentile(&self, p: f64) -> Option<u64> {
        if self.count == 0 {
            return None;
        }
        let p = p.clamp(0.0, 1.0);
        let target = ((p * self.count as f64).ceil() as u64).max(1);
        let mut acc = 0u64;
        for (i, c) in self.counts.iter().enumerate() {
            acc += *c;
            if acc >= target {
                return Some(Self::upper(i).min(self.max));
            }
        }
        Some(self.max)
    }

    /// Ledger-Zeile: (count, p50, p95, p99, max)
    pub fn summary(&self) -> Summary {
        Summary {
            count: self.count,
            p50: self.percentile(0.50),
            p95: self.percentile(0.95),
            p99: self.percentile(0.99),
            max: self.max(),
        }
    }

    pub fn merge(&mut self, other: &Histogram) {
        for i in 0..BUCKETS {
            self.counts[i] += other.counts[i];
        }
        self.count += other.count;
        self.sum += other.sum;
        if other.count > 0 {
            self.min = self.min.min(other.min);
            self.max = self.max.max(other.max);
        }
    }

    pub fn reset(&mut self) {
        *self = Histogram::new();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Summary {
    pub count: u64,
    pub p50: Option<u64>,
    pub p95: Option<u64>,
    pub p99: Option<u64>,
    pub max: Option<u64>,
}

impl std::fmt::Display for Summary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn o(v: Option<u64>) -> String {
            v.map(|x| x.to_string()).unwrap_or_else(|| "N/A".to_string())
        }
        write!(
            f,
            "count={} p50={} p95={} p99={} max={}",
            self.count,
            o(self.p50),
            o(self.p95),
            o(self.p99),
            o(self.max)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_and_upper_are_consistent() {
        for v in [0u64, 1, 7, 8, 9, 15, 16, 17, 31, 32, 100, 1_000, 65_535, 1 << 40, u64::MAX] {
            let i = Histogram::index(v);
            assert!(i < BUCKETS, "index {} out of range for {}", i, v);
            assert!(Histogram::upper(i) >= v, "upper({})={} < {}", i, Histogram::upper(i), v);
            if i > 0 {
                assert!(Histogram::upper(i - 1) < v, "prev bucket upper must be < value");
            }
        }
        assert_eq!(Histogram::index(u64::MAX), BUCKETS - 1);
        assert_eq!(Histogram::upper(BUCKETS - 1), u64::MAX);
    }

    #[test]
    fn empty_is_none_not_zero() {
        let h = Histogram::new();
        assert_eq!(h.max(), None);
        assert_eq!(h.percentile(0.5), None);
        assert_eq!(h.summary().p99, None);
        assert_eq!(h.summary().to_string(), "count=0 p50=N/A p95=N/A p99=N/A max=N/A");
    }

    #[test]
    fn percentiles_are_conservative_and_bounded() {
        let mut h = Histogram::new();
        for v in 1..=1000u64 {
            h.record(v);
        }
        assert_eq!(h.count(), 1000);
        assert_eq!(h.max(), Some(1000));
        let p50 = h.percentile(0.5).unwrap();
        assert!(p50 >= 500 && p50 <= 575, "p50={} (Bucket-Fehler ≤ 12,5 %)", p50);
        let p99 = h.percentile(0.99).unwrap();
        assert!(p99 >= 990 && p99 <= 1000, "p99={}", p99);
        assert_eq!(h.percentile(1.0), Some(1000));
    }

    #[test]
    fn merge_keeps_extremes() {
        let mut a = Histogram::new();
        let mut b = Histogram::new();
        a.record(5);
        b.record(500);
        a.merge(&b);
        assert_eq!(a.count(), 2);
        assert_eq!(a.min(), Some(5));
        assert_eq!(a.max(), Some(500));
    }
}
