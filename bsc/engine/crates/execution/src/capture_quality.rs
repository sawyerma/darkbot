//! The Phase F quality verdict of one finished runtime run (contract §11
//! quality gate, §13 acceptance): the run manifest, the census of the run
//! alone, the replay proof and the runtime's last status combined.
//!
//! `CLEAN` only when none of §11 is unresolved: the chain is BSC; nothing
//! was lost (gap spans, oversize records, ingress drops, write errors); the
//! journal is intact and ended cleanly; the replay proof passes; the
//! canonical interval is certain (no resync, skipped block or unjoined
//! launch); the clock is not suspect; no sighting is undecodable and no Four
//! token derivation contradicts its canonical launch; no record is
//! unparsed. Reorgs the follower handled and Four launches without a proven
//! derivation are reported, not disqualifying (fail-closed knowns).
//!
//! `PHASE_F_ACCEPTANCE` is `MET` only for a `CLEAN` run of at least the
//! planned duration whose pending transactions came from the native geth
//! source (public WSS evidence is secondary, contract §2).

use crate::g04_census::census_of_run;
use crate::replay_proof::{Proof, prove};
use crate::runtime_journal::RUN_MANIFEST;
use darkbot_bsc_record::schema::bsc_capture::{capture_quality as cq, capture_run as cr};
use darkbot_bsc_record::schema::bsc_runtime::runtime_status as rs;
use darkbot_bsc_record::{Draft, decode};
use std::fs;
use std::path::Path;

/// The native geth ingress source's id.
const NATIVE_SOURCE_ID: &str = "BSC_INGRESS_GETH";

/// The quality of one run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quality {
    pub run_id: String,
    pub chain_id: u64,
    pub clean: bool,
    pub reasons: Vec<String>,
    pub accepted: bool,
    pub acceptance_reasons: Vec<String>,
    pub duration_s: u64,
    pub target_s: u64,
    pub native_source: bool,
    pub manifest_state: Option<String>,
    pub census_state: String,
    pub proof: Proof,
    pub lost_records: u64,
    pub ingress_dropped: u64,
    pub write_errors: u64,
    pub reorgs: u64,
    pub resyncs: u64,
    pub blocks_skipped: u64,
    pub unjoined: u64,
    pub unparsed_records: u64,
    pub clock_suspect: bool,
    pub four_mismatch: u64,
    pub four_no_derivation: u64,
}

