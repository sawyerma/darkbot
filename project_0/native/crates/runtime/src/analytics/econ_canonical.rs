//! Final market-driven analytics over the existing production evaluation journal.
//! No RPC, price model, candidate selection or execution is implemented here.
use super::*;
use crate::analytics::record::LossReason;
use crate::market_observer::truth::TruthEvent;

#[derive(Clone, Serialize)]
pub(super) struct Joined {
    pub market: TruthEvent,
    pub target_seen: bool,
    pub target_seen_without_slot: bool,
    pub health_liquidatable: Option<bool>,
    pub candidates_created: Option<bool>,
    pub plan_validated: Option<bool>,
    pub built: Option<bool>,
    pub identity_status: &'static str,
    pub our: Option<OpportunityRecord>,
    pub diagnostic: Option<OpportunityRecord>,
}
fn any_evidence(old: Option<bool>, new: Option<bool>) -> Option<bool> {
    match (old, new) {
        (Some(a), Some(b)) => Some(a || b),
        (a, None) => a,
        (None, b) => b,
    }
}
fn identity_matches(event: &TruthEvent, rec: &OpportunityRecord) -> bool {
    let Some(id) = &rec.market_plan_identity else {
        return false;
    };
    rec.account == event.target
        && id.liquidation_type == event.liquidation_type.as_str()
        && event.asset_bank.as_deref() == Some(id.asset_bank.as_str())
        && event.liability_bank.as_deref() == Some(id.liability_bank.as_str())
}
fn technical(rec: &OpportunityRecord) -> bool {
    rec.timing.t_sign_ns.is_some()
        && rec.simulation_result.as_deref() == Some("PASS")
        && matches!(
            rec.decision,
            Decision::ShadowReady | Decision::ShadowTechnicalReady
        )
        && rec.timing.t_handoff_ns.is_none()
}
fn net(rec: Option<&OpportunityRecord>) -> Option<f64> {
    let rec = rec?;
    if !rec.economics_known {
        return None;
    }
    rec.selected_costs
        .as_ref()?
        .net_profit_usd_micro
        .map(|n| n as f64 / 1e6)
}
fn preferable(new: &OpportunityRecord, old: &OpportunityRecord) -> bool {
    match (technical(new), technical(old)) {
        (true, false) => true,
        (false, true) => false,
        (true, true) => new.timing.t_sign_ns < old.timing.t_sign_ns,
        (false, false) => new.timing.t_rx_ns > old.timing.t_rx_ns,
    }
}
fn race_class(row: &Joined) -> &'static str {
    match row
        .our
        .as_ref()
        .filter(|r| r.timing.t_sign_ns.is_some())
        .and_then(|r| r.sign_slot)
    {
        Some(s) if s < row.market.slot => "PRE_WINNER_SLOT_PROVEN",
        Some(s) if s == row.market.slot => "SAME_SLOT_UNKNOWN",
        Some(_) => "POST_WINNER_SLOT_PROVEN_LATE",
        None => "UNRESOLVED",
    }
}
fn margin(row: &Joined) -> Option<i128> {
    Some(i128::from(row.market.live_t_rx_ns?) - i128::from(row.our.as_ref()?.timing.t_sign_ns?))
}
fn loss(row: &Joined) -> &'static str {
    if !row.market.permissionless_profit_opportunity {
        return "ADMINISTRATIVE_SETTLEMENT";
    }
    if !row.target_seen {
        return if row.target_seen_without_slot {
            "UNKNOWN"
        } else {
            "NOT_DETECTED"
        };
    }
    let rec = row.our.as_ref().or(row.diagnostic.as_ref());
    if let Some(rec) = rec {
        if matches!(
            rec.decision,
            Decision::ShadowSimFailed | Decision::ShadowTechnicalSimFailed
        ) {
            return "SIMULATION";
        }
        if let Some(reason) = rec.loss {
            return match reason {
                LossReason::Health => "HEALTH_ERROR",
                LossReason::StateRace => "STATE_RACE",
                LossReason::MissingAta => "MISSING_ATA",
                LossReason::Routing => "ROUTING",
                LossReason::Funding => "FUNDING",
                LossReason::Bankruptcy => "BANKRUPTCY",
                LossReason::Economics | LossReason::EconomicSkip | LossReason::ThresholdTooHigh => {
                    "ECONOMICS"
                }
                LossReason::Build => "BUILD",
                _ => "UNKNOWN",
            };
        }
        if row.our.is_some() && technical(rec) {
            if !rec.economics_known {
                return "ECONOMICS";
            }
            if race_class(row) == "POST_WINNER_SLOT_PROVEN_LATE" {
                return "WINNER_TOO_EARLY";
            }
            return "NO_PROVEN_TECHNICAL_LOSS";
        }
        if rec.timing.t_ready_ns.is_some() && rec.timing.t_sign_ns.is_none() {
            return "SIGNING";
        }
    }
    "UNKNOWN"
}

