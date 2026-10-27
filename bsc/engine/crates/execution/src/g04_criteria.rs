//! The G0.4/V10-8 decision (order V2 §8/§15.5, plan "GELTENDE REGELN"
//! no. 5): the census of a campaign against the versioned criteria file
//! (`bsc/config/g04_criteria.env`). Every threshold is a key of that file
//! with its bounds. Criteria the user has not confirmed
//! (`BSC_G04_CRITERIA_STATE=PROPOSED`) evaluate nothing: the verdict is
//! `CRITERIA_UNCONFIRMED` and no venue is rated. A confirmed file rates
//! each venue by its actionable share of the tradable launches and the
//! median lead of the actionable ones; the campaign must be finished
//! (its days elapsed since its first journal started, no `RUNNING`
//! journal, none started after its end), covered for the required share,
//! free of an `INVALID` journal and of a suspect clock, else
//! `NOT_EVALUABLE`.

use crate::g04_census::{CRITERIA_UNCONFIRMED, CRITERIA_VENUES, Census, percentile};
use darkbot_bsc_record::schema::bsc_census::verdict as gv;
use darkbot_bsc_record::{Draft, Value};
use std::collections::HashMap;

pub const NOT_EVALUABLE: &str = "NOT_EVALUABLE";
pub const CONTINUE: &str = "CONTINUE";
pub const ADAPT: &str = "ADAPT";
pub const STOP: &str = "STOP";

const DAY_MS: u64 = 86_400_000;

/// The criteria file, parsed strictly: every key known and set once, every
/// bound in range, the STOP share below the CONTINUE share.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Criteria {
    pub confirmed: bool,
    pub campaign_days: u64,
    pub min_covered_ppm: u64,
    /// In [`CRITERIA_VENUES`] order.
    pub min_launches: [u64; 3],
    pub continue_actionable_ppm: u64,
    pub continue_lead_p50_us: u64,
    pub stop_actionable_ppm: u64,
    /// SHA-256 of the file's bytes.
    pub sha256: [u8; 32],
}

impl Criteria {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let text = std::str::from_utf8(bytes).map_err(|_| "the criteria file is not UTF-8")?;
        let mut values: HashMap<&str, &str> = HashMap::new();
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (k, v) = line
                .split_once('=')
                .ok_or_else(|| format!("not KEY=VALUE: {line}"))?;
            if values.insert(k.trim(), v.trim()).is_some() {
                return Err(format!("{} is set twice", k.trim()));
            }
        }
        let confirmed = match values.remove("BSC_G04_CRITERIA_STATE") {
            Some("PROPOSED") => false,
            Some("CONFIRMED") => true,
            Some(other) => {
                return Err(format!(
                    "BSC_G04_CRITERIA_STATE={other} is neither PROPOSED nor CONFIRMED"
                ));
            }
            None => return Err("BSC_G04_CRITERIA_STATE is required".into()),
        };
        let mut take = |name: &str, min: u64, max: u64| -> Result<u64, String> {
            let v = values
                .remove(name)
                .ok_or_else(|| format!("{name} is required"))?;
            let v: u64 = v
                .parse()
                .map_err(|_| format!("{name} is not an unsigned integer"))?;
            if !(min..=max).contains(&v) {
                return Err(format!("{name}={v} is outside {min}..={max}"));
            }
            Ok(v)
        };
        let criteria = Criteria {
            confirmed,
            // Plan rule 5: at most seven calendar days.
            campaign_days: take("BSC_G04_CAMPAIGN_DAYS", 1, 7)?,
            min_covered_ppm: take("BSC_G04_MIN_COVERED_PPM", 1, 1_000_000)?,
            min_launches: [
                take("BSC_G04_MIN_LAUNCHES_FLAP", 1, 10_000_000)?,
                take("BSC_G04_MIN_LAUNCHES_CLASSIC", 1, 10_000_000)?,
                take("BSC_G04_MIN_LAUNCHES_OPENFOUR", 1, 10_000_000)?,
            ],
            continue_actionable_ppm: take("BSC_G04_CONTINUE_ACTIONABLE_PPM", 1, 1_000_000)?,
            continue_lead_p50_us: take("BSC_G04_CONTINUE_LEAD_P50_US", 0, 60_000_000)?,
            stop_actionable_ppm: take("BSC_G04_STOP_ACTIONABLE_PPM", 0, 1_000_000)?,
            sha256: darkbot_bsc_record::sha256(bytes),
        };
        if let Some(k) = values.keys().next() {
            return Err(format!("{k} is not a G0.4 criteria key"));
        }
        if criteria.stop_actionable_ppm >= criteria.continue_actionable_ppm {
            return Err(format!(
                "BSC_G04_STOP_ACTIONABLE_PPM {} is not below BSC_G04_CONTINUE_ACTIONABLE_PPM {}",
                criteria.stop_actionable_ppm, criteria.continue_actionable_ppm
            ));
        }
        Ok(criteria)
    }
}

