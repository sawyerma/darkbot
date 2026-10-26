//! The one owner of first sightings across the signal sources (plan §5a.3,
//! "first verified actionable wins"): every source feeds its observations
//! into one [`IngressCore`]; per transaction hash it keeps the first sighting
//! of any kind and the first *verified full* sighting, so a hash-only event
//! never outranks an actionable full one, and duplicates from later sources
//! never create a second candidate. A full transaction is a Flap launch
//! candidate when its signed bytes target the C1 Flap scope (the same
//! `decode_tx_target` + `C14_FLAP_SCOPE` the geth Fast Lane uses).
//!
//! Memory is bounded: the oldest hashes are forgotten first (`capacity`).
//! Times are the caller's monotonic readings; this module reads no clock.

use crate::sources::SourceId;
use alloy_primitives::Address;
use darkbot_bsc_geth_ipc::ingress::{C14_FLAP_SCOPE, decode_tx_target};
use darkbot_bsc_trading::venues::flap::recognize::{Recognized, recognize};
use darkbot_bsc_trading::venues::fourmeme::deployment::FourDeployment;
use darkbot_bsc_trading::venues::fourmeme::recognize as four;
use std::collections::{HashMap, VecDeque};

/// What one source delivered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Observation<'a> {
    /// A full transaction whose integrity and signer the source adapter
    /// verified; `raw` are its canonical signed bytes.
    Full {
        hash: [u8; 32],
        raw: &'a [u8],
    },
    HashOnly {
        hash: [u8; 32],
    },
    /// A full object that failed verification: counted, never used.
    Rejected,
    Connected,
    Disconnected {
        rate_limited: bool,
    },
    /// The bounded hand-off queue was full: one event lost, counted.
    Dropped,
}

/// A Flap launch sighting decided by this observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Launch {
    /// The first verified full sighting of this launch: its source wins.
    First {
        hash: [u8; 32],
        source: SourceId,
        at_ns: u64,
    },
    /// A later verified full sighting from another source, `lag_ns` behind.
    Later {
        hash: [u8; 32],
        source: SourceId,
        lag_ns: u64,
    },
}

/// Counters of one source, as the runtime status reports them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceStats {
    pub connected: bool,
    pub connects: u64,
    pub disconnects: u64,
    pub rate_limited: u64,
    pub full: u64,
    pub hash_only: u64,
    pub rejected: u64,
    pub dropped: u64,
    /// Sightings of a hash another source delivered first.
    pub duplicates: u64,
    /// Hashes this source delivered first (any kind).
    pub first_any: u64,
    /// Hashes this source delivered first as verified full transactions.
    pub first_full: u64,
    pub launch_sightings: u64,
    pub launch_first: u64,
    /// Monotonic time of the last observation, if any.
    pub last_event_ns: Option<u64>,
}

/// One first sighting: its source, its monotonic time and the runtime's
/// wall-clock estimate of that moment (µs since the Unix epoch; `None`
/// when the wall clock could not be read — never 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirstSeen {
    pub source: SourceId,
    pub at_ns: u64,
    pub wall_us: Option<u64>,
}

/// What the sources delivered for one hash while it is remembered: the
/// first sighting of any kind and the first verified full one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sighting {
    pub first_any: FirstSeen,
    pub first_full: Option<FirstSeen>,
}

#[derive(Clone, Copy)]
struct Seen {
    first_any: (usize, u64, Option<u64>),
    first_full: Option<(usize, u64, Option<u64>)>,
    /// Bit per source index that delivered this hash.
    sources: u8,
    launch: bool,
}

pub struct IngressCore {
    capacity: usize,
    order: VecDeque<[u8; 32]>,
    seen: HashMap<[u8; 32], Seen>,
    stats: [SourceStats; 5],
    launches: u64,
    /// The largest launch calldata the venue decoder accepts.
    max_launch_calldata: usize,
}

