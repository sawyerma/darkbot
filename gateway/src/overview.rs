//! overview.aggregates: Gateway-eigene Projektion. Nur echte Source-Zustände; Readiness,
//! Zähler und Finanzwerte bleiben UNKNOWN/abwesend, bis eine Engine sie belegt.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use prost::Message;
use tracing::warn;

use crate::config::GatewayConfig;
use crate::engine::{SourceRegistry, SourceState};
use crate::hub::{StateHub, StateUpdate};
use crate::proto::{GlobalFinance, OverviewAggregates, ProjectId, ProjectSummary, Readiness, SourceState as WireSourceState, TopicId};

const PROJECTS: &[(ProjectId, &str, &str)] = &[
    (ProjectId::Project0, "Project 0", "Solana / Marginfi"),
    (ProjectId::Bsc, "BSC", "BSC / Birth Entry"),
    (ProjectId::Solana, "Solana", "Solana / Pump.fun"),
];

/// Crash-safe, strictly monotonic gateway-owned publisher generation.
/// Must be stored on durable storage. A stale .lock file is intentionally fail-closed
/// after a crash and requires an explicit operator check before removal.
/// The path defaults to gateway/.runtime/publisher_generation, override via
/// DARKBOT_GATEWAY_GENERATION_FILE. Never use just SystemTime seconds as generation.
pub fn gateway_generation() -> std::io::Result<u64> {
    use std::path::PathBuf;
    let path = std::env::var_os("DARKBOT_GATEWAY_GENERATION_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".runtime/publisher_generation"));
    allocate_generation(&path)
}

fn allocate_generation(path: &std::path::Path) -> std::io::Result<u64> {
    use std::fs::{self, OpenOptions};
    use std::io::{self, Write};
    let dir = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing parent"))?;
    fs::create_dir_all(dir)?;
    let lock_path = path.with_extension("lock");
    let _lock = OpenOptions::new().write(true).create_new(true).open(&lock_path)?;
    struct LockGuard(std::path::PathBuf);
    impl Drop for LockGuard {
        fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); }
    }
    let _guard = LockGuard(lock_path);
    let previous = match fs::read_to_string(path) {
        Ok(s) => s.trim().parse::<u64>()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid persistent generation"))?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => 0,
        Err(e) => return Err(e),
    };
    let wall = SystemTime::now().duration_since(UNIX_EPOCH)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "clock before epoch"))?
        .as_nanos();
    let wall = u64::try_from(wall)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "epoch nanoseconds exceed u64"))?;
    let next = previous.checked_add(1)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "generation overflow"))?
        .max(wall);
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
    file.write_all(next.to_string().as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(tmp, path)?;
    fs::File::open(dir)?.sync_all()?;
    Ok(next)
}

pub fn unix_ns_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as i64).unwrap_or(0)
}

fn wire_source(s: SourceState) -> WireSourceState {
    match s {
        SourceState::NotConnected => WireSourceState::NotConnected,
        SourceState::Connected => WireSourceState::Connected,
        SourceState::Stale => WireSourceState::Stale,
        SourceState::Detached => WireSourceState::Detached,
    }
}

pub fn build(registry: &SourceRegistry) -> OverviewAggregates {
    let mut projects = Vec::with_capacity(PROJECTS.len());
    let mut connected = 0u32;
    for (id, label, domain) in PROJECTS {
        let st = registry.status(*id);
        let source = st.as_ref().map(|s| s.state).unwrap_or(SourceState::NotConnected);
        if source == SourceState::Connected {
            connected += 1;
        }
        projects.push(ProjectSummary {
            project: *id as i32,
            label: (*label).to_string(),
            domain: (*domain).to_string(),
            source: wire_source(source) as i32,
            // Readiness kommt nur aus der Engine-Statusprojektion (F7). Bis dahin UNKNOWN.
            readiness: Readiness::Unknown as i32,
            live_authorized: None,
            active_bots: None,
            active_jobs: None,
            active_captures: None,
            warnings: Vec::new(),
            rpc_latency: None,
            finance: None,
            source_timestamp_unix_ns: 0,
            source_id: st.and_then(|s| s.source_id.map(|x| x.to_string())).unwrap_or_default(),
        });
    }
    OverviewAggregates {
        projects,
        global: Some(GlobalFinance {
            available: false,
            unavailable_reason: "no finalized cross-project valuation".to_string(),
            net_pnl_usd_micro_signed: None,
            total_capital_usd_micro: None,
        }),
        projects_connected: connected,
        computed_at_unix_ns: unix_ns_now(),
    }
}

/// Publiziert bei jeder Quellenänderung und zusätzlich periodisch (Freshness-Zeitstempel).
pub fn spawn_publisher(
    hub: Arc<StateHub>,
    registry: Arc<SourceRegistry>,
    cfg: &GatewayConfig,
    generation: u64,
) -> tokio::task::JoinHandle<()> {
    let interval = Duration::from_millis(cfg.overview_interval_ms.max(500));
    let source_id: Arc<str> = Arc::from(format!("gateway:{generation}"));
    tokio::spawn(async move {
        let mut version = registry.watch();
        let mut seq: u64 = 0;
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            seq += 1;
            let msg = build(&registry);
            let payload = Bytes::from(msg.encode_to_vec());
            if let Err(e) = hub.publish(StateUpdate {
                topic: TopicId::OverviewAggregates,
                generation,
                seq,
                source_timestamp_unix_ns: unix_ns_now(),
                source_id: source_id.clone(),
                payload,
            }) {
                warn!(error = %e, "overview publish failed");
            }
            tokio::select! {
                _ = ticker.tick() => {}
                r = version.changed() => { if r.is_err() { return; } }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overview_without_engines_is_honest() {
        let reg = SourceRegistry::new();
        let o = build(&reg);
        assert_eq!(o.projects.len(), 3);
        assert_eq!(o.projects_connected, 0);
        assert!(!o.global.as_ref().unwrap().available);
        for p in &o.projects {
            assert_eq!(p.source, WireSourceState::NotConnected as i32);
            assert_eq!(p.readiness, Readiness::Unknown as i32);
            assert_eq!(p.live_authorized, None);
            assert!(p.finance.is_none());
            assert!(p.active_bots.is_none());
        }
    }
}
