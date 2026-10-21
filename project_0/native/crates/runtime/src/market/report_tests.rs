// Included in observer tests: the native C0-E1 report (B7) over a complete V2 run fixture —
// live capture journal, an offline strict verification from cached finalized blocks, the
// Shadow evaluation journal and an opportunity row.
mod report_e2e {
    use super::super::strict_driver::{drive, DriverRequest, StrictBudget, NATIVE_PREFIX};
    use super::super::verification::{RpcCacheEntry, StrictScope};
    use super::*;
    use crate::analytics::journal::{self, Provenance};
    use crate::analytics::measurement::{
        journal as phase_journal, persist_manifest, MeasurementPlan, Phase,
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

    fn cache(dir: &Path, method: &str, params: Value, result: Value) {
        std::fs::create_dir_all(dir).unwrap();
        let key = json!({"method": method, "params": params});
        let name = solana_sdk::hash::hash(&serde_json::to_vec(&key).unwrap()).to_string();
        let bytes = serde_json::to_vec(&result).unwrap();
        let entry = RpcCacheEntry {
            method: method.into(),
            params: params.to_string(),
            result_sha256: solana_sdk::hash::hash(&bytes).to_bytes(),
            result: bytes,
        };
        journal::write_typed_value(
            &dir.join(format!("{name}.p0j")),
            journal::source::STRICT,
            &entry,
        )
        .unwrap();
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

    fn canonical_run(tag: u32, complete: bool) -> Fixture {
        let d = std::env::temp_dir().join(format!("p0_report_{}_{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let run = d.join("run_r");
        let paths = RunPaths::new(&run, RunLayout::V2);
        paths.create().unwrap();
        let plan = MeasurementPlan::new(60).unwrap();
        let clock = darkbot_common::timing::Clock::new();
        persist_manifest(&paths, "run_r", "test", &plan, clock).unwrap();
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
        cache(
            &cache_dir,
            "getSlot",
            json!([{"commitment":"finalized"}]),
            json!(90),
        );
        for lo in [10, 41, 73] {
            cache(
                &cache_dir,
                "getBlocks",
                json!([lo, 90, {"commitment":"finalized"}]),
                json!((lo..=90).collect::<Vec<u64>>()),
            );
        }
        for slot in 10u64..=90 {
            let txs = if slot == 30 { vec![tx.clone()] } else { vec![] };
            cache(
                &cache_dir,
                "getBlock",
                json!([slot,{"encoding":"json","transactionDetails":"full","rewards":false,"commitment":"finalized","maxSupportedTransactionVersion":1}]),
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
            let program_data = darkbot_project0::protocol::program_data_address(&program())
                .to_string();
            let audited = |slot| super::super::paths::Deployment {
                context_slot: slot,
                hash: darkbot_project0::protocol::PROGRAM_DATA_REFERENCE_HASH.into(),
                program_data: program_data.clone(),
            };
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
        let stored: qualification::QualificationRecord =
            journal::read_typed_value(&path).unwrap();
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
        assert!(old.failed.contains(&"VERIFICATION_NOT_CANONICAL".to_string()));
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
        // Without the post-capture deployment proof the same run is not qualified.
        let proof = super::super::strict_driver::deployment_proof_path(
            &f.run,
            &f.second.verification_spec_id,
            f.second.verification_result_id.as_deref().unwrap(),
        );
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
            ["VERIFICATION_NOT_CANONICAL", "DEPLOYMENT_CONTINUITY_UNPROVEN"]
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
}