impl Worker {
    /// Replay each relevant target once, so old coalesced representatives cannot hide
    /// a pre-winner plan. Selection uses canonical chain episodes + exact plan identity.
    pub(super) fn finalize_canonical(&mut self) -> Result<(), String> {
        let mut grouped: BTreeMap<String, Vec<TruthEvent>> = BTreeMap::new();
        for event in &self.canonical_market {
            grouped
                .entry(event.target.clone())
                .or_default()
                .push(event.clone());
        }
        let mut result = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(7200);
        let mut replayed = 0u64;
        for (target, mut events) in grouped {
            events.sort_by_key(|e| (e.slot, e.transaction_index, e.event_index));
            let mut rows: Vec<_> = events
                .into_iter()
                .map(|market| Joined {
                    market,
                    target_seen: false,
                    target_seen_without_slot: false,
                    health_liquidatable: None,
                    candidates_created: None,
                    plan_validated: None,
                    built: None,
                    identity_status: "NO_MATCH",
                    our: None,
                    diagnostic: None,
                })
                .collect();
            let mut cursor = self.shadow_heads.get(&target).copied();
            while let Some(at) = cursor {
                replayed += 1;
                if replayed > self.shadow_evaluations_streamed
                    || std::time::Instant::now() > deadline
                {
                    return Err("canonical replay budget".into());
                }
                let (_, previous, rec) = self.chain_frame(&target, at)?;
                assign_record(&mut rows, &rec);
                cursor = previous;
            }
            result.extend(rows);
        }
        result.sort_by_key(|r| {
            (
                r.market.slot,
                r.market.transaction_index,
                r.market.event_index,
            )
        });
        for row in &result {
            self.append_report(journal::kind::ECON_CANONICAL, &joined_value(row))?;
        }
        self.render_rows(
            journal::kind::ECON_CANONICAL,
            "canonical_market_shadow.jsonl",
        )?;
        self.canonical_joined = result;
        self.canonical_finalized = true;
        Ok(())
    }
    pub(super) fn canonical_metrics(&self, from: usize, through: usize) -> Value {
        let start = self
            .h
            .clock
            .unix_ns(self.h.shared.start_ns.load(Ordering::Acquire)) as i128;
        let hour = |r: &Joined| {
            r.market.block_time.and_then(|s| {
                let delta = i128::from(s) * 1_000_000_000 - start;
                u64::try_from(delta)
                    .ok()
                    .and_then(|ns| self.h.cfg.plan.chain_bucket(ns))
            })
        };
        let final_total = from == 0 && through == SNAPSHOT_COUNT;
        let rows: Vec<_> = self
            .canonical_joined
            .iter()
            .filter(|r| final_total || hour(r).is_some_and(|h| h >= from && h < through))
            .collect();
        let unassigned = self
            .canonical_joined
            .iter()
            .filter(|r| hour(r).is_none())
            .count();
        let seconds = self.h.cfg.plan.span_seconds(from, through);
        summarize(&rows, seconds, unassigned, self.canonical_finalized)
    }
}

fn assign_record(rows: &mut [Joined], rec: &OpportunityRecord) {
    let Some(rx) = rec.rx_slot else {
        for row in rows {
            row.target_seen_without_slot = true;
        }
        return;
    };
    // First next chain event closes this target's episode. Multiple events in that
    // slot are not ordered by wall time or guessed transaction send ordering.
    let Some(first) = rows.iter().position(|r| r.market.slot >= rx) else {
        return;
    };
    let slot = rows[first].market.slot;
    let end = first
        + rows[first..]
            .iter()
            .take_while(|r| r.market.slot == slot)
            .count();
    let matches = rows[first..end]
        .iter()
        .filter(|r| identity_matches(&r.market, rec))
        .count();
    for row in &mut rows[first..end] {
        row.target_seen = true;
        row.health_liquidatable =
            any_evidence(row.health_liquidatable, rec.progress.health_liquidatable);
        row.candidates_created =
            any_evidence(row.candidates_created, rec.progress.candidates_created);
        row.plan_validated = any_evidence(row.plan_validated, rec.progress.plan_validated);
        row.built = any_evidence(row.built, rec.progress.transaction_built);
        if row
            .diagnostic
            .as_ref()
            .is_none_or(|old| rec.timing.t_rx_ns > old.timing.t_rx_ns)
        {
            row.diagnostic = Some(rec.clone());
        }
        if identity_matches(&row.market, rec) {
            if matches != 1 {
                row.identity_status = "AMBIGUOUS_SAME_SLOT_EVENTS";
                row.our = None;
                continue;
            }
            row.identity_status = "TYPE_TARGET_BANK_PAIR_CANONICAL_EVENT";
            if row.our.as_ref().is_none_or(|old| preferable(rec, old)) {
                row.our = Some(rec.clone());
            }
        } else if row.our.is_none() && row.identity_status != "AMBIGUOUS_SAME_SLOT_EVENTS" {
            row.identity_status = if rec.market_plan_identity.is_none() {
                "PLAN_IDENTITY_UNKNOWN"
            } else if row.market.asset_bank.is_none() || row.market.liability_bank.is_none() {
                "MARKET_BANK_PAIR_UNKNOWN"
            } else {
                "PLAN_TYPE_OR_BANK_PAIR_MISMATCH"
            };
        }
    }
}

