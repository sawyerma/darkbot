//! `CapacityPreflight` (master plan §11, storage contract): before a scoped
//! continuous capture starts, the measured source rates and writer
//! throughput are checked against the planned duration, the disk and the
//! capture memory. A run that cannot be qualified is not started; the
//! blocking resource and the window that would be qualified are named.
//! Nothing here measures: every input is a measurement of the caller's.

/// Measured workload and the resources of one planned run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapacityInputs {
    /// Planned duration, in seconds.
    pub duration_s: u64,
    /// Measured average and peak capture rates, bytes per second.
    pub average_bytes_per_s: u64,
    pub peak_bytes_per_s: u64,
    /// How long a peak lasts, in seconds (the burst the memory must hold).
    pub burst_s: u64,
    /// Measured sustained writer throughput, bytes per second.
    pub writer_bytes_per_s: u64,
    /// Segment frames, manifests and index overhead, parts per million of
    /// the journal bytes.
    pub overhead_ppm: u64,
    /// Free bytes of the capture volume, and the reserve to keep free.
    pub free_disk_bytes: u64,
    pub disk_reserve_bytes: u64,
    /// The capture's byte budget (`MemoryBudget`).
    pub memory_budget_bytes: u64,
    /// Measured instead of `(peak - writer) * burst`: the largest backlog
    /// of a recorded trace drained at the writer's rate, in bytes and in
    /// records, and the recorder's queue capacity in records.
    pub measured_backlog_bytes: Option<u64>,
    pub measured_backlog_records: Option<u64>,
    pub queue_records: u64,
}

/// The resource that blocks a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    /// The inputs are not measurements (zero rate or throughput).
    Unmeasured,
    /// The writer cannot sustain the average rate.
    WriterThroughput,
    /// A burst above the writer's rate does not fit the memory budget.
    Memory,
    /// A burst holds more records than the recorder's queue.
    Queue,
    /// The journal does not fit the free disk above the reserve.
    Disk,
}

/// The verdict of one preflight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preflight {
    /// The run fits: the expected journal bytes and the memory a burst
    /// needs at most.
    Qualified {
        expected_bytes: u64,
        burst_backlog_bytes: u64,
    },
    /// The run does not fit; `achievable_s` is the longest duration that
    /// would (zero when no duration does).
    Blocked {
        resource: Resource,
        achievable_s: u64,
    },
}

/// The journal bytes `seconds` of capture produce, overhead included.
fn journal_bytes(i: &CapacityInputs, seconds: u64) -> Option<u64> {
    let raw = i.average_bytes_per_s.checked_mul(seconds)?;
    let overhead = u128::from(raw) * u128::from(i.overhead_ppm) / 1_000_000;
    raw.checked_add(u64::try_from(overhead).ok()?)
}

/// Checks one planned run.
pub fn preflight(i: &CapacityInputs) -> Preflight {
    if i.average_bytes_per_s == 0 || i.writer_bytes_per_s == 0 || i.duration_s == 0 {
        return Preflight::Blocked {
            resource: Resource::Unmeasured,
            achievable_s: 0,
        };
    }
    if i.writer_bytes_per_s < i.average_bytes_per_s {
        return Preflight::Blocked {
            resource: Resource::WriterThroughput,
            achievable_s: 0,
        };
    }
    // During a burst the writer drains at its rate; the rest waits in
    // memory (measured on a trace where one exists).
    let backlog = i.measured_backlog_bytes.unwrap_or_else(|| {
        i.peak_bytes_per_s
            .saturating_sub(i.writer_bytes_per_s)
            .saturating_mul(i.burst_s)
    });
    if backlog > i.memory_budget_bytes {
        return Preflight::Blocked {
            resource: Resource::Memory,
            achievable_s: 0,
        };
    }
    if i.measured_backlog_records
        .is_some_and(|r| r > i.queue_records)
    {
        return Preflight::Blocked {
            resource: Resource::Queue,
            achievable_s: 0,
        };
    }
    let usable = i.free_disk_bytes.saturating_sub(i.disk_reserve_bytes);
    match journal_bytes(i, i.duration_s) {
        Some(bytes) if bytes <= usable => Preflight::Qualified {
            expected_bytes: bytes,
            burst_backlog_bytes: backlog,
        },
        _ => {
            // The longest whole-second run that fits.
            let (mut lo, mut hi) = (0u64, i.duration_s);
            while lo < hi {
                let mid = lo + (hi - lo).div_ceil(2);
                if journal_bytes(i, mid).is_some_and(|b| b <= usable) {
                    lo = mid;
                } else {
                    hi = mid - 1;
                }
            }
            Preflight::Blocked {
                resource: Resource::Disk,
                achievable_s: lo,
            }
        }
    }
}