/// Assesses the run `<journal_dir>/<run_id>/` against `target_s` (the
/// planned duration); the other inputs are the replay proof's.
#[allow(clippy::too_many_arguments)]
pub fn assess(
    journal_dir: &Path,
    run_id: &str,
    status: Option<&[u8]>,
    max_payload: usize,
    max_calldata: usize,
    config_sha256: Option<[u8; 32]>,
    own_build: Option<[u8; 32]>,
    target_s: u64,
) -> Result<Quality, String> {
    let proof = prove(
        journal_dir,
        run_id,
        status,
        max_payload,
        max_calldata,
        config_sha256,
        own_build,
    )?;
    let alone = census_of_run(journal_dir, run_id, max_payload)?;
    let window = alone
        .windows
        .first()
        .ok_or_else(|| format!("{run_id}: no census window"))?;
    let manifest = fs::read(journal_dir.join(run_id).join(RUN_MANIFEST))
        .ok()
        .and_then(|b| cr::decode_run(&b).ok());
    let m = |tag| {
        manifest
            .as_ref()
            .and_then(|m| m.opt_u64(tag).ok().flatten())
            .unwrap_or(0)
    };
    let chain_id = manifest
        .as_ref()
        .and_then(|m| m.u64(cr::CHAIN_ID).ok())
        .unwrap_or(0);
    // The native source delivered full pending transactions in this run.
    let native_source = status
        .and_then(|b| decode(&rs::SCHEMA, b).ok())
        .filter(|s| {
            s.u64(rs::STARTED_UNIX_MS)
                .is_ok_and(|v| v.to_string() == run_id)
        })
        .and_then(|s| s.nested_all(rs::SOURCES).ok())
        .is_some_and(|sources| {
            sources.iter().any(|x| {
                x.text(rs::source::SOURCE_ID).ok() == Some(NATIVE_SOURCE_ID)
                    && x.u64(rs::source::FIRST_FULL).is_ok_and(|n| n > 0)
            })
        });
    let mut q = Quality {
        run_id: run_id.to_string(),
        chain_id,
        clean: false,
        reasons: Vec::new(),
        accepted: false,
        acceptance_reasons: Vec::new(),
        duration_s: window.duration_ms / 1_000,
        target_s,
        native_source,
        manifest_state: manifest
            .as_ref()
            .and_then(|m| m.text(cr::STATE).ok().map(str::to_string)),
        census_state: window.state.to_string(),
        lost_records: m(cr::LOST_RECORDS),
        ingress_dropped: m(cr::INGRESS_DROPPED).max(window.ingress_dropped),
        write_errors: m(cr::WRITE_ERRORS),
        reorgs: window.reorgs,
        resyncs: window.resyncs,
        blocks_skipped: window.blocks_skipped,
        unjoined: window.unjoined,
        unparsed_records: window.unparsed_records,
        clock_suspect: alone.clock_suspect,
        four_mismatch: alone.four_mismatch,
        four_no_derivation: alone.four_no_derivation,
        proof,
    };
    let mut why = |bad: bool, reason: String| {
        if bad {
            q.reasons.push(reason);
        }
    };
    why(chain_id != 56, format!("chain id {chain_id}, not BSC (56)"));
    why(
        q.manifest_state.as_deref() != Some("COMPLETE"),
        format!(
            "run manifest {}",
            q.manifest_state.as_deref().unwrap_or("missing")
        ),
    );
    why(
        q.lost_records > 0,
        format!("{} records lost", q.lost_records),
    );
    why(
        q.ingress_dropped > 0,
        format!(
            "{} source events dropped by the ingress queue",
            q.ingress_dropped
        ),
    );
    why(
        q.write_errors > 0,
        format!("{} write errors", q.write_errors),
    );
    why(
        q.census_state != "COMPLETE",
        format!("journal {}", q.census_state),
    );
    why(q.resyncs > 0, format!("{} canonical resyncs", q.resyncs));
    why(
        q.blocks_skipped > 0,
        format!("{} canonical blocks not followed", q.blocks_skipped),
    );
    why(
        q.unjoined > 0,
        format!("{} launches never joined", q.unjoined),
    );
    why(
        q.unparsed_records > 0,
        format!("{} records unparsed", q.unparsed_records),
    );
    why(
        q.clock_suspect,
        "the local clock is suspect (negative fetch lag)".into(),
    );
    why(
        q.proof.undecodable > 0,
        format!("{} sightings undecodable", q.proof.undecodable),
    );
    why(
        q.four_mismatch > 0,
        format!(
            "{} Four derivations contradict the canonical token",
            q.four_mismatch
        ),
    );
    if q.proof.verdict != "PASS" {
        q.reasons.push(format!("replay proof {}", q.proof.verdict));
        let detail: Vec<String> = q
            .proof
            .reasons
            .iter()
            .map(|r| format!("replay: {r}"))
            .collect();
        q.reasons.extend(detail);
    }
    q.clean = q.reasons.is_empty();
    if !q.clean {
        q.acceptance_reasons.push("not CLEAN".into());
    }
    if !q.native_source {
        q.acceptance_reasons
            .push("no pending transaction from the native geth source".into());
    }
    if q.duration_s < q.target_s {
        q.acceptance_reasons.push(format!(
            "{} s shorter than the planned {} s",
            q.duration_s, q.target_s
        ));
    }
    q.accepted = q.acceptance_reasons.is_empty();
    Ok(q)
}