fn values(rows: &[&Joined], f: impl Fn(&TruthEvent) -> Option<f64>) -> Value {
    let known: Vec<_> = rows.iter().filter_map(|r| f(&r.market)).collect();
    json!({"count_known":known.len(),"count_unknown":rows.len()-known.len(),"distribution":distribution(known)})
}
fn summarize(rows: &[&Joined], seconds: f64, unassigned: usize, finalized: bool) -> Value {
    let mut market = serde_json::Map::new();
    let mut fees = BTreeMap::new();
    for row in rows {
        fees.entry(&row.market.signature)
            .or_insert(row.market.transaction_fee_lamports);
    }
    for kind in [
        "ALL",
        "CLASSIC",
        "RECEIVERSHIP",
        "FORCED_DELEVERAGE",
        "BANKRUPTCY_SETTLEMENT",
    ] {
        let group: Vec<_> = rows
            .iter()
            .copied()
            .filter(|r| kind == "ALL" || r.market.liquidation_type.as_str() == kind)
            .collect();
        let tx: BTreeSet<_> = group.iter().map(|r| &r.market.signature).collect();
        market.insert(kind.into(),json!({"events":group.len(),"transactions":tx.len(),"events_per_hour":if seconds>0.{Some(group.len() as f64*3600./seconds)}else{None},
            "seized_usd":values(&group,|e|e.seized_value_usd),"repaid_usd":values(&group,|e|e.repaid_value_usd),
            "gross_edge_usd":values(&group,|e|e.gross_edge_usd),"winner_net_edge_usd":values(&group,|e|e.winner_net_edge_usd),
            "native_amounts":"canonical_market_shadow.jsonl join.market; per mint/venue units, never summed across denominations"}));
    }
    let tradable: Vec<_> = rows
        .iter()
        .copied()
        .filter(|r| r.market.permissionless_profit_opportunity)
        .collect();
    let capital:Vec<_>=CAPITAL_USD.iter().map(|budget| {
        let mut detected=0;let mut exec=0;let mut funded=0;let mut unknown=0;let mut blocked=0;
        let mut nets=Vec::new();let mut lost=Vec::new();
        for row in &tradable {
            detected+=usize::from(row.target_seen);
            let rec=row.our.as_ref();
            let technical=rec.is_some_and(technical);exec+=usize::from(technical);
            let capacity=rec.and_then(|r|r.selected_costs.as_ref()).and_then(CandidateCostRecord::total_budget);
            let n=net(rec);
            if n.is_none() || capacity.is_none(){unknown+=1;}
            if technical {
                match capacity {
                    Some(v) if v<=budget*1_000_000 => {funded+=1;if let Some(n)=n.filter(|n|*n>0.){nets.push(n);}},
                    Some(_) if n.is_some_and(|n|n>0.) && race_class(row)=="PRE_WINNER_SLOT_PROVEN"=>{blocked+=1;lost.push(n.unwrap());},
                    _=>{},
                }
            }
        }
        json!({"budget_usd":budget,"market_opportunities":tradable.len(),"shadow_detected":detected,"executable":exec,
            "fundable":funded,"known_profitable":nets.len(),"unknown_economics":unknown,"known_net_edge_usd":distribution(nets),
            "lost_only_because_capital":blocked,"lost_edge_only_because_capital_usd":distribution(lost),
            "only_capital_semantics":"otherwise signed/simulated, positive known net, PRE winner slot; still not guaranteed capture"})
    }).collect();
    let mut reasons: BTreeMap<&str, Vec<&Joined>> = [
        "NOT_DETECTED",
        "HEALTH_ERROR",
        "STATE_RACE",
        "MISSING_ATA",
        "ROUTING",
        "FUNDING",
        "CAPITAL",
        "ECONOMICS",
        "BANKRUPTCY",
        "SIMULATION",
        "BUILD",
        "SIGNING",
        "WINNER_TOO_EARLY",
        "UNKNOWN",
    ]
    .into_iter()
    .map(|k| (k, vec![]))
    .collect();
    let mut race: BTreeMap<&str, Vec<&Joined>> = BTreeMap::new();
    let mut stages: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    let mut bottlenecks: BTreeMap<&str, Vec<&Joined>> = BTreeMap::new();
    let mut margins = Vec::new();
    let mut speed = Vec::new();
    for row in &tradable {
        reasons.entry(loss(row)).or_default().push(row);
        race.entry(race_class(row)).or_default().push(row);
        if let Some(rec) = &row.our {
            let intervals = stage_intervals(rec);
            // Exclude the overlapping end-to-end total from stage ranking.
            if let Some((stage, _)) = intervals
                .iter()
                .filter(|(k, _)| {
                    matches!(**k, "rx_to_detect" | "detect_to_ready" | "ready_to_sign")
                })
                .filter_map(|(k, v)| v.map(|n| (*k, n)))
                .max_by_key(|(_, n)| *n)
            {
                bottlenecks.entry(stage).or_default().push(row);
            }
            for (k, v) in intervals {
                if let Some(n) = v {
                    stages.entry(k).or_default().push(n as f64 / 1e6);
                }
            }
        }
        if let Some(n) = margin(row) {
            margins.push(n as f64 / 1e6);
            if n < 0 {
                speed.push((-n) as f64 / 1e6);
            }
        }
    }
    let loss_dollars:BTreeMap<_,_>=reasons.into_iter().map(|(reason,group)| {
        let ours:Vec<_>=group.iter().filter_map(|r|r.our.as_ref().filter(|r|technical(r)).and_then(|r|net(Some(r)))).collect();
        (reason,json!({"count":group.len(),"known_market_gross_edge_usd":values(&group,|e|e.gross_edge_usd),
            "known_our_executable_net_edge_usd":distribution(ours.clone()),"our_edge_unknown_count":group.len()-ours.len(),
            "interpretation":"opportunity attribution; not realized monetary loss"}))
    }).collect();
    let race:BTreeMap<_,_>=race.into_iter().map(|(k,v)|(k,json!({"count":v.len(),"known_net_edge_usd":distribution(v.iter().filter_map(|r|net(r.our.as_ref())).collect())}))).collect();
    let mut actors: BTreeMap<&str, (u64, Vec<f64>)> = BTreeMap::new();
    for row in &tradable {
        let a = actors.entry(&row.market.liquidator).or_default();
        a.0 += 1;
        if let Some(g) = row.market.gross_edge_usd {
            a.1.push(g);
        }
    }
    let mut actors: Vec<_> = actors
        .into_iter()
        .map(|(id, (count, edges))| (id, count, distribution(edges)))
        .collect();
    actors.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    json!({"finalized":finalized,"market":market,"authoritative_scope":"FINALIZED_ALL_PATH_MARKET_JOIN; production selected_costs only","market_window_hour_assignment":"blockTime against shared-clock UTC capture boundaries; boundary/out-of-window or unknown timestamps reported separately",
        "hour_unassigned_events_full_window":unassigned,"unassigned_included_in_final_total":true,
        "matching":{"events":rows.len(),"target_seen":rows.iter().filter(|r|r.target_seen).count(),"exact_plan_match":rows.iter().filter(|r|r.our.is_some()).count(),
            "target_seen_without_slot":rows.iter().filter(|r|r.target_seen_without_slot).count(),
            "identity_unresolved":rows.iter().filter(|r|r.target_seen && r.our.is_none()).count()},
        "network_fees":{"transactions":fees.len(),"known_transactions":fees.values().filter(|v|v.is_some()).count(),"unknown_transactions":fees.values().filter(|v|v.is_none()).count(),
            "known_total_lamports":fees.values().copied().flatten().try_fold(None,|sum:Option<u64>,n|Some(Some(sum.unwrap_or(0).checked_add(n)?))).flatten(),
            "priority_included_in_meta_fee":true,"unknown_never_zero":true},
        "capital":capital,"loss_attribution":loss_dollars,
        "largest_measured_stage":bottlenecks.into_iter().map(|(k,rows)|(k,json!({"events":rows.len(),"known_market_gross_edge_usd":values(&rows,|e|e.gross_edge_usd),"claim":"correlation with largest measured nonoverlapping stage; not proof of causal loss"}))).collect::<BTreeMap<_,_>>(),"race":race,"matched_stage_ms":stages.into_iter().map(|(k,v)|(k,distribution(v))).collect::<BTreeMap<_,_>>(),
        "local_margin_ms":distribution(margins),"required_local_speedup_ms":distribution(speed.clone()),
        "speedup_sensitivity":SPEEDUP_MS.iter().map(|ms|json!({"ms":ms,"local_boundaries_crossed":speed.iter().filter(|s|**s<=*ms as f64).count(),"claim":"COUNTERFACTUAL LOCAL BOUNDARY ANALYSIS"})).collect::<Vec<_>>(),
        "competition":{"liquidators":actors,"top_shares": ([1,3,5].iter().map(|n|json!({"top":n,"event_share":if tradable.is_empty(){None}else{Some(actors.iter().take(*n).map(|a|a.1).sum::<u64>() as f64/tradable.len() as f64)}})).collect::<Vec<_>>())},
        "true_competitor_submission_deadline":"UNKNOWN","live_ready":false})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_values_have_no_zero_total() {
        let s = summarize(&[], 3600., 0, true);
        assert!(s["network_fees"]["known_total_lamports"].is_null());
        assert_eq!(s["capital"].as_array().unwrap().len(), 6);
    }
}

