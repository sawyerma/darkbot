// Included in observer tests: the native C0-E1 report (B7) over a complete V2 run fixture —
// live capture journal, an offline strict verification from cached finalized blocks, the
// Shadow evaluation journal and an opportunity row.
mod report_e2e {
    use super::super::reconcile::ChainRead;
    use super::super::strict_driver::{drive, DriverRequest, StrictBudget, NATIVE_PREFIX};
    use super::super::verification::{cache_fixture, StrictScope};
    use super::*;
    use crate::analytics::journal::{self, Provenance};
    use crate::analytics::measurement::{
        journal as phase_journal, persist_manifest_with, MeasurementPlan, Phase,
    };
    use crate::analytics::record::{Decision, OpportunityRecord, TimingEvidence};
    use crate::analytics::report::{self, Format, ProvenanceView, ReportQuery, View};
    use crate::run_layout::{RunLayout, RunPaths};

    const NS: u64 = 1_000_000_000;

    fn record(account: &str, rx_slot: u64) -> OpportunityRecord {
        OpportunityRecord {
            progress: Default::default(),
            market_plan_identity: None,
            timing: TimingEvidence {
                t_rx_ns: Some(10),
                t_state_ns: Some(15),
                t_detect_ns: Some(20),
                t_ready_ns: Some(30),
                t_sign_ns: Some(40),
                t_handoff_ns: None,
            },
            rx_slot: Some(rx_slot),
            detect_slot: Some(rx_slot),
            selected_costs: None,
            simulation_result: Some("PASS".into()),
            account: account.into(),
            trigger: "test".into(),
            decision: Decision::ShadowReady,
            net_profit: None,
            economics_known: false,
            premium_bps: None,
            t_profitable_unix_s: None,
            tx_version: Some("V0".into()),
            serialized_bytes: None,
            locked_accounts: None,
            path: None,
            funding: None,
            signature: None,
            sign_slot: Some(rx_slot),
            simulated_cu: None,
            candidate_costs: vec![],
            exposure: None,
            all_signatures: vec![],
            bundle_id: None,
            submitted_tip: None,
            intervals_ns: vec![],
            reason: None,
            loss: None,
        }
    }

    fn write_frames<P: crate::analytics::payload::Payload>(path: &Path, source: u32, rows: &[P]) {
        let mut w = journal::Writer::create(path).unwrap();
        for r in rows {
            w.append_typed(
                Provenance {
                    source,
                    ..Default::default()
                },
                r,
            )
            .unwrap();
        }
        w.seal(true).unwrap();
    }

    /// `dir` is the strict output's `cache/` directory.
    fn cache(dir: &Path, read: ChainRead, wire: Value) {
        cache_fixture(dir.parent().unwrap(), read, &wire);
    }