/// The largest backlog of a trace of per-second amounts (bytes or records)
/// drained at `rate` per second: what waits in memory at its worst.
pub fn measured_backlog(per_second: &[u64], rate: u64) -> u64 {
    let (mut backlog, mut worst) = (0u64, 0u64);
    for amount in per_second {
        backlog = backlog.saturating_add(*amount).saturating_sub(rate);
        worst = worst.max(backlog);
    }
    worst
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs() -> CapacityInputs {
        CapacityInputs {
            duration_s: 1_800,
            average_bytes_per_s: 1_000_000,
            peak_bytes_per_s: 5_000_000,
            burst_s: 10,
            writer_bytes_per_s: 3_000_000,
            overhead_ppm: 50_000,
            free_disk_bytes: 10_000_000_000,
            disk_reserve_bytes: 1_000_000_000,
            memory_budget_bytes: 64_000_000,
            measured_backlog_bytes: None,
            measured_backlog_records: None,
            queue_records: 4_096,
        }
    }

    #[test]
    fn a_run_that_fits_is_qualified_with_its_bytes_and_backlog() {
        assert_eq!(
            preflight(&inputs()),
            Preflight::Qualified {
                expected_bytes: 1_890_000_000,
                burst_backlog_bytes: 20_000_000,
            }
        );
    }

    #[test]
    fn each_resource_blocks_and_names_itself() {
        let unmeasured = CapacityInputs {
            writer_bytes_per_s: 0,
            ..inputs()
        };
        assert!(matches!(
            preflight(&unmeasured),
            Preflight::Blocked {
                resource: Resource::Unmeasured,
                ..
            }
        ));
        let slow = CapacityInputs {
            writer_bytes_per_s: 999_999,
            ..inputs()
        };
        assert!(matches!(
            preflight(&slow),
            Preflight::Blocked {
                resource: Resource::WriterThroughput,
                ..
            }
        ));
        let small = CapacityInputs {
            memory_budget_bytes: 19_999_999,
            ..inputs()
        };
        assert!(matches!(
            preflight(&small),
            Preflight::Blocked {
                resource: Resource::Memory,
                ..
            }
        ));
        // A measured backlog replaces peak x burst, and its records must fit
        // the queue.
        let measured = CapacityInputs {
            measured_backlog_bytes: Some(64_000_001),
            ..inputs()
        };
        assert!(matches!(
            preflight(&measured),
            Preflight::Blocked {
                resource: Resource::Memory,
                ..
            }
        ));
        let records = CapacityInputs {
            measured_backlog_bytes: Some(1),
            measured_backlog_records: Some(4_097),
            ..inputs()
        };
        assert!(matches!(
            preflight(&records),
            Preflight::Blocked {
                resource: Resource::Queue,
                ..
            }
        ));
        // The backlog of a trace drained at a rate: it grows through the
        // seconds above the rate and drains in the quiet ones.
        assert_eq!(measured_backlog(&[5, 5, 30, 30, 0, 0, 30], 10), 40);
        assert_eq!(measured_backlog(&[10, 10, 10], 10), 0);
        assert_eq!(measured_backlog(&[u64::MAX, u64::MAX], 0), u64::MAX);
        // 1.05 MB per second fit into 1.05 GB usable: 1000 s, not 1800.
        let full = CapacityInputs {
            free_disk_bytes: 2_050_000_000,
            ..inputs()
        };
        assert_eq!(
            preflight(&full),
            Preflight::Blocked {
                resource: Resource::Disk,
                achievable_s: 1_000,
            }
        );
    }
}