fn index(source: SourceId) -> usize {
    SourceId::ALL.iter().position(|s| *s == source).unwrap_or(0)
}

/// Whether canonical signed bytes are a Flap launch of the C1 scope: the
/// frozen HOT0 prefilter (Portal, `newTokenV6`) and then the production
/// Flap decoder's verified decode (Phase D). Calldata the decoder refuses
/// is no launch.
pub fn is_flap_launch(raw: &[u8], max_calldata: usize) -> bool {
    let Ok(target) = decode_tx_target(raw) else {
        return false;
    };
    if !C14_FLAP_SCOPE.matches(&target) {
        return false;
    }
    // The prefilter's one address is the Portal; the signed bytes bound
    // the calldata.
    let portal = Address::from(C14_FLAP_SCOPE.addresses[0]);
    matches!(
        recognize(
            portal,
            target.to.map(Address::from),
            target.data,
            max_calldata
        ),
        Recognized::LaunchV6(_)
    )
}

/// The pre-execution token of canonical signed bytes that the production
/// Four.meme decoder (Phase E) recognizes as a launch (Classic or OpenFour
/// `createToken` of `deployment`); `None` for anything else, including
/// calldata the decoder refuses or above `max_calldata`.
pub fn four_launch_token(
    deployment: &FourDeployment,
    raw: &[u8],
    max_calldata: usize,
) -> Option<four::PendingToken> {
    let target = decode_tx_target(raw).ok()?;
    let recognized = four::recognize(
        deployment,
        target.to.map(Address::from),
        target.data,
        max_calldata,
    );
    matches!(
        recognized,
        four::Recognized::ClassicLaunch(_) | four::Recognized::OpenFourLaunch(_)
    )
    .then(|| four::pending_token(deployment, &recognized))
}

impl IngressCore {
    /// `capacity` hashes are remembered (at least one); launch calldata
    /// above `max_launch_calldata` is refused as oversized.
    pub fn new(capacity: usize, max_launch_calldata: usize) -> Self {
        let capacity = capacity.max(1);
        IngressCore {
            capacity,
            order: VecDeque::with_capacity(capacity),
            seen: HashMap::with_capacity(capacity),
            stats: [SourceStats::default(); 5],
            launches: 0,
            max_launch_calldata,
        }
    }

    pub fn stats(&self, source: SourceId) -> SourceStats {
        self.stats[index(source)]
    }

    /// Distinct Flap launches seen as verified full transactions.
    pub fn launches(&self) -> u64 {
        self.launches
    }

    /// The sighting of `hash`, if it is still remembered (bounded window).
    pub fn sighting(&self, hash: &[u8; 32]) -> Option<Sighting> {
        let seen = self.seen.get(hash)?;
        let at = |(i, at_ns, wall_us): (usize, u64, Option<u64>)| FirstSeen {
            source: SourceId::ALL[i],
            at_ns,
            wall_us,
        };
        Some(Sighting {
            first_any: at(seen.first_any),
            first_full: seen.first_full.map(at),
        })
    }

    fn remember(&mut self, hash: [u8; 32], seen: Seen) {
        if self.order.len() == self.capacity
            && let Some(old) = self.order.pop_front()
        {
            self.seen.remove(&old);
        }
        self.order.push_back(hash);
        self.seen.insert(hash, seen);
    }