#[cfg(test)]
mod regression {
    use super::super::tests::{costs, rec, worker};
    use super::*;
    use crate::analytics::record::MarketPlanIdentity;
    use crate::market_observer::truth::LiquidationType;
    fn row(r: &OpportunityRecord, slot: u64, index: u32) -> Joined {
        Joined {
            market: TruthEvent {
                signature: format!("tx-{slot}"),
                slot,
                transaction_index: 0,
                event_index: index,
                liquidation_type: LiquidationType::Receivership,
                target: r.account.clone(),
                liquidator: "winner".into(),
                liquidator_marginfi_account: None,
                asset_bank: Some("asset".into()),
                liability_bank: Some("liability".into()),
                asset_mint: None,
                liability_mint: None,
                seized_asset_native_exact: None,
                native_evidence: None,
                seized_value_usd: Some(105.),
                repaid_value_usd: Some(100.),
                gross_edge_usd: Some(5.),
                protocol_fee_lamports: Some(5000),
                transaction_fee_lamports: Some(7000),
                fee_payer: Some("payer".into()),
                visible_payments: Some(vec![]),
                incremental_rent_lamports: None,
                priority_fee_lamports: None,
                winner_net_edge_usd: None,
                winner_net_edge_complete: false,
                economic_provenance: "FIXTURE".into(),
                block_time: None,
                live_t_rx_ns: Some(50),
                discovery_source: "LIVE".into(),
                permissionless_profit_opportunity: true,
            },
            target_seen: false,
            target_seen_without_slot: false,
            health_liquidatable: None,
            candidates_created: None,
            plan_validated: None,
            built: None,
            identity_status: "NO_MATCH",
            our: None,
            diagnostic: None,
        }
    }
    fn identified() -> OpportunityRecord {
        let mut r = rec();
        r.market_plan_identity = Some(MarketPlanIdentity {
            liquidation_type: "RECEIVERSHIP".into(),
            asset_bank: "asset".into(),
            liability_bank: "liability".into(),
        });
        r.progress.health_liquidatable = Some(true);
        r.progress.candidates_created = Some(true);
        r.progress.plan_validated = Some(true);
        r.progress.transaction_built = Some(true);
        r
    }
    #[test]
    fn offline_replay_uses_same_selector_and_exact_odd_plan_boundaries() {
        let mut w = worker();
        let r = identified();
        w.accept(Evidence::Shadow(Box::new(r.clone()))).unwrap();
        let mut event = row(&r, 10, 0).market;
        event.block_time = Some(101);
        let input = w.h.cfg.run_dir.join("replay_market.jsonl");
        fs::write(
            &input,
            format!("{}\n", serde_json::to_string(&event).unwrap()),
        )
        .unwrap();
        let out = w.h.cfg.run_dir.join("offline_replay");
        let plan = MeasurementPlan::new(61).unwrap();
        replay(
            &input,
            &w.h.cfg.run_dir.join("shadow_evaluations.jsonl"),
            &out,
            &plan,
            100_000_000_000,
            30,
        )
        .unwrap();
        let got: Value = serde_json::from_str(
            fs::read_to_string(out.join("canonical_market_shadow.jsonl"))
                .unwrap()
                .trim(),
        )
        .unwrap();
        let mut expected = vec![row(&r, 10, 0)];
        expected[0].market = event;
        assign_record(&mut expected, &r);
        assert_eq!(
            got["economic_net_usd"],
            joined_value(&expected[0])["economic_net_usd"]
        );
        assert_eq!(
            got["join"]["identity_status"],
            "TYPE_TARGET_BANK_PAIR_CANONICAL_EVENT"
        );
        for i in 1..=4 {
            let v: Value =
                serde_json::from_reader(File::open(out.join(format!("hour_{i:02}.json"))).unwrap())
                    .unwrap();
            assert_eq!(v["cutoff_unix_ns"], 100_000_000_000u64 + plan.offset_ns(i));
            assert_eq!(v["cumulative"]["market"]["ALL"]["events"], 1);
            assert_eq!(v["master_pass"], false);
        }
        assert!(replay(
            &input,
            &w.h.cfg.run_dir.join("shadow_evaluations.jsonl"),
            &out,
            &plan,
            100_000_000_000,
            30
        )
        .is_err());
        // Runs before PERSIST-01 keep their JSONL stream: same replay, byte-identical renders.
        let legacy_dir = w.h.cfg.run_dir.join("legacy");
        fs::create_dir(&legacy_dir).unwrap();
        let legacy = legacy_dir.join("shadow_evaluations.jsonl");
        fs::write(&legacy, format!("{}\n", serde_json::to_string(&r).unwrap())).unwrap();
        let from_legacy = w.h.cfg.run_dir.join("offline_replay_legacy");
        replay(&input, &legacy, &from_legacy, &plan, 100_000_000_000, 30).unwrap();
        for name in ["canonical_market_shadow.jsonl", "economics_final.json"] {
            assert_eq!(
                fs::read(out.join(name)).unwrap(),
                fs::read(from_legacy.join(name)).unwrap(),
                "{name}"
            );
        }
        // Both sources at once are ambiguous, never merged or guessed.
        fs::copy(
            journal::binary_path(&w.h.cfg.run_dir.join("shadow_evaluations.jsonl")),
            journal::binary_path(&legacy),
        )
        .unwrap();
        assert_eq!(
            replay(
                &input,
                &legacy,
                &w.h.cfg.run_dir.join("ambiguous"),
                &plan,
                100_000_000_000,
                30
            )
            .err()
            .as_deref(),
            Some("AMBIGUOUS_SHADOW_SOURCE")
        );
    }
    #[test]
    fn exact_identity_and_chain_episode_prevent_cross_event_allocation() {
        let r = identified();
        let mut rows = vec![row(&r, 10, 0), row(&r, 20, 0)];
        assign_record(&mut rows, &r);
        assert!(rows[0].our.is_some());
        assert!(!rows[1].target_seen);
        let mut later = r.clone();
        later.rx_slot = Some(11);
        assign_record(&mut rows, &later);
        assert!(rows[1].our.is_some());
        let mut wrong = identified();
        wrong.account = r.account.clone();
        wrong.market_plan_identity.as_mut().unwrap().asset_bank = "wrong".into();
        let mut rows = vec![row(&r, 10, 0)];
        assign_record(&mut rows, &wrong);
        assert!(rows[0].target_seen);
        assert!(rows[0].our.is_none());
        assert_eq!(rows[0].identity_status, "PLAN_TYPE_OR_BANK_PAIR_MISMATCH");
    }
    #[test]
    fn same_slot_same_pair_ambiguous_but_distinct_pair_resolves() {
        let r = identified();
        let mut rows = vec![row(&r, 10, 0), row(&r, 10, 1)];
        assign_record(&mut rows, &r);
        assert!(rows
            .iter()
            .all(|r| r.our.is_none() && r.identity_status == "AMBIGUOUS_SAME_SLOT_EVENTS"));
        rows[1].market.asset_bank = Some("other".into());
        assign_record(&mut rows, &r);
        assert!(rows[0].our.is_some());
        assert!(rows[1].our.is_none());
    }
    #[test]
    fn unknown_identity_and_old_records_never_become_known_economics() {
        let r = rec();
        let mut rows = vec![row(&r, 10, 0)];
        assign_record(&mut rows, &r);
        let s = summarize(&[&rows[0]], 3600., 0, true);
        assert_eq!(s["matching"]["exact_plan_match"], 0);
        assert_eq!(s["capital"][0]["unknown_economics"], 1);
        assert!(s["capital"][0]["known_net_edge_usd"]["total"].is_null());
        let mut v = serde_json::to_value(r).unwrap();
        v.as_object_mut().unwrap().remove("progress");
        v.as_object_mut().unwrap().remove("market_plan_identity");
        let old: OpportunityRecord = serde_json::from_value(v).unwrap();
        assert!(old.progress.health_liquidatable.is_none());
        assert!(old.market_plan_identity.is_none());
        let mut no_slot = old;
        no_slot.rx_slot = None;
        let mut rows = vec![row(&no_slot, 10, 0)];
        assign_record(&mut rows, &no_slot);
        assert_eq!(loss(&rows[0]), "UNKNOWN");
        assert!(rows[0].our.is_none());
    }
    #[test]
    fn capital_uses_full_production_budget_and_excludes_admin() {
        let mut r = identified();
        r.selected_costs = Some(costs(200_000_000));
        let mut rows = vec![row(&r, 10, 0)];
        assign_record(&mut rows, &r);
        let s = summarize(&[&rows[0]], 3600., 0, true);
        assert_eq!(s["capital"][0]["fundable"], 0); // fee reserve makes exactly $200 insufficient
        assert_eq!(s["capital"][0]["lost_only_because_capital"], 1);
        assert_eq!(s["capital"][1]["fundable"], 1);
        assert_eq!(s["capital"][1]["known_profitable"], 1);
        rows[0].market.permissionless_profit_opportunity = false;
        rows[0].market.liquidation_type = LiquidationType::ForcedDeleverage;
        let s = summarize(&[&rows[0]], 3600., 0, true);
        assert_eq!(s["market"]["ALL"]["events"], 1);
        assert_eq!(s["capital"][0]["market_opportunities"], 0);
    }
    #[test]
    fn transaction_fee_dedup_unknown_costs_and_loss_dollars() {
        let r = identified();
        let a = row(&r, 10, 0);
        let mut b = row(&r, 10, 1);
        b.market.gross_edge_usd = None;
        let s = summarize(&[&a, &b], 3600., 0, true);
        assert_eq!(s["network_fees"]["transactions"], 1);
        assert_eq!(s["network_fees"]["known_total_lamports"], 7000);
        assert_eq!(s["loss_attribution"]["NOT_DETECTED"]["count"], 2);
        assert_eq!(
            s["loss_attribution"]["NOT_DETECTED"]["known_market_gross_edge_usd"]["distribution"]
                ["total"],
            5.
        );
        assert_eq!(
            s["loss_attribution"]["NOT_DETECTED"]["known_market_gross_edge_usd"]["count_unknown"],
            1
        );
        assert!(s["market"]["ALL"]["winner_net_edge_usd"]["distribution"]["total"].is_null());
        assert!(s["loss_attribution"]["SIMULATION"].is_object());
    }
    #[test]
    fn race_same_slot_never_proves_win_and_backfill_has_no_fake_margin() {
        let mut r = identified();
        r.sign_slot = Some(10);
        let mut rows = vec![row(&r, 10, 0)];
        assign_record(&mut rows, &r);
        assert_eq!(race_class(&rows[0]), "SAME_SLOT_UNKNOWN");
        assert_eq!(margin(&rows[0]), Some(10));
        rows[0].market.live_t_rx_ns = None;
        assert_eq!(margin(&rows[0]), None);
    }
    #[test]
    fn journal_replay_preserves_earliest_signed_record_and_hourly_final_reconcile() {
        let mut w = worker();
        let mut r = identified();
        let start = (w.h.clock.unix_ns(1) / 1_000_000_000) as i64 + 1;
        w.h.cfg.plan = MeasurementPlan::new(14400).unwrap();
        for i in 0..4 {
            let mut e = row(&r, 10 + i, 0).market;
            e.block_time = Some(start + i as i64 * 3600);
            w.canonical_market.push(e);
        }
        w.accept(Evidence::Shadow(Box::new(r.clone()))).unwrap();
        r.timing.t_sign_ns = Some(45);
        r.timing.t_rx_ns = Some(11);
        w.accept(Evidence::Shadow(Box::new(r))).unwrap();
        w.finalize_canonical().unwrap();
        assert_eq!(
            w.canonical_joined[0].our.as_ref().unwrap().timing.t_sign_ns,
            Some(40)
        );
        let sum: u64 = (1..=4)
            .map(|i| {
                w.canonical_metrics(i - 1, i)["market"]["ALL"]["events"]
                    .as_u64()
                    .unwrap()
            })
            .sum();
        assert_eq!(sum, 4);
        assert_eq!(w.canonical_metrics(0, 4)["market"]["ALL"]["events"], 4);
        assert!(w
            .h
            .cfg
            .run_dir
            .join("canonical_market_shadow.jsonl")
            .is_file());
    }
}

