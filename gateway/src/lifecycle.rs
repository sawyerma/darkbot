//! Process truth from launchd; independent of gateway/feed connectivity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureProcess {
    Running,
    Stopped,
    Stopping,
    Failed,
    Unknown,
}

impl CaptureProcess {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "RUNNING",
            Self::Stopped => "STOPPED",
            Self::Stopping => "STOPPING",
            Self::Failed => "FAILED",
            Self::Unknown => "UNKNOWN",
        }
    }
}

/// Classify only a successful launchd response. Query failures are handled separately.
pub fn classify(
    pid: Option<u32>,
    last_exit: Option<i32>,
    stop_matches_launch: bool,
    completed_result: bool,
) -> CaptureProcess {
    match pid {
        Some(_) if stop_matches_launch => CaptureProcess::Stopping,
        Some(_) => CaptureProcess::Running,
        None if stop_matches_launch && completed_result => CaptureProcess::Stopped,
        None if last_exit.is_some_and(|code| code != 0) => CaptureProcess::Failed,
        None if last_exit == Some(0) => CaptureProcess::Stopped,
        None => CaptureProcess::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn process_exit_and_operator_stop_are_distinct() {
        assert_eq!(
            classify(Some(12), Some(1), false, false),
            CaptureProcess::Running
        );
        assert_eq!(
            classify(Some(12), None, true, false),
            CaptureProcess::Stopping
        );
        assert_eq!(
            classify(None, Some(256), true, true),
            CaptureProcess::Stopped
        );
        assert_eq!(
            classify(None, Some(256), true, false),
            CaptureProcess::Failed
        );
        assert_eq!(
            classify(None, Some(256), false, false),
            CaptureProcess::Failed
        );
        assert_eq!(
            classify(None, Some(0), false, false),
            CaptureProcess::Stopped
        );
        assert_eq!(classify(None, None, false, false), CaptureProcess::Unknown);
    }
}