/// One venue's rating.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VenueVerdict {
    pub venue: &'static str,
    pub connected: u64,
    pub scope: u64,
    pub actionable: u64,
    pub share_ppm: Option<u64>,
    pub lead_p50_us: Option<u64>,
    pub min_launches: u64,
    pub verdict: &'static str,
    pub reasons: Vec<&'static str>,
}

/// The campaign's verdict.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Evaluation {
    pub criteria_sha256: [u8; 32],
    pub confirmed: bool,
    pub verdict: &'static str,
    pub reasons: Vec<&'static str>,
    pub campaign_ms: u64,
    pub covered_ms: u64,
    pub covered_ppm: Option<u64>,
    pub venues: Vec<VenueVerdict>,
}

pub fn evaluate(criteria: &Criteria, census: &Census, now_unix_ms: u64) -> Evaluation {
    let campaign_ms = criteria.campaign_days * DAY_MS;
    let mut e = Evaluation {
        criteria_sha256: criteria.sha256,
        confirmed: criteria.confirmed,
        verdict: CRITERIA_UNCONFIRMED,
        reasons: Vec::new(),
        campaign_ms,
        covered_ms: census.covered_ms,
        covered_ppm: None,
        venues: Vec::new(),
    };
    if !criteria.confirmed {
        e.reasons.push("CRITERIA_PROPOSED");
        return e;
    }
    let covered_ppm =
        u64::try_from(u128::from(census.covered_ms) * 1_000_000 / u128::from(campaign_ms))
            .unwrap_or(u64::MAX);
    e.covered_ppm = Some(covered_ppm);
    let end = census
        .windows
        .iter()
        .map(|w| w.started_unix_ms)
        .min()
        .map(|start| start.saturating_add(campaign_ms));
    if census.windows.iter().any(|w| w.state == "RUNNING")
        || end.is_none_or(|end| now_unix_ms < end)
    {
        e.reasons.push("CAMPAIGN_RUNNING");
    }
    if end.is_some_and(|end| census.windows.iter().any(|w| w.started_unix_ms >= end)) {
        e.reasons.push("DATA_BEYOND_CAMPAIGN");
    }
    if census.windows.iter().any(|w| w.state == "INVALID") {
        e.reasons.push("INVALID_JOURNAL");
    }
    if census.clock_suspect {
        e.reasons.push("CLOCK_SUSPECT");
    }
    if covered_ppm < criteria.min_covered_ppm {
        e.reasons.push("COVERAGE_BELOW_MINIMUM");
    }
    let valid = e.reasons.is_empty();
    for (i, v) in census.criteria_inputs.iter().enumerate() {
        let mut row = VenueVerdict {
            venue: CRITERIA_VENUES[i],
            connected: v.connected,
            scope: v.scope,
            actionable: v.actionable,
            share_ppm: None,
            lead_p50_us: None,
            min_launches: criteria.min_launches[i],
            verdict: NOT_EVALUABLE,
            reasons: Vec::new(),
        };
        if !valid {
            row.reasons.push("CAMPAIGN_NOT_EVALUABLE");
        } else if v.scope < criteria.min_launches[i] {
            row.reasons.push("SAMPLE_BELOW_MINIMUM");
        } else {
            let share = v.actionable * 1_000_000 / v.scope;
            let lead = percentile(&v.leads_us, 500);
            row.share_ppm = Some(share);
            row.lead_p50_us = lead;
            row.verdict = if share < criteria.stop_actionable_ppm {
                row.reasons.push("ACTIONABLE_SHARE_BELOW_STOP");
                STOP
            } else {
                if share < criteria.continue_actionable_ppm {
                    row.reasons.push("ACTIONABLE_SHARE_BELOW_CONTINUE");
                }
                if lead.is_none_or(|l| l < criteria.continue_lead_p50_us) {
                    row.reasons.push("LEAD_BELOW_CONTINUE");
                }
                if row.reasons.is_empty() {
                    CONTINUE
                } else {
                    ADAPT
                }
            };
        }
        e.venues.push(row);
    }
    let rated = |verdict: &str| e.venues.iter().any(|v| v.verdict == verdict);
    e.verdict = if !valid {
        NOT_EVALUABLE
    } else if rated(CONTINUE) {
        CONTINUE
    } else if rated(ADAPT) {
        ADAPT
    } else if rated(STOP) {
        STOP
    } else {
        e.reasons.push("NO_EVALUABLE_VENUE");
        NOT_EVALUABLE
    };
    e
}

