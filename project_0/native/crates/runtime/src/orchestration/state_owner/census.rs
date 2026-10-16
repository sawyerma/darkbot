//! [C0-02] Opt-in one-shot market census at the first READY (`P0_C0_CENSUS=1`). The cost model
//! is explicit configuration; the result is one typed text log line, no JSON.
use super::*;

/// Cost model read once at setup: (realizable_bps, fixed_cost_usd_micro, min_profit_usd_micro).
pub(super) type CensusConfig = (u64, u64, i128);

pub(super) fn config() -> Result<Option<CensusConfig>, Box<dyn std::error::Error>> {
    if !config::parsed_or("P0_C0_CENSUS", false)? {
        return Ok(None);
    }
    Ok(Some((
        config::require_parsed("P0_C0_REALIZABLE_BPS")?,
        config::require_parsed("P0_C0_FIXED_COST_USD_MICRO")?,
        config::require_parsed("P0_C0_MIN_PROFIT_USD_MICRO")?,
    )))
}

#[cfg(feature = "crate_health")]
impl ShadowOwner {
    pub(super) fn run_c0_census(
        &mut self,
        (realizable_bps, fixed_cost_usd_micro, min_profit_usd_micro): CensusConfig,
    ) {
        use darkbot_project0::census::{classify, Census, CensusModel};
        use darkbot_project0::health::HealthEngine;
        let started = std::time::Instant::now();
        let now_s = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let slot = self.slot_watch.get();
        let result = {
            let view = StateView(&self.st);
            let sizer = darkbot_project0::sizing::ProtocolSizer {
                state: &view,
                program: self.program,
                group: self.group,
                headroom_bps: darkbot_project0::sizing::DEFAULT_HEADROOM_BPS,
            };
            sizer.fee_snapshot().map(|fee| {
                let model = CensusModel {
                    fee_state_max_bps: fee.liquidation_max_bps,
                    realizable_bps,
                    fixed_cost_usd_micro,
                    min_profit_usd_micro,
                };
                let engine = darkbot_project0::health::crate_impl::CrateHealth {
                    state: &view,
                    group: self.group,
                };
                let mut census = Census::default();
                for (pk, r) in self.st.iter_owned_by(&self.program) {
                    if !liability_account_in_group(&r.data, &self.group) {
                        continue;
                    }
                    let health = engine.health(pk, now_s as i64, slot);
                    let decay = engine.premium_decay_per_second(pk).unwrap_or(0);
                    census.add(
                        classify(&health, decay, now_s, &model),
                        health.as_ref().ok(),
                        now_s,
                    );
                }
                (model, census)
            })
        };
        match result {
            Ok((m, census)) => owner_log!(self,
                "c0_census: slot={} unix={} clock=LOCAL_WALL fee_bps={} realizable_bps={} fixed_cost_usd_micro={} min_profit_usd_micro={} elapsed_ms={} {}",
                slot, now_s, m.fee_state_max_bps, m.realizable_bps, m.fixed_cost_usd_micro,
                m.min_profit_usd_micro, started.elapsed().as_millis(), census
            ),
            Err(e) => owner_log!(self, "c0_census: FAILED fee_state={e}"),
        }
    }
}