    fn hashes(run: &Path) -> BTreeMap<String, [u8; 32]> {
        let mut out = BTreeMap::new();
        let mut stack = vec![run.join("capture"), run.join("verification")];
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    out.insert(p.display().to_string(), journal::file_sha256(&p).unwrap());
                }
            }
        }
        out
    }

    fn rows(run: &Path, q: &ReportQuery) -> (report::Output, Vec<Vec<String>>, Vec<&'static str>) {
        let out = report::report(run, &report::Options::default(), q).unwrap();
        let ds = report::normalize(run, &report::Options::default()).unwrap();
        let (columns, cells) = report::cells(&ds.records, q);
        (out, cells, columns)
    }

    fn col(columns: &[&str], row: &[String], name: &str) -> String {
        row[columns.iter().position(|c| *c == name).unwrap()].clone()
    }

    /// The V2 run of the native report: manifest, CAPTURE phase, one receivership liquidation in
    /// the live journal, Shadow and opportunity rows, an incomplete first and a canonical resumed
    /// strict verification from cached finalized blocks. `complete`: full capture clock and live
    /// coverage, a passing final technical record and a clean owners' end (else cut short).
    struct Fixture {
        d: std::path::PathBuf,
        run: std::path::PathBuf,
        w_s: i64,
        target: String,
        first: super::super::strict_driver::DriverOutcome,
        second: super::super::strict_driver::DriverOutcome,
    }

    /// The audited Mainnet deployment as read at `slot`.
    fn audited(slot: u64) -> super::super::paths::Deployment {
        super::super::paths::Deployment {
            context_slot: slot,
            hash: darkbot_project0::protocol::PROGRAM_DATA_REFERENCE_HASH.into(),
            program_data: darkbot_project0::protocol::program_data_address(&program()).to_string(),
        }
    }

    fn canonical_run(tag: u32, complete: bool) -> Fixture {
        canonical_run_recording(tag, complete, "0")
    }

    /// `canonical_run` whose launcher recorded the minimum profit `min_profit` (USD-micro).
    fn canonical_run_recording(tag: u32, complete: bool, min_profit: &'static str) -> Fixture {
        let d = std::env::temp_dir().join(format!("p0_report_{}_{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let run = d.join("run_r");
        let paths = RunPaths::new(&run, RunLayout::V2);
        paths.create().unwrap();
        let plan = MeasurementPlan::new(60).unwrap();
        let clock = darkbot_common::timing::Clock::new();
        // The run records its minimum profit like every launcher run (capture.sh exports it).
        persist_manifest_with(&paths, "run_r", "test", &plan, clock, &|k| {
            (k == "P0_MIN_PROFIT").then(|| min_profit.into())
        })
        .unwrap();
        let window = plan.capture_window(clock, 1_000 * NS).unwrap();
        let w_s = (window.capture_start_unix_ns / u128::from(NS)) as i64;
        phase_journal(&paths, clock, Phase::Capture(window.clone())).unwrap();

        // One receivership liquidation (lone end_liquidation marker) with a keeper-close CPI.
        let keeper =
            bs58::encode(related::RelatedKind::KeeperCloseOrder.discriminator()).into_string();
        let mut tx = with_inner(gap_liquidation("liq-sig", 30, false));
        tx["meta"]["innerInstructions"] = json!([{"index": 0, "instructions": [
            {"programIdIndex": 1, "accounts": [2, 3, 4], "data": keeper, "stackHeight": 2}
        ]}]);
        let target = tx["transaction"]["message"]["accountKeys"][2]
            .as_str()
            .unwrap()
            .to_string();
        let chain_tx = darkbot_solana::chain::decode_transaction(&tx).unwrap();
        let market = paths.market_dir();
        std::fs::create_dir_all(&market).unwrap();
        write_frames(
            &market.join(MARKET_JOURNAL),
            journal::source::MARKET,
            &[rows::MarketTransaction {
                signature: "liq-sig".into(),
                slot: 30,
                origin: rows::TxOrigin::TransactionSubscribe {
                    observed_unix_s: w_s as u64 + 20,
                    t_rx_ns: 1_020 * NS,
                    tx: Ok(chain_tx),
                },
            }],
        );
        // A cut-short live capture stays incomplete whatever is verified later; a complete one
        // ran its full clock with complete transport coverage.
        journal::write_typed_value(
            &market.join(rows::MARKET_COMPLETION_JOURNAL),
            journal::source::MARKET,
            &rows::MarketCompletion {
                run_id: "run_r".into(),
                verdict: if complete {
                    "MARKET_DATA_SOURCE_COVERAGE_UNVERIFIED"
                } else {
                    "MARKET_DATA_INCOMPLETE"
                }
                .into(),
                stream_ready: true,
                full_duration_observed: complete,
                drain_complete: true,
                coverage_complete: true,
                decode_complete: true,
                receivership_transport_capture_complete: complete,
                full_slot_reconciliation_complete: false,
                // The capture alone never proves the deployment after the window.
                liquidation_path_coverage_complete: false,
                market_capture_complete: false,
                source_coverage_verified: false,
                incomplete_reasons: if complete {
                    vec![]
                } else {
                    vec!["OBSERVATION_ENDED_EARLY".into()]
                },
            },
        )
        .unwrap();
        // Shadow: one evaluation of the liquidated target, one opportunity without a chain entity.
        write_frames(
            &run.join("capture/shadow_evaluations.p0j"),
            journal::source::ECON,
            &[crate::analytics::econ::ShadowEvaluation {
                previous: None,
                // Inside the capture window [1000 s, 1060 s] of the monotone clock (A3: the join
                // admits exactly the evaluations the live chain admits).
                record: {
                    let mut r = record(&target, 29);
                    r.timing.t_rx_ns = Some(1_010 * NS);
                    r
                },
            }],
        );
        let mut reports = journal::Writer::create(
            &run.join("snapshots")
                .join(crate::analytics::econ::REPORT_JOURNAL),
        )
        .unwrap();
        let econ = Provenance {
            source: journal::source::ECON,
            ..Default::default()
        };
        reports
            .append_typed(
                econ.clone(),
                &crate::analytics::econ::OpportunityRow {
                    rec: record(&Pubkey::new_unique().to_string(), 31),
                    winner: None,
                    market: None,
                    slot_seen: None,
                    capital: vec![],
                    market_commitment: "finalized".into(),
                },
            )
            .unwrap();
        if complete {
            // The capture's market summary with its deployment read at the window start.
            let mut summary = crate::analytics::econ::tests::market_summary(60.0);
            summary.deployment_continuity.before = Some(audited(9));
            journal::write_typed_value(
                &market.join(MARKET_SUMMARY_JOURNAL),
                journal::source::MARKET,
                &MarketFinal::checkpoint(summary),
            )
            .unwrap();
            // The final technical record of a run whose every technical gate passed, and a
            // clean end of both owners.
            reports
                .append_typed(
                    econ,
                    &crate::analytics::econ::tests::passing_final_report("run_r", &plan),
                )
                .unwrap();
            phase_journal(
                &paths,
                clock,
                Phase::OwnersFinished {
                    aborted: false,
                    shadow_failed: false,
                    market_failed: false,
                },
            )
            .unwrap();
        }
        reports.seal(true).unwrap();

        // Strict: a first offline attempt has no cached blocks (incomplete); the resumed one
        // verifies slots 10..=80 from cached finalized blocks bracketing the capture window.
        let budget = StrictBudget {
            rps: 2,
            max_calls: 1000,
            max_seconds: 60,
            max_bytes: 1 << 24,
            max_disk_bytes: 1 << 30,
            max_ram_bytes: 1 << 34,
            min_free_bytes: 1,
        };
        let req = |resume| DriverRequest {
            run: run.clone(),
            budget: budget.clone(),
            offline: true,
            resume,
            mode: super::super::strict_driver::VerifyMode::Strict,
            provider_evidence: None,
        };
        let first = drive(&req(false)).unwrap();
        assert_eq!(first.status, "HISTORICAL_INCOMPLETE");
        let strict_dir = run
            .join("verification")
            .join(format!("{NATIVE_PREFIX}{}", first.verification_spec_id))
            .join("strict");
        let cache_dir = strict_dir.join("cache");
        let _ = std::fs::remove_dir_all(&cache_dir);
        journal::write_typed_value(
            &strict_dir.join("scope.p0j"),
            journal::source::STRICT,
            &StrictScope {
                verification_spec_id: first.verification_spec_id.clone(),
                start_ns: window.capture_start_unix_ns as u64,
                end_ns: window.capture_cutoff_unix_ns as u64,
                lo: 10,
                hi: 80,
            },
        )
        .unwrap();
        cache(&cache_dir, ChainRead::FinalizedSlot, json!(90));
        for lo in [10, 41, 73] {
            cache(
                &cache_dir,
                ChainRead::Blocks { start: lo, end: 90 },
                json!((lo..=90).collect::<Vec<u64>>()),
            );
        }
        for slot in 10u64..=90 {
            let txs = if slot == 30 { vec![tx.clone()] } else { vec![] };
            cache(
                &cache_dir,
                ChainRead::FullBlock { slot },
                json!({"parentSlot": slot - 1, "previousBlockhash": format!("h{}", slot - 1),
                    "blockhash": format!("h{slot}"), "blockTime": w_s - 1 + (slot as i64 - 10),
                    "transactions": txs}),
            );
        }
        let second = drive(&req(true)).unwrap();
        assert_eq!(second.status, "CANONICAL_VERIFIED", "{:?}", second.reason);
        if complete {
            // The post-capture deployment proof of the canonical result (C0E1-P4-RESUME): the
            // audited build before and after the window, as `paths::verified` judges it.
            let (before, after) = (audited(9), audited(95));
            let verified = super::super::paths::verified(Some(&before), Some(&after), Some(80));
            let rid = second.verification_result_id.clone().unwrap();
            journal::write_typed_value(
                &super::super::strict_driver::deployment_proof_path(
                    &run,
                    &second.verification_spec_id,
                    &rid,
                ),
                journal::source::STRICT,
                &super::super::strict_driver::DeploymentProof {
                    verification_result_id: rid.clone(),
                    end_slot: 80,
                    before: Some(before),
                    after: Some(after),
                    error: None,
                    verified,
                },
            )
            .unwrap();
        }
        Fixture {
            d,
            run,
            w_s,
            target,
            first,
            second,
        }
    }

    #[test]
    fn native_report_binds_sources_and_filters_only_the_view() {
        let Fixture {
            d,
            run,
            w_s,
            target,
            first,
            second,
        } = canonical_run(line!(), false);

        // Default query: strict result, entity once, related hidden, Shadow-only hidden.
        let q = ReportQuery::default();
        let (out, cells, columns) = rows(&run, &q);
        let ds = &out.dataset;
        assert_eq!(ds.verification_status, "CANONICAL_VERIFIED");
        assert_eq!(ds.report_status, "FINAL_VERIFICATION_RESULT");
        assert_eq!(ds.live_coverage_status, "LIVE_COVERAGE_INCOMPLETE");
        assert_eq!(
            ds.identity.verification_result_id,
            second.verification_result_id
        );
        assert!(
            ds.economics_replay.starts_with("PASS events=1"),
            "{}",
            ds.economics_replay
        );
        assert!(!ds.c0_e1_master_pass);
        let liq: Vec<_> = cells
            .iter()
            .filter(|r| col(&columns, r, "record_kind") == "LIQUIDATION")
            .collect();
        assert_eq!(liq.len(), 1);
        let l = liq[0];
        assert_eq!(col(&columns, l, "source"), "strict");
        assert_eq!(col(&columns, l, "liquidation_type"), "RECEIVERSHIP");
        assert_eq!(col(&columns, l, "markers"), "end_liquidation");
        assert_eq!(
            col(&columns, l, "pair_status"),
            "RECEIVERSHIP:INCOMPLETE_OR_AMBIGUOUS"
        );
        assert_eq!(col(&columns, l, "chain_liquidation_confirmed"), "true");
        assert_eq!(col(&columns, l, "target"), target);
        assert_eq!(col(&columns, l, "shadow_origin"), "STRICT_REPLAY");
        assert_eq!(col(&columns, l, "shadow_identity"), "PLAN_IDENTITY_UNKNOWN");
        assert_eq!(
            col(&columns, l, "executable_shadow_net_edge_usd"),
            "UNKNOWN"
        );
        assert_eq!(col(&columns, l, "time_source"), "BLOCK_TIME");
        assert!(col(&columns, l, "missing_reasons").contains("LIVE_COVERAGE_INCOMPLETE"));
        assert!(cells.iter().all(|r| !matches!(
            col(&columns, r, "record_kind").as_str(),
            "RELATED_NON_LIQUIDATION" | "SHADOW_ONLY"
        )));
        assert!(out
            .meta
            .contains("liquidations_chain_confirmed CLASSIC=0 RECEIVERSHIP=1"));
        assert!(out.meta.contains("shadow_only_hypothetical=0"));

        // Filters change the view only: capture and verifier evidence and the dataset are
        // unchanged by any query; the query identity is reproducible and query-specific.
        let before = hashes(&run);
        let mut all = q.clone();
        all.provenance = ProvenanceView::All;
        all.related_events = true;
        all.markers = vec![];
        all.format = Format::Csv;
        let (out_all, cells_all, columns_all) = rows(&run, &all);
        assert_eq!(hashes(&run), before);
        assert_eq!(out_all.dataset.records_sha256, ds.records_sha256);
        assert_ne!(
            out_all.query.report_query_identity,
            out.query.report_query_identity
        );
        assert_eq!(
            report::report(&run, &report::Options::default(), &q)
                .unwrap()
                .query
                .report_query_identity,
            out.query.report_query_identity
        );
        let qpath = run
            .join(report::REPORT_DIR)
            .join(format!("query-{}.p0j", out.query.report_query_identity));
        let kept: report::QueryRecord = journal::read_typed_value(&qpath).unwrap();
        assert_eq!(kept, out.query);
        // `all`: the entity once with both sources; hiding every marker keeps the class.
        let liq_all: Vec<_> = cells_all
            .iter()
            .filter(|r| col(&columns_all, r, "record_kind") == "LIQUIDATION")
            .collect();
        assert_eq!(liq_all.len(), 1);
        assert_eq!(col(&columns_all, liq_all[0], "source"), "strict+live");
        assert_eq!(
            col(&columns_all, liq_all[0], "liquidation_type"),
            "RECEIVERSHIP"
        );
        assert_eq!(col(&columns_all, liq_all[0], "markers"), "-");
        assert!(cells_all
            .iter()
            .all(|r| col(&columns_all, r, "record_kind") != "INSTRUCTION_MARKER"));
        // Related: listed once, never a liquidation, winner or profit.
        let related: Vec<_> = cells_all
            .iter()
            .filter(|r| col(&columns_all, r, "record_kind") == "RELATED_NON_LIQUIDATION")
            .collect();
        assert_eq!(related.len(), 1);
        assert_eq!(
            col(&columns_all, related[0], "related_type"),
            "marginfi_account_keeper_close_order"
        );
        assert_eq!(col(&columns_all, related[0], "source"), "strict+live");
        assert_eq!(
            col(&columns_all, related[0], "chain_liquidation_confirmed"),
            "false"
        );
        assert_eq!(col(&columns_all, related[0], "winner"), "-");
        assert!(out_all
            .meta
            .contains("liquidations_chain_confirmed CLASSIC=0 RECEIVERSHIP=1"));
        assert!(out_all
            .meta
            .contains("related_non_liquidation marginfi_account_keeper_close_order SUCCESS=1"));
        // Shadow-only: separate and hypothetical.
        let shadow: Vec<_> = cells_all
            .iter()
            .filter(|r| col(&columns_all, r, "record_kind") == "SHADOW_ONLY")
            .collect();
        assert_eq!(shadow.len(), 1);
        assert_eq!(
            col(&columns_all, shadow[0], "classification"),
            "HYPOTHETICAL"
        );
        assert_eq!(
            col(&columns_all, shadow[0], "chain_liquidation_confirmed"),
            "false"
        );
        assert!(out_all.meta.contains("shadow_only_hypothetical=1"));
        // Table and CSV render the same cells.
        let csv: Vec<Vec<String>> = out_all
            .data
            .lines()
            .skip(1)
            .map(|l| l.split(',').map(str::to_string).collect())
            .collect();
        assert_eq!(csv, cells_all);
        let mut table_q = all.clone();
        table_q.format = Format::Table;
        let table = report::report(&run, &report::Options::default(), &table_q).unwrap();
        let parsed: Vec<Vec<String>> = table
            .data
            .lines()
            .skip(1)
            .map(|l| l.split_whitespace().map(str::to_string).collect())
            .collect();
        assert_eq!(parsed, cells_all);

        // Views and provenance: live shows only live evidence; shadow never chain facts.
        let mut live = q.clone();
        live.provenance = ProvenanceView::Live;
        live.view = View::Liquidations;
        let (_, live_cells, lc) = rows(&run, &live);
        let l = live_cells
            .iter()
            .find(|r| col(&lc, r, "record_kind") == "LIQUIDATION")
            .unwrap();
        assert_eq!(col(&lc, l, "chain_liquidation_confirmed"), "false");
        assert_eq!(col(&lc, l, "winner"), "-");
        assert!(col(&lc, l, "missing_reasons").contains("CHAIN_EXECUTION_UNQUALIFIED"));
        let mut sh = q.clone();
        sh.provenance = ProvenanceView::Shadow;
        let (_, sh_cells, sc) = rows(&run, &sh);
        assert_eq!(sh_cells.len(), 2);
        assert!(sh_cells
            .iter()
            .all(|r| col(&sc, r, "record_kind") == "SHADOW_ONLY"
                && col(&sc, r, "chain_liquidation_confirmed") == "false"
                && col(&sc, r, "winner") == "-"));
        // A time window outside the capture hides every timed row.
        let mut late = q.clone();
        late.from_unix_ns = Some(i128::from(w_s + 3600) * 1_000_000_000);
        assert!(rows(&run, &late).1.is_empty());

        // The first (incomplete) result can be selected explicitly: provisional, unconfirmed.
        let opts = report::Options {
            verification_result: first.verification_result_id.clone(),
            ..Default::default()
        };
        let old = report::report(&run, &opts, &q).unwrap();
        assert_eq!(old.dataset.report_status, "PROVISIONAL/INCOMPLETE");
        assert!(old
            .meta
            .contains("liquidations_chain_confirmed CLASSIC=0 RECEIVERSHIP=0"));
        assert!(old
            .meta
            .contains("liquidations_unconfirmed CLASSIC=0 RECEIVERSHIP=1"));
        assert_ne!(old.dataset.records_sha256, ds.records_sha256);

        // Changed capture evidence fails closed.
        std::fs::write(run.join("capture/late_file.bin"), b"x").unwrap();
        assert_eq!(
            report::normalize(&run, &report::Options::default()).unwrap_err(),
            "CAPTURE_EVIDENCE_CHANGED"
        );
        std::fs::remove_dir_all(d).unwrap();
    }

    /// C0E1-P4-VC / V21: the separate master qualification over the persisted run. A cut-short
    /// capture without a final technical record or owners' end is verified canonically yet never
    /// qualified, and the result names exactly why; the same inputs give the same immutable
    /// record (never overwritten by a different one); another verification result is another
    /// qualification. The complete chain — full capture clock and live coverage, the canonical
    /// result, a canonical economics replay, a passing final technical record and a clean
    /// owners' end — is `C0_E1_MASTER_PASS`, while the report itself still never claims it.
    #[test]
    fn c0e1_master_qualification_binds_the_persisted_run() {
        use crate::analytics::qualification::{self, MASTER_PASS, NOT_QUALIFIED};
        let opts = report::Options::default();
        let f = canonical_run(line!(), false);
        let (q, path) = qualification::qualify(&f.run, &opts).unwrap();
        assert_eq!(q.verdict, NOT_QUALIFIED);
        assert_eq!(q.verification, "CANONICAL_VERIFIED");
        assert_eq!(q.economics, "CANONICAL_ECONOMICS_REPLAYED");
        assert_eq!(
            q.failed,
            [
                "CAPTURE_CLOCK_INCOMPLETE",
                "LIVE_COVERAGE_INCOMPLETE",
                "DEPLOYMENT_CONTINUITY_UNPROVEN",
                "ECON_FINAL_MISSING",
                "OWNERS_END_UNKNOWN"
            ]
        );
        assert_eq!(q.inputs.capture_run_id, "run_r");
        assert_eq!(
            q.inputs.verification_result_id,
            f.second.verification_result_id
        );
        assert!(path.starts_with(f.run.join(qualification::QUALIFICATION_DIR)));
        let stored: qualification::QualificationRecord = journal::read_typed_value(&path).unwrap();
        assert_eq!(stored, q);
        // Reproducible and immutable.
        let bytes = std::fs::read(&path).unwrap();
        let (again, again_path) = qualification::qualify(&f.run, &opts).unwrap();
        assert_eq!(
            (&again.qualification_id, &again_path),
            (&q.qualification_id, &path)
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        // The incomplete first result is another qualification, never canonical.
        let first_opts = report::Options {
            verification_result: f.first.verification_result_id.clone(),
            ..Default::default()
        };
        let (old, _) = qualification::qualify(&f.run, &first_opts).unwrap();
        assert_ne!(old.qualification_id, q.qualification_id);
        assert_eq!(old.verdict, NOT_QUALIFIED);
        assert!(old
            .failed
            .contains(&"VERIFICATION_NOT_CANONICAL".to_string()));
        // A stored result is never replaced by a different one for the same inputs.
        let mut forged = q.clone();
        forged.verdict = MASTER_PASS.into();
        forged.failed.clear();
        std::fs::remove_file(&path).unwrap();
        journal::write_typed_value(&path, journal::source::REPORT, &forged).unwrap();
        assert_eq!(
            qualification::qualify(&f.run, &opts).unwrap_err(),
            "QUALIFICATION_RESULT_CHANGED"
        );
        std::fs::remove_dir_all(&f.d).unwrap();

        // The complete chain qualifies.
        let f = canonical_run(line!(), true);
        let (q, _) = qualification::qualify(&f.run, &opts).unwrap();
        assert_eq!(q.failed, Vec::<String>::new());
        assert_eq!(q.verdict, MASTER_PASS);
        assert_eq!(
            (
                q.capture_clock.as_str(),
                q.live_coverage.as_str(),
                q.verification.as_str(),
                q.economics.as_str(),
                q.trading.as_str()
            ),
            (
                "CAPTURE_CLOCK_COMPLETE",
                "LIVE_COVERAGE_COMPLETE",
                "CANONICAL_VERIFIED",
                "CANONICAL_ECONOMICS_REPLAYED",
                "LIVE_DISABLED_NO_SUBMISSION"
            )
        );
        assert_eq!(
            (q.capture_journal.as_str(), q.deployment_continuity.as_str()),
            ("CAPTURE_JOURNAL_COMPLETE", "DEPLOYMENT_CONTINUITY_PROVEN")
        );
        assert_eq!(q.canonical_events_in_window, Some(1));
        assert!(q.inputs.econ_final_frame_sha256.is_some());
        assert_eq!(
            q.inputs.owners,
            Some(qualification::OwnersEnd {
                aborted: false,
                shadow_failed: false,
                market_failed: false
            })
        );
        assert!(q.inputs.deployment_proof_sha256.is_some());
        let out = report::report(&f.run, &opts, &ReportQuery::default()).unwrap();
        assert!(!out.dataset.c0_e1_master_pass);
        // A stored `verified = true` proves nothing by itself: a forged after-read, a slot bound
        // other than the result's last slot or a before-read other than the capture's is no
        // continuity.
        let rid = f.second.verification_result_id.clone().unwrap();
        let proof = super::super::strict_driver::deployment_proof_path(
            &f.run,
            &f.second.verification_spec_id,
            &rid,
        );
        let honest: super::super::strict_driver::DeploymentProof =
            journal::read_typed_value(&proof).unwrap();
        let forged = |f: &dyn Fn(&mut super::super::strict_driver::DeploymentProof)| {
            let mut p = honest.clone();
            f(&mut p);
            p.verified = true;
            p
        };
        for (case, p) in [
            (
                "after hash",
                forged(&|p| p.after.as_mut().unwrap().hash = "forged".into()),
            ),
            ("slot bound", forged(&|p| p.end_slot = 79)),
            ("before read", forged(&|p| p.before = Some(audited(8)))),
        ] {
            std::fs::remove_file(&proof).unwrap();
            journal::write_typed_value(&proof, journal::source::STRICT, &p).unwrap();
            let (forged_q, _) = qualification::qualify(&f.run, &opts).unwrap();
            assert_eq!(
                forged_q.failed,
                ["DEPLOYMENT_CONTINUITY_UNPROVEN"],
                "{case}"
            );
        }
        // Without the post-capture deployment proof the same run is not qualified.
        std::fs::remove_file(&proof).unwrap();
        let (unproven, _) = qualification::qualify(&f.run, &opts).unwrap();
        assert_eq!(unproven.failed, ["DEPLOYMENT_CONTINUITY_UNPROVEN"]);
        assert_ne!(unproven.qualification_id, q.qualification_id);
        // Only the canonical result qualifies the complete run.
        let first_opts = report::Options {
            verification_result: f.first.verification_result_id.clone(),
            ..Default::default()
        };
        let (old, _) = qualification::qualify(&f.run, &first_opts).unwrap();
        assert_eq!(old.verdict, NOT_QUALIFIED);
        // The deployment proof belongs to the canonical result only.
        assert_eq!(
            old.failed,
            [
                "VERIFICATION_NOT_CANONICAL",
                "DEPLOYMENT_CONTINUITY_UNPROVEN"
            ]
        );
        assert_eq!(old.economics, "ECONOMICS_REPLAYED_PROVISIONAL");
        std::fs::remove_dir_all(&f.d).unwrap();
    }

    #[test]
    fn report_arguments_follow_the_contract_and_refuse_json_and_index() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let inv = report::parse_args(&args(&[
            "--run",
            "r",
            "--report-types",
            "all,-BANKRUPTCY_SETTLEMENT",
            "--report-markers",
            "none,+end_liquidation",
            "--report-related-events",
            "on",
            "--report-view",
            "economics",
            "--report-from",
            "2026-10-17T12:00:00Z",
            "--report-to",
            "2026-10-17T13:00:00.5+00:00",
            "--report-min-net-usd",
            "-1.25",
            "--format",
            "csv",
        ]))
        .unwrap();
        let q = inv.query;
        assert_eq!(q.types.len(), 3);
        assert_eq!(q.markers, vec![markers::Marker::EndLiquidation]);
        assert!(q.related_events);
        assert_eq!(q.view, View::Economics);
        assert_eq!(q.min_net_usd_micro, Some(-1_250_000));
        assert_eq!(
            q.to_unix_ns.unwrap() - q.from_unix_ns.unwrap(),
            3_600_500_000_000
        );
        assert_eq!(q.from_unix_ns, Some(1_792_238_400_000_000_000));
        for bad in [
            vec!["--run", "r", "--format", "json"],
            vec!["--run", "r", "--report-provenance", "index"],
            vec!["--run", "r", "--report-markers", "start_liquidaton"],
            vec!["--run", "r", "--report-min-net-usd", "1.0000001"],
            vec!["--run", "r", "--report-from", "2026-10-17 12:00:00"],
            vec![
                "--run",
                "r",
                "--report-from",
                "2026-10-17T13:00:00Z",
                "--report-to",
                "2026-10-17T12:00:00Z",
            ],
            vec!["--report-view", "summary"],
        ] {
            assert!(report::parse_args(&args(&bad)).is_err(), "{bad:?}");
        }
        assert!(
            report::parse_args(&args(&["--run", "r", "--format", "json"]))
                .unwrap_err()
                .starts_with("REPORT_FORMAT_JSON_REFUSED")
        );
    }

    fn backtest_params() -> crate::analytics::backtest::Params {
        crate::analytics::backtest::Params {
            min_profit_usd_micro: 0,
            capital_usd_micro: vec![1_000_000_000],
            funding: vec![],
        }
    }

    const BACKTEST_BUDGET: crate::analytics::backtest::Budget =
        crate::analytics::backtest::Budget {
            max_seconds: 60,
            max_ram_bytes: 1 << 20,
            cancelled: None,
        };

    /// Every file under `dir` (relative path) — for the no-JSON and unchanged-evidence checks.
    fn files(dir: &Path) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    out.push(p.strip_prefix(dir).unwrap().display().to_string());
                }
            }
        }
        out.sort();
        out
    }

    /// The native backtest end to end over the V2 run: the typed capture, the strict result and
    /// the Shadow chain in, one sealed typed result journal out (header, one row per verified
    /// liquidation, summary). No JSON, JSONL or SQLite file exists before or after; capture and
    /// verification stay byte-identical; a rerun is the same job with the same hash; another
    /// parameter set is another job. The optimizer over the same rows is deterministic, bounded
    /// and, without an actionable row, chooses nothing (NO_SAMPLE).
    #[test]
    fn native_backtest_reads_only_typed_evidence_and_is_reproducible() {
        use crate::analytics::backtest;
        let f = canonical_run(line!(), true);
        let before = hashes(&f.run);
        let p = backtest_params();
        let a = backtest::backtest(&f.run, None, &p, BACKTEST_BUDGET).unwrap();
        assert_eq!(a.summary.rows, 1);
        assert_eq!(a.summary.observed.liquidations, 1);
        assert_eq!(a.rows[0].signature, "liq-sig");
        assert_eq!(a.rows[0].target, f.target);
        assert_eq!(a.header.verification_status, "CANONICAL_VERIFIED");
        assert_eq!(
            Some(a.header.verification_result_id.clone()),
            f.second.verification_result_id
        );
        // The receivership names no bank pair: the evaluation is a diagnostic, never a match.
        assert_eq!(a.summary.counterfactual.matched, 0);
        assert_eq!(
            a.rows[0].facts.loss_provenance,
            "TARGET_EPISODE_DIAGNOSTIC; executable identity unproven"
        );
        assert_eq!(a.summary.realized, backtest::REALIZED_NOT_RUN);
        let again = backtest::backtest(&f.run, None, &p, BACKTEST_BUDGET).unwrap();
        assert_eq!(again.summary, a.summary);
        assert_eq!(again.path, a.path);
        let mut r = journal::Reader::open(&a.path).unwrap();
        let mut kinds = Vec::new();
        while let Some(frame) = r.next_frame().unwrap() {
            kinds.push(frame.kind);
        }
        assert!(r.sealed && r.complete);
        use crate::analytics::journal::kind;
        assert_eq!(
            kinds,
            [
                kind::BACKTEST_HEADER,
                kind::BACKTEST_ROW,
                kind::BACKTEST_SUMMARY,
                journal::MANIFEST
            ]
        );
        let other = backtest::Params {
            min_profit_usd_micro: 1,
            ..p.clone()
        };
        let b = backtest::backtest(&f.run, None, &other, BACKTEST_BUDGET).unwrap();
        assert_ne!(b.path, a.path);
        assert_ne!(b.summary.result_sha256, a.summary.result_sha256);
        let space = backtest::Space {
            min_profit_usd_micro: vec![0, 1],
            funding: vec![vec![]],
            capital_usd_micro: vec![1_000_000_000],
        };
        let (o1, p1) =
            backtest::optimize(&f.run, None, &p, &space, 5_000, 2, BACKTEST_BUDGET).unwrap();
        let (o2, p2) =
            backtest::optimize(&f.run, None, &p, &space, 5_000, 2, BACKTEST_BUDGET).unwrap();
        assert_eq!((o1.result_sha256, &p1), (o2.result_sha256, &p2));
        assert_eq!(o1.chosen, None, "no actionable training row: NO_SAMPLE");
        assert_eq!(o1.train_rows + o1.holdout_rows, 1);
        assert_eq!(
            backtest::optimize(&f.run, None, &p, &space, 5_000, 1, BACKTEST_BUDGET)
                .err()
                .as_deref(),
            Some("OPTIMIZE_GRID_OUT_OF_BOUNDS")
        );
        assert_eq!(hashes(&f.run), before, "capture and verification unchanged");
        for file in files(&f.run) {
            assert!(
                !(file.ends_with(".json") || file.ends_with(".jsonl") || file.contains(".sqlite")),
                "{file}"
            );
        }
        let _ = std::fs::remove_dir_all(f.d);
    }

    /// B03/U-9: a backtest and an optimization as jobs of the analytics job controller (the
    /// action of `p0ctl job start` and the GUI): the worker body runs the existing backtest once,
    /// records COMPLETED with the result identity of the direct run, refuses to run the job again
    /// and keeps the id; a cancelled job records CANCELLED without a result; the store lists both.
    #[test]
    fn b03_jobs_run_the_existing_backtest_once_and_record_the_outcome() {
        use crate::analytics::{backtest, jobs};
        let f = canonical_run(line!(), true);
        let project = f.d.join("project");
        let p = backtest_params();
        let request = |id: &str, spec| jobs::JobRequest {
            job_id: id.into(),
            run: f.run.display().to_string(),
            verification_result: None,
            max_seconds: BACKTEST_BUDGET.max_seconds,
            max_ram_bytes: BACKTEST_BUDGET.max_ram_bytes,
            spec,
            requested_unix_ms: Some(1),
        };
        let bt = request("bt-1", jobs::JobSpec::Backtest { params: p.clone() });
        jobs::submit(&project, &bt).unwrap();
        assert_eq!(jobs::status(&project, "bt-1").unwrap().state, jobs::JobState::Queued);
        assert_eq!(jobs::submit(&project, &bt).err().as_deref(), Some("JOB_EXISTS"));
        let (outcome, executed) = jobs::run(&project, "bt-1", None).unwrap();
        let direct = backtest::backtest(&f.run, None, &p, BACKTEST_BUDGET).unwrap();
        assert_eq!((outcome.state.as_str(), outcome.code.as_str()), (jobs::COMPLETED, ""));
        match &outcome.result {
            Some(jobs::JobResult::Backtest {
                result_id, summary, ..
            }) => {
                assert_eq!(result_id, &direct.header.job_id);
                assert_eq!(**summary, direct.summary);
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(executed, Some(jobs::Executed::Backtest(_))));
        match jobs::status(&project, "bt-1").unwrap().state {
            jobs::JobState::Done(o) => assert_eq!(*o, outcome),
            s => panic!("{s:?}"),
        }
        assert!(jobs::run(&project, "bt-1", None)
            .err()
            .unwrap()
            .starts_with("JOB_DONE"));
        let space = backtest::Space {
            min_profit_usd_micro: vec![0, 1],
            funding: vec![vec![]],
            capital_usd_micro: vec![1_000_000_000],
        };
        let mut opt = request(
            "opt-1",
            jobs::JobSpec::Optimize {
                baseline: p.clone(),
                space,
                holdout_bps: 5_000,
                max_points: 2,
                save_profile: true,
            },
        );
        opt.requested_unix_ms = Some(2);
        jobs::submit(&project, &opt).unwrap();
        let (cancelled, none) = jobs::run(&project, "opt-1", Some(|| true)).unwrap();
        assert!(none.is_none());
        assert_eq!(
            (cancelled.state.as_str(), cancelled.code.as_str(), &cancelled.result),
            (jobs::CANCELLED, backtest::CANCELLED, &None)
        );
        let listed: Vec<_> = jobs::list(&project, 10)
            .unwrap()
            .into_iter()
            .map(|s| (s.request.job_id, s.state.name().to_string()))
            .collect();
        assert_eq!(
            listed,
            [
                ("opt-1".to_string(), jobs::CANCELLED.to_string()),
                ("bt-1".to_string(), jobs::COMPLETED.to_string())
            ]
        );
        assert_eq!(jobs::list(&project, 1).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(f.d);
    }

    /// A08/F-02: the post-capture chain as a job of the controller (the GUI's verify action):
    /// verification off, the native report and the master qualification run once; the job
    /// records each stage, and the qualification record it wrote is the run's newest.
    #[test]
    fn a08_post_capture_job_runs_the_production_chain() {
        use crate::analytics::{jobs, qualification};
        let f = canonical_run(line!(), true);
        let project = f.d.join("project");
        let request = jobs::JobRequest {
            job_id: "pc-1".into(),
            run: f.run.display().to_string(),
            verification_result: None,
            max_seconds: 60,
            max_ram_bytes: 1 << 26,
            spec: jobs::JobSpec::PostCapture {
                chain_verify: "off".into(),
                offline: false,
                resume: false,
                provider_evidence: None,
            },
            requested_unix_ms: Some(1),
        };
        jobs::submit(&project, &request).unwrap();
        let (outcome, _) = jobs::run(&project, "pc-1", None).unwrap();
        assert_eq!(outcome.state, jobs::COMPLETED, "{}", outcome.code);
        let Some(jobs::JobResult::PostCapture {
            verification_ok,
            report_ok,
            verdict,
        }) = outcome.result
        else {
            panic!("{:?}", outcome.result)
        };
        assert!(verification_ok && report_ok);
        let record = qualification::latest(&f.run).unwrap().unwrap();
        assert_eq!(verdict, Some(record.verdict));
        let _ = std::fs::remove_dir_all(f.d);
    }

    /// Review 4: like the optimizer, a backtest never goes below the run's recorded minimum
    /// profit (nothing was evaluated there); at or above it, it runs.
    #[test]
    fn backtest_refuses_parameters_below_the_recorded_minimum_profit() {
        use crate::analytics::backtest;
        let f = canonical_run_recording(line!(), true, "5");
        let mut p = backtest_params();
        p.min_profit_usd_micro = 4;
        let refused = backtest::backtest(&f.run, None, &p, BACKTEST_BUDGET).err();
        assert!(
            refused
                .as_deref()
                .is_some_and(|e| e.starts_with("BACKTEST_PARAMS_BELOW_EVIDENCE")),
            "{refused:?}"
        );
        p.min_profit_usd_micro = 5;
        assert!(backtest::backtest(&f.run, None, &p, BACKTEST_BUDGET).is_ok());
        let _ = std::fs::remove_dir_all(f.d);
    }

    /// The backtest fails closed and writes nothing: an invalid or exceeded budget, an unknown
    /// verification result, a changed verified batch, a changed capture (a legacy JSONL file
    /// beside the Shadow journal) and a run without a verified result (BLOCKED).
    #[test]
    fn native_backtest_fails_closed_on_missing_or_changed_evidence() {
        use crate::analytics::backtest::{self, Budget};
        let p = backtest_params();
        let f = canonical_run(line!(), true);
        let refused = |run: &Path, wanted: Option<&str>, budget| {
            backtest::backtest(run, wanted, &p, budget).err().unwrap()
        };
        let none = Budget {
            max_seconds: 0,
            ..BACKTEST_BUDGET
        };
        assert_eq!(refused(&f.run, None, none), "BACKTEST_BUDGET_INVALID");
        let tiny = Budget {
            max_ram_bytes: 1,
            ..BACKTEST_BUDGET
        };
        assert_eq!(refused(&f.run, None, tiny), "BACKTEST_RAM_BUDGET");
        assert_eq!(
            refused(&f.run, Some("0000"), BACKTEST_BUDGET),
            "RESULT_NOT_FOUND_OR_AMBIGUOUS"
        );
        let strict = f
            .run
            .join("verification")
            .join(format!("{NATIVE_PREFIX}{}", f.second.verification_spec_id))
            .join("strict");
        let batch = std::fs::read_dir(&strict)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .find(|p| {
                p.is_file()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("batch_") && n.ends_with(".p0j"))
            })
            .unwrap();
        let mut bytes = std::fs::read(&batch).unwrap();
        bytes.push(0);
        std::fs::write(&batch, bytes).unwrap();
        assert_eq!(
            refused(&f.run, None, BACKTEST_BUDGET),
            "RESULT_EVIDENCE_CHANGED"
        );

        let g = canonical_run(line!(), true);
        std::fs::write(g.run.join("capture/shadow_evaluations.jsonl"), b"{}\n").unwrap();
        assert_eq!(
            refused(&g.run, None, BACKTEST_BUDGET),
            "CAPTURE_EVIDENCE_CHANGED"
        );

        let h = canonical_run(line!(), true);
        std::fs::remove_dir_all(h.run.join("verification")).unwrap();
        assert_eq!(
            refused(&h.run, None, BACKTEST_BUDGET),
            "BACKTEST_BLOCKED: NO_VERIFIED_RESULT"
        );
        for run in [&f.run, &g.run, &h.run] {
            assert!(
                !run.join(backtest::BACKTEST_DIR).exists(),
                "nothing written"
            );
        }
        for d in [f.d, g.d, h.d] {
            let _ = std::fs::remove_dir_all(d);
        }
    }
}