impl Evaluation {
    pub fn encode(&self, observed_unix_ms: u64) -> Result<Vec<u8>, String> {
        let mut d = Draft::new(&gv::SCHEMA);
        d.set(gv::CRITERIA_SHA256, Value::B256(self.criteria_sha256))
            .set(
                gv::CRITERIA_STATE,
                if self.confirmed {
                    "CONFIRMED"
                } else {
                    "PROPOSED"
                },
            )
            .set(gv::PROVENANCE_TOOL, "bsc_runtime g04-verdict")
            .set(gv::PROVENANCE_OBSERVED_UNIX_MS, observed_unix_ms)
            .set(gv::VERDICT, self.verdict)
            .set(gv::CAMPAIGN_MS, self.campaign_ms)
            .set(gv::COVERED_MS, self.covered_ms);
        for r in &self.reasons {
            d.push(gv::REASONS, *r);
        }
        if let Some(p) = self.covered_ppm {
            d.set(gv::COVERED_PPM, p);
        }
        for v in &self.venues {
            use gv::venue as k;
            let mut row = Draft::new(&k::SCHEMA);
            row.set(k::VENUE, v.venue)
                .set(k::CONNECTED, v.connected)
                .set(k::SCOPE, v.scope)
                .set(k::ACTIONABLE, v.actionable)
                .set(k::MIN_LAUNCHES, v.min_launches)
                .set(k::VERDICT, v.verdict);
            if let Some(s) = v.share_ppm {
                row.set(k::SHARE_PPM, s);
            }
            if let Some(l) = v.lead_p50_us {
                row.set(k::LEAD_P50_US, l);
            }
            for r in &v.reasons {
                row.push(k::REASONS, *r);
            }
            d.push(gv::VENUES, row.into_nested());
        }
        d.encode().map_err(|e| format!("g04-verdict: {e:?}"))
    }