impl Quality {
    pub fn encode(&self, observed_unix_ms: u64) -> Result<Vec<u8>, String> {
        let mut d = Draft::new(&cq::SCHEMA);
        d.set(cq::RUN_ID, self.run_id.as_str())
            .set(cq::CHAIN_ID, self.chain_id)
            .set(cq::PROVENANCE_TOOL, "bsc_runtime capture-quality")
            .set(cq::PROVENANCE_OBSERVED_UNIX_MS, observed_unix_ms)
            .set(cq::VERDICT, if self.clean { "CLEAN" } else { "NOT_CLEAN" })
            .set(
                cq::PHASE_F_ACCEPTANCE,
                if self.accepted { "MET" } else { "NOT_MET" },
            )
            .set(cq::DURATION_S, self.duration_s)
            .set(cq::TARGET_S, self.target_s)
            .set(cq::NATIVE_SOURCE, self.native_source)
            .set(cq::CENSUS_STATE, self.census_state.as_str())
            .set(cq::PROOF_VERDICT, self.proof.verdict)
            .set(cq::LOST_RECORDS, self.lost_records)
            .set(cq::INGRESS_DROPPED, self.ingress_dropped)
            .set(cq::WRITE_ERRORS, self.write_errors)
            .set(cq::REORGS, self.reorgs)
            .set(cq::RESYNCS, self.resyncs)
            .set(cq::BLOCKS_SKIPPED, self.blocks_skipped)
            .set(cq::UNJOINED, self.unjoined)
            .set(cq::UNPARSED_RECORDS, self.unparsed_records)
            .set(cq::CLOCK_SUSPECT, self.clock_suspect)
            .set(cq::UNDECODABLE, self.proof.undecodable)
            .set(cq::FOUR_MISMATCH, self.four_mismatch)
            .set(cq::FOUR_NO_DERIVATION, self.four_no_derivation)
            .push(
                cq::NON_CLAIMS,
                "NO_ECONOMIC_CLAIM: capture quality only; no BUY, no EV, no T0 claim",
            );
        if let Some(s) = &self.manifest_state {
            d.set(cq::MANIFEST_STATE, s.as_str());
        }
        for r in &self.reasons {
            d.push(cq::REASONS, r.as_str());
        }
        for r in &self.acceptance_reasons {
            d.push(cq::ACCEPTANCE_REASONS, r.as_str());
        }
        d.encode().map_err(|e| format!("capture quality: {e}"))
    }