    /// One observation of `source` at monotonic `now_ns`, `wall_us` being
    /// the wall-clock estimate of the same moment.
    pub fn observe(
        &mut self,
        source: SourceId,
        observation: Observation<'_>,
        now_ns: u64,
        wall_us: Option<u64>,
    ) -> Option<Launch> {
        let i = index(source);
        let bit = 1u8 << i;
        self.stats[i].last_event_ns = Some(now_ns);
        match observation {
            Observation::Connected => {
                self.stats[i].connected = true;
                self.stats[i].connects += 1;
                None
            }
            Observation::Disconnected { rate_limited } => {
                self.stats[i].connected = false;
                self.stats[i].disconnects += 1;
                self.stats[i].rate_limited += u64::from(rate_limited);
                None
            }
            Observation::Rejected => {
                self.stats[i].rejected += 1;
                None
            }
            Observation::Dropped => {
                self.stats[i].dropped += 1;
                None
            }
            Observation::HashOnly { hash } => {
                self.stats[i].hash_only += 1;
                match self.seen.get_mut(&hash) {
                    Some(seen) => {
                        seen.sources |= bit;
                        self.stats[i].duplicates += 1;
                    }
                    None => {
                        self.stats[i].first_any += 1;
                        self.remember(
                            hash,
                            Seen {
                                first_any: (i, now_ns, wall_us),
                                first_full: None,
                                sources: bit,
                                launch: false,
                            },
                        );
                    }
                }
                None
            }
            Observation::Full { hash, raw } => {
                self.stats[i].full += 1;
                let launch = is_flap_launch(raw, self.max_launch_calldata);
                if launch {
                    self.stats[i].launch_sightings += 1;
                }
                let Some(seen) = self.seen.get_mut(&hash) else {
                    self.stats[i].first_any += 1;
                    self.stats[i].first_full += 1;
                    if launch {
                        self.stats[i].launch_first += 1;
                        self.launches += 1;
                    }
                    self.remember(
                        hash,
                        Seen {
                            first_any: (i, now_ns, wall_us),
                            first_full: Some((i, now_ns, wall_us)),
                            sources: bit,
                            launch,
                        },
                    );
                    return launch.then_some(Launch::First {
                        hash,
                        source,
                        at_ns: now_ns,
                    });
                };
                seen.sources |= bit;
                match seen.first_full {
                    // The first actionable sighting, even after a hash-only one.
                    None => {
                        seen.first_full = Some((i, now_ns, wall_us));
                        seen.launch = launch;
                        self.stats[i].first_full += 1;
                        if seen.first_any.0 != i {
                            self.stats[i].duplicates += 1;
                        }
                        if launch {
                            self.stats[i].launch_first += 1;
                            self.launches += 1;
                        }
                        launch.then_some(Launch::First {
                            hash,
                            source,
                            at_ns: now_ns,
                        })
                    }
                    Some((first, at, _)) => {
                        self.stats[i].duplicates += 1;
                        (launch && first != i).then(|| Launch::Later {
                            hash,
                            source,
                            lag_ns: now_ns.saturating_sub(at),
                        })
                    }
                }
            }
        }
    }
}

/// The derived health of one source (plan §5a.2), separate from its mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Health {
    /// The source is not effective (OFF or BLOCKED): nothing runs.
    Off,
    /// No adapter exists for this source in this build (e.g. no local geth).
    Unqualified,
    Disconnected,
    RateLimited,
    /// Connected but silent for longer than the quiet bound.
    Degraded,
    Healthy,
}

impl Health {
    pub fn name(self) -> &'static str {
        match self {
            Health::Off => "OFF",
            Health::Unqualified => "UNQUALIFIED",
            Health::Disconnected => "DISCONNECTED",
            Health::RateLimited => "RATE_LIMITED",
            Health::Degraded => "DEGRADED",
            Health::Healthy => "HEALTHY",
        }
    }
}

