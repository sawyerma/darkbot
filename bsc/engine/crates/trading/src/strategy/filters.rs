//! Typed entry filters (Strategy C, the original `parse_filters` /
//! `evaluate_filters`): `feature:min:max` with an empty bound unbounded,
//! each bound an exact decimal at the feature's own places; a NULL feature
//! fails its filter; a true gap flag makes the decision `DATA_INCOMPLETE`.

use super::features::{GAP_FLAG, NAMES, Value, places};
use super::fixed::parse;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rule {
    pub feature: usize,
    pub min: Option<i128>,
    pub max: Option<i128>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    /// The first filter that failed.
    Reject(usize),
    DataIncomplete,
}

/// `feature:min:max,feature:min:max` (an empty list is no filter); every
/// deviation is refused with its reason.
pub fn parse_rules(text: &str, ratio_places: u32) -> Result<Vec<Rule>, String> {
    let mut rules: Vec<Rule> = Vec::new();
    for item in text.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let parts: Vec<&str> = item.split(':').collect();
        let [name, min, max] = parts.as_slice() else {
            return Err(format!("filter {item:?} is not feature:min:max"));
        };
        let feature = NAMES
            .iter()
            .position(|n| n == name)
            .filter(|i| *i != GAP_FLAG)
            .ok_or_else(|| format!("filter {item:?}: {name} is not a filterable feature"))?;
        if rules.iter().any(|r| r.feature == feature) {
            return Err(format!("filter {item:?}: {name} twice"));
        }
        let p = places(feature, ratio_places);
        let bound = |b: &str| -> Result<Option<i128>, String> {
            if b.is_empty() {
                return Ok(None);
            }
            parse(b, p).map(Some).ok_or_else(|| {
                format!("filter {item:?}: {b:?} is no decimal with at most {p} places")
            })
        };
        let (min, max) = (bound(min)?, bound(max)?);
        if let (Some(a), Some(b)) = (min, max)
            && a > b
        {
            return Err(format!("filter {item:?}: min > max"));
        }
        rules.push(Rule { feature, min, max });
    }
    Ok(rules)
}

pub fn evaluate(rules: &[Rule], features: &[Value; 16]) -> Verdict {
    let failed = rules.iter().find(|r| match features[r.feature] {
        Value::Num(v) => r.min.is_some_and(|m| v < m) || r.max.is_some_and(|m| v > m),
        Value::Null | Value::Flag(_) => true,
    });
    if features[GAP_FLAG] == Value::Flag(true) {
        return Verdict::DataIncomplete;
    }
    match failed {
        Some(r) => Verdict::Reject(r.feature),
        None => Verdict::Pass,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_parse_exactly_and_refuse_deviations() {
        let r = parse_rules(
            "buy_count:3:,unique_buyer_ratio:0.5:1,buy_volume_bnb::0.25",
            4,
        )
        .unwrap();
        assert_eq!(
            r,
            [
                Rule {
                    feature: 0,
                    min: Some(3),
                    max: None
                },
                Rule {
                    feature: 6,
                    min: Some(5_000),
                    max: Some(10_000)
                },
                Rule {
                    feature: 2,
                    min: None,
                    max: Some(250_000_000_000_000_000)
                },
            ]
        );
        assert!(parse_rules("", 4).unwrap().is_empty());
        for bad in [
            "buy_count:3",
            "nope:1:2",
            "provider_gap_flag::",
            "buy_count:1:2,buy_count:3:4",
            "unique_buyer_ratio:0.12345:",
            "buy_count:5:1",
            "buy_count:1.5:",
        ] {
            assert!(parse_rules(bad, 4).is_err(), "{bad}");
        }
    }

    #[test]
    fn null_fails_bounds_are_inclusive_and_a_gap_is_incomplete() {
        let rules = parse_rules("buy_count:3:5,price_change::", 4).unwrap();
        let mut f = [Value::Num(0); 16];
        f[GAP_FLAG] = Value::Flag(false);
        f[0] = Value::Num(3);
        assert_eq!(evaluate(&rules, &f), Verdict::Pass);
        f[0] = Value::Num(6);
        assert_eq!(evaluate(&rules, &f), Verdict::Reject(0));
        f[0] = Value::Num(5);
        f[11] = Value::Null;
        assert_eq!(evaluate(&rules, &f), Verdict::Reject(11));
        f[GAP_FLAG] = Value::Flag(true);
        assert_eq!(evaluate(&rules, &f), Verdict::DataIncomplete);
    }
}