    pub fn summary(&self) -> String {
        format!(
            "capture quality of run {}: {} (Phase F acceptance {}; {} s of {} s, native source {}, replay {})\n{}{}",
            self.run_id,
            if self.clean { "CLEAN" } else { "NOT_CLEAN" },
            if self.accepted { "MET" } else { "NOT_MET" },
            self.duration_s,
            self.target_s,
            self.native_source,
            self.proof.verdict,
            self.reasons.join("\n"),
            self.acceptance_reasons
                .iter()
                .map(|r| format!("\nacceptance: {r}"))
                .collect::<String>()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_journal::{
        EXIT_PAYLOAD, JournalSetup, RunIdentity, RuntimeJournal, ingress_drop_payload,
        window_payload,
    };
    use crate::sources::SourceId;
    use darkbot_bsc_journal::RecordKind;

    const CONFIG: Option<[u8; 32]> = Some([0xC0; 32]);
    const BUILD: Option<[u8; 32]> = Some([0xB0; 32]);

    fn status(started: u64, native_first_full: u64) -> Vec<u8> {
        use rs::source as e;
        let mut d = Draft::new(&rs::SCHEMA);
        d.set(rs::GENERATION, 1u64)
            .set(rs::PROVENANCE_TOOL, "bsc_runtime")
            .set(rs::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
            .set(rs::STARTED_UNIX_MS, started)
            .set(rs::LAUNCHES, 0u64)
            .set(rs::JOURNAL_PATH, "j")
            .set(rs::JOURNAL_RECORDS, 0u64)
            .set(rs::JOURNAL_LOST, 0u64)
            .set(rs::MODE, "READ_ONLY")
            .set(rs::STATUS_INTERVAL_MS, 1000u64);
        let mut row = Draft::new(&e::SCHEMA);
        row.set(e::SOURCE_ID, NATIVE_SOURCE_ID)
            .set(e::EFFECTIVE, "SHADOW")
            .set(e::HEALTH, "HEALTHY");
        for tag in [
            e::CONNECTS,
            e::DISCONNECTS,
            e::RATE_LIMITED,
            e::FULL,
            e::HASH_ONLY,
            e::REJECTED,
            e::DROPPED,
            e::DUPLICATES,
            e::FIRST_ANY,
            e::LAUNCH_SIGHTINGS,
            e::LAUNCH_FIRST,
        ] {
            row.set(tag, 0u64);
        }
        row.set(e::FIRST_FULL, native_first_full);
        d.push(rs::SOURCES, row.into_nested());
        d.encode().unwrap()
    }

    /// A clean run of `seconds` of record time, `drops` ingress drops.
    fn write(dir: &Path, seconds: u64, drops: u64) {
        let mut j = RuntimeJournal::open(dir, 7, 1, JournalSetup::whole(16, 4096)).unwrap();
        j.set_identity(RunIdentity {
            host: "test".into(),
            build: "test".into(),
            config_sha256: CONFIG.unwrap(),
            build_sha256: BUILD,
            build_dirty: Some(false),
            capture_pending: "LAUNCHES",
        });
        let end = seconds * 1_000_000_000;
        j.record(
            RecordKind::Canonical,
            1,
            &window_payload(b'H', 10, &[10; 32]),
        );
        if drops > 0 {
            j.record(
                RecordKind::Runtime,
                2,
                &ingress_drop_payload(SourceId::Geth, drops),
            );
            j.set_ingress_dropped(drops);
        }
        j.record(
            RecordKind::Canonical,
            end,
            &window_payload(b'E', 11, &[11; 32]),
        );
        j.record(RecordKind::Runtime, end, &EXIT_PAYLOAD);
        let _ = j.finish();
    }

    /// A clean 30-minute native run is accepted; the same run from public
    /// sources only, a shorter one and one with a loss are not, each with
    /// its reason.
    #[test]
    fn quality_and_acceptance_name_what_they_miss() {
        let base = std::env::temp_dir().join(format!("bsc-quality-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let (a, b, c) = (base.join("a"), base.join("b"), base.join("c"));
        write(&a, 1_800, 0);
        let q = assess(
            &a,
            "7",
            Some(&status(7, 5)),
            4096,
            16_384,
            CONFIG,
            BUILD,
            1_800,
        )
        .unwrap();
        assert!(q.clean && q.accepted, "{}", q.summary());
        let r = decode(&cq::SCHEMA, &q.encode(1).unwrap()).unwrap();
        assert_eq!(r.text(cq::VERDICT).unwrap(), "CLEAN");
        assert_eq!(r.text(cq::PHASE_F_ACCEPTANCE).unwrap(), "MET");
        // Public sources only: clean, not accepted.
        let q = assess(
            &a,
            "7",
            Some(&status(7, 0)),
            4096,
            16_384,
            CONFIG,
            BUILD,
            1_800,
        )
        .unwrap();
        assert!(q.clean && !q.accepted);
        assert_eq!(
            q.acceptance_reasons,
            ["no pending transaction from the native geth source"]
        );
        write(&b, 300, 0);
        let q = assess(
            &b,
            "7",
            Some(&status(7, 5)),
            4096,
            16_384,
            CONFIG,
            BUILD,
            1_800,
        )
        .unwrap();
        assert!(q.clean && !q.accepted);
        assert_eq!(
            q.acceptance_reasons,
            ["300 s shorter than the planned 1800 s"]
        );
        write(&c, 1_800, 4);
        let q = assess(
            &c,
            "7",
            Some(&status(7, 5)),
            4096,
            16_384,
            CONFIG,
            BUILD,
            1_800,
        )
        .unwrap();
        assert!(!q.clean && !q.accepted, "{}", q.summary());
        assert!(
            q.reasons
                .iter()
                .any(|r| r.contains("4 source events dropped"))
        );
        assert!(q.reasons.iter().any(|r| r.starts_with("replay proof FAIL")));
        fs::remove_dir_all(&base).unwrap();
    }
}