/// Health of a running source from its counters at `now_ns`.
/// `rate_limited_recent`: the last disconnect was a rate limit.
pub fn health(
    stats: &SourceStats,
    rate_limited_recent: bool,
    now_ns: u64,
    quiet_ns: u64,
) -> Health {
    if !stats.connected {
        return if rate_limited_recent {
            Health::RateLimited
        } else {
            Health::Disconnected
        };
    }
    match stats.last_event_ns {
        Some(at) if now_ns.saturating_sub(at) <= quiet_ns => Health::Healthy,
        _ => Health::Degraded,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pending `newTokenV6` launch of the G0.4 coverage capture (real,
    /// canonical calldata).
    const LAUNCH_CALLDATA: &str =
        include_str!("../../trading/fixtures/flap/launch_v6_native_split_tax_pending.hex");

    fn launch_calldata() -> Vec<u8> {
        alloy_primitives::hex::decode(LAUNCH_CALLDATA.trim()).unwrap()
    }

    /// A canonical signed legacy transaction to `to`: the real launch
    /// calldata for the launch selector (`newTokenV6`), else the selector
    /// and one zero word.
    fn legacy(to: [u8; 20], selector: [u8; 4]) -> Vec<u8> {
        let data = if selector == LAUNCH {
            launch_calldata()
        } else {
            let mut data = selector.to_vec();
            data.extend_from_slice(&[0u8; 32]);
            data
        };
        signed(to, &data)
    }

    fn signed(to: [u8; 20], data: &[u8]) -> Vec<u8> {
        use darkbot_bsc_tx_encoding::rlp::{encode_bytes, encode_list, encode_u64};
        encode_list(&[
            encode_u64(1),
            encode_u64(1_000_000_000),
            encode_u64(21_000),
            encode_bytes(&to),
            encode_u64(0),
            encode_bytes(data),
            encode_u64(147),
            encode_bytes(&[1u8; 32]),
            encode_bytes(&[2u8; 32]),
        ])
    }

    const PORTAL: [u8; 20] = [
        0xe2, 0xce, 0x6a, 0xb8, 0x08, 0x74, 0xfa, 0x9f, 0xa2, 0xaa, 0xe6, 0x5d, 0x27, 0x7d, 0xd6,
        0xb8, 0xe6, 0x5c, 0x9d, 0xe0,
    ];
    const LAUNCH: [u8; 4] = [0x8c, 0xb5, 0x77, 0x2c];
    const MAX_CALLDATA: usize = 16_384;

    #[test]
    fn a_flap_launch_is_the_c1_scope_and_nothing_else() {
        assert!(is_flap_launch(&legacy(PORTAL, LAUNCH), MAX_CALLDATA));
        assert!(!is_flap_launch(&legacy([7; 20], LAUNCH), MAX_CALLDATA));
        assert!(!is_flap_launch(&legacy(PORTAL, [1, 2, 3, 4]), MAX_CALLDATA));
        assert!(!is_flap_launch(b"not a transaction", MAX_CALLDATA));
        // Phase D: the launch selector alone is no launch; the production
        // decoder must accept the calldata (before: counted as a launch).
        let mut short = LAUNCH.to_vec();
        short.extend_from_slice(&[0u8; 32]);
        assert!(!is_flap_launch(&signed(PORTAL, &short), MAX_CALLDATA));
        let mut truncated = launch_calldata();
        truncated.truncate(truncated.len() - 32);
        assert!(!is_flap_launch(&signed(PORTAL, &truncated), MAX_CALLDATA));
        // Phase D review LOW-1: above the configured bound a canonical
        // launch is refused as oversized (before: the raw length was the
        // bound, so nothing was ever oversized).
        // The frozen prefilter's Portal is the decoder's deployment Portal.
        assert_eq!(
            Address::from(C14_FLAP_SCOPE.addresses[0]),
            darkbot_bsc_trading::venues::flap::deployment::bsc_mainnet_v5_27_0().portal
        );
        let launch = launch_calldata();
        assert!(is_flap_launch(&legacy(PORTAL, LAUNCH), launch.len()));
        assert!(!is_flap_launch(&legacy(PORTAL, LAUNCH), launch.len() - 1));
    }

    /// Plan §5a.3: the first verified full sighting wins, even when another
    /// source had the hash earlier; later full sightings are lags, duplicates
    /// never create a second launch.
    #[test]
    fn the_first_verified_full_sighting_wins_and_duplicates_never_count_twice() {
        let mut core = IngressCore::new(16, MAX_CALLDATA);
        let raw = legacy(PORTAL, LAUNCH);
        let h = [9u8; 32];
        assert_eq!(
            core.observe(
                SourceId::Drpc,
                Observation::HashOnly { hash: h },
                100,
                Some(1_000)
            ),
            None
        );
        assert_eq!(
            core.observe(
                SourceId::PublicNode,
                Observation::Full { hash: h, raw: &raw },
                150,
                Some(1_050)
            ),
            Some(Launch::First {
                hash: h,
                source: SourceId::PublicNode,
                at_ns: 150
            })
        );
        assert_eq!(
            core.observe(
                SourceId::BlockReq,
                Observation::Full { hash: h, raw: &raw },
                190,
                None
            ),
            Some(Launch::Later {
                hash: h,
                source: SourceId::BlockReq,
                lag_ns: 40
            })
        );
        // The winner again: a duplicate, no second launch.
        assert_eq!(
            core.observe(
                SourceId::PublicNode,
                Observation::Full { hash: h, raw: &raw },
                200,
                None
            ),
            None
        );
        assert_eq!(core.launches(), 1);
        let (d, p, b) = (
            core.stats(SourceId::Drpc),
            core.stats(SourceId::PublicNode),
            core.stats(SourceId::BlockReq),
        );
        assert_eq!((d.first_any, d.first_full, d.hash_only), (1, 0, 1));
        assert_eq!((p.first_full, p.launch_first, p.duplicates), (1, 1, 2));
        assert_eq!(
            (b.launch_sightings, b.launch_first, b.duplicates),
            (1, 0, 1)
        );
        // The join reads both firsts with their source and time.
        assert_eq!(
            core.sighting(&h),
            Some(Sighting {
                first_any: FirstSeen {
                    source: SourceId::Drpc,
                    at_ns: 100,
                    wall_us: Some(1_000)
                },
                first_full: Some(FirstSeen {
                    source: SourceId::PublicNode,
                    at_ns: 150,
                    wall_us: Some(1_050)
                }),
            })
        );
        assert_eq!(core.sighting(&[1; 32]), None);
    }

    #[test]
    fn memory_is_bounded_and_counters_cover_every_event() {
        let mut core = IngressCore::new(2, MAX_CALLDATA);
        for n in 0..5u8 {
            core.observe(
                SourceId::Drpc,
                Observation::HashOnly { hash: [n; 32] },
                u64::from(n),
                None,
            );
        }
        assert_eq!(core.seen.len(), 2);
        // A forgotten hash counts as first again (bounded memory, explicit).
        core.observe(
            SourceId::Drpc,
            Observation::HashOnly { hash: [0; 32] },
            9,
            None,
        );
        assert_eq!(core.stats(SourceId::Drpc).first_any, 6);
        core.observe(SourceId::Drpc, Observation::Rejected, 10, None);
        core.observe(SourceId::Drpc, Observation::Dropped, 11, None);
        core.observe(SourceId::Drpc, Observation::Connected, 12, None);
        core.observe(
            SourceId::Drpc,
            Observation::Disconnected { rate_limited: true },
            13,
            None,
        );
        let s = core.stats(SourceId::Drpc);
        assert_eq!(
            (
                s.rejected,
                s.dropped,
                s.connects,
                s.disconnects,
                s.rate_limited
            ),
            (1, 1, 1, 1, 1)
        );
        assert!(!s.connected);
    }

    #[test]
    fn health_is_derived_from_the_counters_never_assumed() {
        let mut s = SourceStats::default();
        assert_eq!(health(&s, false, 10, 5), Health::Disconnected);
        assert_eq!(health(&s, true, 10, 5), Health::RateLimited);
        s.connected = true;
        assert_eq!(
            health(&s, false, 10, 5),
            Health::Degraded,
            "connected, nothing seen"
        );
        s.last_event_ns = Some(8);
        assert_eq!(health(&s, false, 10, 5), Health::Healthy);
        assert_eq!(health(&s, false, 20, 5), Health::Degraded);
    }
}