    pub fn summary(&self) -> String {
        let mut s = format!(
            "G0.4 verdict {} (criteria {}, sha256 {}); covered {} of {} ms; reasons [{}]\n",
            self.verdict,
            if self.confirmed {
                "CONFIRMED"
            } else {
                "PROPOSED"
            },
            darkbot_bsc_record::hex(&self.criteria_sha256),
            self.covered_ms,
            self.campaign_ms,
            self.reasons.join(",")
        );
        for v in &self.venues {
            s.push_str(&format!(
                "  {} {}: connected {} scope {} actionable {} share_ppm {:?} lead_p50_us {:?} (min {}) [{}]\n",
                v.venue,
                v.verdict,
                v.connected,
                v.scope,
                v.actionable,
                v.share_ppm,
                v.lead_p50_us,
                v.min_launches,
                v.reasons.join(",")
            ));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::g04_census::{VenueInputs, Window};

    const PROPOSED: &str = "BSC_G04_CRITERIA_STATE=PROPOSED\nBSC_G04_CAMPAIGN_DAYS=7\n\
        BSC_G04_MIN_COVERED_PPM=800000\nBSC_G04_MIN_LAUNCHES_FLAP=1000\n\
        BSC_G04_MIN_LAUNCHES_CLASSIC=1000\nBSC_G04_MIN_LAUNCHES_OPENFOUR=100\n\
        BSC_G04_CONTINUE_ACTIONABLE_PPM=300000\nBSC_G04_CONTINUE_LEAD_P50_US=100000\n\
        BSC_G04_STOP_ACTIONABLE_PPM=50000\n";

    /// After the seven days of a campaign whose first journal started at 0.
    const NOW: u64 = 7 * DAY_MS;

    fn rate(c: &Criteria, census: &Census) -> Evaluation {
        evaluate(c, census, NOW)
    }

    fn confirmed() -> Criteria {
        Criteria::parse(PROPOSED.replace("=PROPOSED", "=CONFIRMED").as_bytes()).unwrap()
    }

    fn inputs(scope: u64, actionable: u64, lead_us: u64) -> VenueInputs {
        VenueInputs {
            connected: scope,
            scope,
            actionable,
            leads_us: vec![lead_us; actionable as usize],
        }
    }

    fn census(covered_ms: u64, venues: [VenueInputs; 3]) -> Census {
        Census {
            windows: vec![Window {
                state: "COMPLETE",
                ..Window::default()
            }],
            covered_ms,
            criteria_inputs: venues,
            ..Census::default()
        }
    }

    #[test]
    fn the_committed_proposal_parses_and_evaluates_nothing() {
        let bytes = include_bytes!("../../../../config/g04_criteria.env");
        let c = Criteria::parse(bytes).unwrap();
        assert!(!c.confirmed);
        assert_eq!(c.sha256, darkbot_bsc_record::sha256(bytes));
        let full = census(
            7 * DAY_MS,
            [
                inputs(5000, 4000, 900_000),
                inputs(5000, 0, 0),
                inputs(500, 400, 900_000),
            ],
        );
        let e = rate(&c, &full);
        assert_eq!(e.verdict, CRITERIA_UNCONFIRMED);
        assert_eq!(e.reasons, ["CRITERIA_PROPOSED"]);
        assert!(e.venues.is_empty() && e.covered_ppm.is_none());
    }

    #[test]
    fn a_criteria_file_is_strict() {
        for (bad, why) in [
            (PROPOSED.replace("=PROPOSED", "=MAYBE"), "neither"),
            (
                PROPOSED.replace("BSC_G04_CAMPAIGN_DAYS=7", "BSC_G04_CAMPAIGN_DAYS=8"),
                "outside",
            ),
            (
                PROPOSED.replace("BSC_G04_MIN_LAUNCHES_OPENFOUR=100\n", ""),
                "required",
            ),
            (
                format!("{PROPOSED}BSC_G04_EXTRA=1\n"),
                "not a G0.4 criteria key",
            ),
            (format!("{PROPOSED}BSC_G04_CAMPAIGN_DAYS=7\n"), "set twice"),
            (
                PROPOSED.replace("STOP_ACTIONABLE_PPM=50000", "STOP_ACTIONABLE_PPM=300000"),
                "not below",
            ),
        ] {
            let e = Criteria::parse(bad.as_bytes()).unwrap_err();
            assert!(e.contains(why), "{e}");
        }
    }

    #[test]
    fn each_venue_is_rated_by_its_share_and_lead() {
        let e = rate(
            &confirmed(),
            &census(
                7 * DAY_MS,
                [
                    // 40 % actionable, median lead 150 ms: CONTINUE.
                    inputs(2000, 800, 150_000),
                    // Classic: a token is only a candidate, never actionable.
                    inputs(3000, 0, 0),
                    // 20 %: between STOP and CONTINUE.
                    inputs(200, 40, 150_000),
                ],
            ),
        );
        assert_eq!(e.verdict, CONTINUE);
        assert_eq!(e.covered_ppm, Some(1_000_000));
        let rows: Vec<_> = e
            .venues
            .iter()
            .map(|v| (v.venue, v.verdict, v.share_ppm, v.reasons.clone()))
            .collect();
        assert_eq!(
            rows,
            [
                ("FLAP", CONTINUE, Some(400_000), vec![]),
                (
                    "FOUR_CLASSIC",
                    STOP,
                    Some(0),
                    vec!["ACTIONABLE_SHARE_BELOW_STOP"]
                ),
                (
                    "OPENFOUR",
                    ADAPT,
                    Some(200_000),
                    vec!["ACTIONABLE_SHARE_BELOW_CONTINUE"]
                ),
            ]
        );
        // A share above CONTINUE with a short lead adapts.
        let short = rate(
            &confirmed(),
            &census(
                7 * DAY_MS,
                [
                    inputs(2000, 800, 99_999),
                    inputs(10, 0, 0),
                    inputs(10, 0, 0),
                ],
            ),
        );
        assert_eq!(short.verdict, ADAPT);
        assert_eq!(short.venues[0].reasons, ["LEAD_BELOW_CONTINUE"]);
        assert_eq!(short.venues[1].reasons, ["SAMPLE_BELOW_MINIMUM"]);
        // Only STOP and too small samples: STOP.
        let stop = rate(
            &confirmed(),
            &census(
                7 * DAY_MS,
                [
                    inputs(2000, 99, 150_000),
                    inputs(10, 0, 0),
                    inputs(10, 0, 0),
                ],
            ),
        );
        assert_eq!(stop.verdict, STOP);
    }

    #[test]
    fn an_incomplete_campaign_is_not_evaluable() {
        let good = [
            inputs(2000, 800, 150_000),
            inputs(2000, 0, 0),
            inputs(200, 0, 0),
        ];
        // Below the covered share.
        let short = rate(&confirmed(), &census(5 * DAY_MS, good.clone()));
        assert_eq!(short.verdict, NOT_EVALUABLE);
        assert_eq!(short.reasons, ["COVERAGE_BELOW_MINIMUM"]);
        assert!(short.venues.iter().all(|v| v.verdict == NOT_EVALUABLE));
        // Still running, an invalid journal, a suspect clock.
        let mut c = census(7 * DAY_MS, good.clone());
        c.windows.push(Window {
            state: "RUNNING",
            ..Window::default()
        });
        c.windows.push(Window {
            state: "INVALID",
            ..Window::default()
        });
        c.clock_suspect = true;
        let e = rate(&confirmed(), &c);
        assert_eq!(e.verdict, NOT_EVALUABLE);
        assert_eq!(
            e.reasons,
            ["CAMPAIGN_RUNNING", "INVALID_JOURNAL", "CLOCK_SUSPECT"]
        );
        // Evaluated before the seven days elapsed, or with a journal
        // started after them.
        let early = evaluate(&confirmed(), &census(7 * DAY_MS, good.clone()), NOW - 1);
        assert_eq!(early.reasons, ["CAMPAIGN_RUNNING"]);
        let mut late = census(7 * DAY_MS, good.clone());
        late.windows.push(Window {
            state: "COMPLETE",
            started_unix_ms: NOW,
            ..Window::default()
        });
        assert_eq!(rate(&confirmed(), &late).reasons, ["DATA_BEYOND_CAMPAIGN"]);
        // No venue reaches its minimum sample.
        let small = rate(
            &confirmed(),
            &census(
                7 * DAY_MS,
                [inputs(10, 5, 150_000), inputs(10, 0, 0), inputs(10, 0, 0)],
            ),
        );
        assert_eq!(small.verdict, NOT_EVALUABLE);
        assert_eq!(small.reasons, ["NO_EVALUABLE_VENUE"]);
    }

    #[test]
    fn the_verdict_record_round_trips() {
        let e = rate(
            &confirmed(),
            &census(
                7 * DAY_MS,
                [
                    inputs(2000, 800, 150_000),
                    inputs(3000, 0, 0),
                    inputs(10, 0, 0),
                ],
            ),
        );
        let r = darkbot_bsc_record::decode(&gv::SCHEMA, &e.encode(1).unwrap()).unwrap();
        assert_eq!(r.text(gv::VERDICT).unwrap(), CONTINUE);
        assert_eq!(r.text(gv::CRITERIA_STATE).unwrap(), "CONFIRMED");
        let unconfirmed = rate(
            &Criteria::parse(PROPOSED.as_bytes()).unwrap(),
            &census(0, Default::default()),
        );
        let r = darkbot_bsc_record::decode(&gv::SCHEMA, &unconfirmed.encode(1).unwrap()).unwrap();
        assert_eq!(r.text(gv::VERDICT).unwrap(), CRITERIA_UNCONFIRMED);
        assert!(r.nested_all(gv::VENUES).unwrap().is_empty());
    }
}