fn joined_value(row: &Joined) -> Value {
    json!({"join":row,"race_class":race_class(row),"local_observed_race_margin_ns":margin(row),"loss_reason":loss(row),
                    "signed":row.our.as_ref().map(|r|r.timing.t_sign_ns.is_some()),
                    "simulation_result":row.our.as_ref().and_then(|r|r.simulation_result.as_deref()),
                    "economic_net_usd":net(row.our.as_ref()),"economics_known":row.our.as_ref().map(|r|r.economics_known),
                    "fundable_by_budget":CAPITAL_USD.iter().map(|budget|json!({"budget_usd":budget,"fundable":row.our.as_ref().and_then(|r|r.selected_costs.as_ref()).and_then(CandidateCostRecord::total_budget).map(|n|n<=budget*1_000_000)})).collect::<Vec<_>>(),
                    "loss_provenance":if row.our.is_some(){"EXACT_PLAN_MATCH"}else if row.diagnostic.is_some(){"TARGET_EPISODE_DIAGNOSTIC; executable identity unproven"}else{"NO_TARGET_EPISODE_RECORD"}})
}

/// Offline replay uses the identical production episode/plan selector and
/// economics/attribution functions. It cannot call a provider or submit a TX.
pub(super) fn replay(
    market_path: &Path,
    shadow_path: &Path,
    out: &Path,
    plan: &MeasurementPlan,
    start_ns: u64,
    max_seconds: u64,
) -> Result<(), String> {
    use std::io::BufRead;
    plan.validate()?;
    if max_seconds == 0 || max_seconds > 7200 {
        return Err("REPLAY_TIME_BUDGET".into());
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(max_seconds);
    let mut grouped: BTreeMap<String, Vec<Joined>> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut reader = BufReader::new(File::open(market_path).map_err(|_| "MARKET_INPUT")?);
    let mut count = 0usize;
    loop {
        if std::time::Instant::now() >= deadline {
            return Err("REPLAY_TIME_BUDGET".into());
        }
        let mut line = Vec::new();
        let len = (&mut reader)
            .take(16 * 1024 * 1024 + 1)
            .read_until(b'\n', &mut line)
            .map_err(|_| "MARKET_READ")?;
        if len == 0 {
            break;
        }
        if len > 16 * 1024 * 1024 || line.last() != Some(&b'\n') {
            return Err("REPLAY_ROW_BUDGET".into());
        }
        let mut market: TruthEvent = serde_json::from_slice(&line).map_err(|_| "MARKET_DECODE")?;
        market.discovery_source = "OFFLINE_FINALIZED_EVIDENCE".into();
        market.economic_provenance = "PERSISTED_PROTOCOL_EVENT_AND_ACTUAL_TX_META".into();
        let key = (market.signature.clone(), market.slot, market.event_index);
        if !seen.insert(key) {
            return Err("DUPLICATE_MARKET_EVENT".into());
        }
        count += 1;
        if count > 100_000 {
            return Err("REPLAY_MARKET_CAPACITY".into());
        }
        grouped
            .entry(market.target.clone())
            .or_default()
            .push(Joined {
                market,
                target_seen: false,
                target_seen_without_slot: false,
                health_liquidatable: None,
                candidates_created: None,
                plan_validated: None,
                built: None,
                identity_status: "NO_MATCH",
                our: None,
                diagnostic: None,
            });
    }
    for rows in grouped.values_mut() {
        rows.sort_by_key(|r| {
            (
                r.market.slot,
                r.market.transaction_index,
                r.market.event_index,
            )
        });
    }
    // One bounded streaming pass. No RPC repricing/re-simulation and no synthetic
    // evaluation when the original capture lost data. The evaluation stream is the binary
    // journal (`shadow_evaluations.p0j`); runs before PERSIST-01 keep their JSONL file.
    let binary = if shadow_path.extension().is_some_and(|e| e == "p0j") {
        shadow_path.to_path_buf()
    } else {
        journal::binary_path(shadow_path)
    };
    let legacy = binary != shadow_path && shadow_path.exists();
    if binary.exists() && legacy {
        return Err("AMBIGUOUS_SHADOW_SOURCE".into());
    }
    let available = binary.exists() || legacy;
    if count > 0 && !available {
        return Err("SHADOW_JOURNAL_MISSING".into());
    }
    if count > 0 && binary.exists() {
        let mut reader = journal::Reader::open(&binary).map_err(|_| "SHADOW_INPUT")?;
        while let Some(frame) = reader.next_frame().map_err(|_| "SHADOW_READ")? {
            if std::time::Instant::now() >= deadline {
                return Err("REPLAY_TIME_BUDGET".into());
            }
            match frame.kind {
                journal::kind::ECON_SHADOW_EVALUATION => {
                    let rec: OpportunityRecord = serde_json::from_value(
                        frame.legacy_value().map_err(|_| "SHADOW_DECODE")?["record"].take(),
                    )
                    .map_err(|_| "SHADOW_DECODE")?;
                    if let Some(rows) = grouped.get_mut(&rec.account) {
                        assign_record(rows, &rec);
                    }
                }
                journal::MANIFEST => {}
                _ => return Err("SHADOW_DECODE".into()),
            }
        }
    } else if count > 0 {
        let mut reader = BufReader::new(File::open(shadow_path).map_err(|_| "SHADOW_INPUT")?);
        loop {
            if std::time::Instant::now() >= deadline {
                return Err("REPLAY_TIME_BUDGET".into());
            }
            let mut line = Vec::new();
            let len = (&mut reader)
                .take(16 * 1024 * 1024 + 1)
                .read_until(b'\n', &mut line)
                .map_err(|_| "SHADOW_READ")?;
            if len == 0 {
                break;
            }
            if len > 16 * 1024 * 1024 || line.last() != Some(&b'\n') {
                return Err("REPLAY_ROW_BUDGET".into());
            }
            let rec: OpportunityRecord =
                serde_json::from_slice(&line).map_err(|_| "SHADOW_DECODE")?;
            if let Some(rows) = grouped.get_mut(&rec.account) {
                assign_record(rows, &rec);
            }
        }
    }
    fs::create_dir_all(out).map_err(|_| "REPLAY_OUTPUT")?;
    let mut rows: Vec<_> = grouped.into_values().flatten().collect();
    rows.sort_by_key(|r| {
        (
            r.market.slot,
            r.market.transaction_index,
            r.market.event_index,
        )
    });
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(out.join("canonical_market_shadow.jsonl"))
        .map_err(|_| "REPLAY_OUTPUT_EXISTS")?;
    let mut writer = BufWriter::new(file);
    for row in &rows {
        line(&mut writer, &joined_value(row))?;
    }
    writer.flush().map_err(|_| "REPLAY_WRITE")?;
    let bucket = |r: &Joined| {
        r.market
            .block_time
            .and_then(|t| u64::try_from((t as i128) * 1_000_000_000 - start_ns as i128).ok())
            .and_then(|n| plan.chain_bucket(n))
    };
    for i in 1..=SNAPSHOT_COUNT {
        let cumulative: Vec<_> = rows
            .iter()
            .filter(|r| bucket(r).is_some_and(|b| b < i))
            .collect();
        let delta: Vec<_> = rows.iter().filter(|r| bucket(r) == Some(i - 1)).collect();
        let unassigned = rows.iter().filter(|r| bucket(r).is_none()).count();
        let value = json!({"schema":"C0-E1/OfflineCanonicalEconomics/1","scope":"EXPLICIT_FINALIZED_EVENT_INPUT; verification coverage is separate",
            "start_unix_ns":start_ns,"cutoff_unix_ns":start_ns+plan.offset_ns(i),"plan":plan,
            "cumulative":summarize(&cumulative,plan.span_seconds(0,i),unassigned,true),
            "window_delta":summarize(&delta,plan.span_seconds(i-1,i),unassigned,true),
            "raw_shadow_available":available,"live_enabled":false,"master_pass":false});
        atomic_json(&out.join(format!("hour_{i:02}.json")), &value)?;
        if i == SNAPSHOT_COUNT {
            atomic_json(&out.join("economics_final.json"), &value)?;
        }
    }
    Ok(())
}
