//! C1.9b hook/path-early qualification on Testnet (`c19b_report`).
//!
//! For controlled, known launches it measures, per source and peer,
//!
//! ```text
//! known launch handoff -> packet decode -> T_ingress
//! ```
//!
//! from three inputs: the controlled launches (a `darkbot.c1.c19b-launches.v1`
//! record built with `c19b_report launches init|add`: per launch its hash
//! and handoff time), and the observation table and capture record written
//! by the `c19b_capture` example. Both sides must be timestamps of the same
//! clock (`CLOCK_MONOTONIC`, clock id 1, of the same host and boot);
//! otherwise the differences are meaningless and the tool refuses. It
//! refuses anything that is not a Testnet run, and an incomplete capture.
//! The result characterizes how late the selected hook/path is. It does not
//! prove Mainnet peer coverage, private-flow visibility or a Mainnet arrival
//! advantage, and it authorizes no value-bearing action.

use crate::c1_common::{self as common, Args, Stats, describe};
use darkbot_bsc_record::schema::c1_qualification::{
    c19b_capture as cap, c19b_launches as cl, hook_path as hp, table,
};
use darkbot_bsc_record::{Draft, Value, decode, observed_now};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The chains C1.9b may measure.
pub const TESTNET_CHAIN_IDS: [(u64, &str); 1] = [(97, "bsc-testnet")];
pub const SOURCES: [&str; 2] = ["eth-broadcast", "eth-pooled"];
pub const NON_CLAIMS: [&str; 6] = [
    "MAINNET_INGRESS_COVERAGE = NOT_PROVEN",
    "a Testnet hook/path result is not a Mainnet coverage or arrival-advantage result",
    "private-flow visibility is not measured",
    "T0_TO_T_BUY_LT_1MS = NOT_PROVEN",
    "PROVIDER_ORDERING/PLACEMENT = UNQUALIFIED",
    "LIVE = BLOCKED",
];

/// The controlled launches of one run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Launches {
    pub chain_id: u64,
    pub clock_id: u64,
    pub host_boot_id: String,
    pub handoff_boundary: String,
    /// (transaction hash, handoff ns), in launch order.
    pub launches: Vec<([u8; 32], u64)>,
}

fn network(chain_id: u64) -> Option<&'static str> {
    TESTNET_CHAIN_IDS
        .iter()
        .find(|(id, _)| *id == chain_id)
        .map(|(_, name)| *name)
}

impl Launches {
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let record = decode(&cl::SCHEMA, bytes).map_err(|e| e.0)?;
        let launches = record
            .nested_all(cl::LAUNCHES)
            .map_err(|e| e.0)?
            .iter()
            .map(|launch| {
                Ok((
                    launch.b256(cl::launch::TX_HASH).map_err(|e| e.0)?,
                    launch.u64(cl::launch::HANDOFF_NS).map_err(|e| e.0)?,
                ))
            })
            .collect::<Result<_, String>>()?;
        Ok(Self {
            chain_id: record.u64(cl::CHAIN_ID).map_err(|e| e.0)?,
            clock_id: record.u64(cl::CLOCK_ID).map_err(|e| e.0)?,
            host_boot_id: record.text(cl::HOST_BOOT_ID).map_err(|e| e.0)?.into(),
            handoff_boundary: record.text(cl::HANDOFF_BOUNDARY).map_err(|e| e.0)?.into(),
            launches,
        })
    }

    pub fn to_draft(&self, observed_unix_ms: u64) -> Draft {
        let mut d = Draft::new(&cl::SCHEMA);
        d.set(cl::CHAIN_ID, self.chain_id)
            .set(cl::CLOCK_ID, self.clock_id)
            .set(
                cl::PROVENANCE_TOOL,
                "darkbot-bsc-qualification c19b_report launches",
            )
            .set(cl::PROVENANCE_OBSERVED_UNIX_MS, observed_unix_ms)
            .set(cl::HOST_BOOT_ID, self.host_boot_id.as_str())
            .set(cl::HANDOFF_BOUNDARY, self.handoff_boundary.as_str());
        for (hash, handoff) in &self.launches {
            let mut launch = Draft::new(&cl::launch::SCHEMA);
            launch
                .set(cl::launch::TX_HASH, Value::B256(*hash))
                .set(cl::launch::HANDOFF_NS, *handoff);
            d.push(cl::LAUNCHES, launch.into_nested());
        }
        d
    }

    /// The run's identity: Testnet, one clock of one boot, a stated
    /// handoff boundary. Checked when the record is made and when it is read.
    pub fn check_header(&self) -> Result<(), String> {
        if network(self.chain_id).is_none() {
            return Err(format!(
                "chain_id {} is not a Testnet; C1.9b measures Testnet only",
                self.chain_id
            ));
        }
        if self.clock_id != 1 || self.host_boot_id.is_empty() {
            return Err(
                "launch handoffs need clock_id 1 (CLOCK_MONOTONIC) and the host boot id".into(),
            );
        }
        if self.handoff_boundary.is_empty() {
            return Err("the launcher must state what its handoff timestamp marks".into());
        }
        Ok(())
    }

    /// Everything [`Launches::check_header`] checks, plus at least one
    /// launch, unique hashes and a handoff time for each.
    pub fn check(&self) -> Result<(), String> {
        self.check_header()?;
        if self.launches.is_empty() {
            return Err("no controlled launches".into());
        }
        let mut seen = BTreeSet::new();
        for (hash, handoff) in &self.launches {
            if !seen.insert(*hash) {
                return Err(format!("launch hash {} is repeated", common::hex(hash)));
            }
            if *handoff == 0 {
                return Err(format!("launch {}: handoff_ns missing", common::hex(hash)));
            }
        }
        Ok(())
    }
}

/// One usable observation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    pub tx_hash: [u8; 32],
    pub peer: [u8; 32],
    pub source: &'static str,
    pub decoded: u64,
    pub ingress: u64,
}

/// Checks every observation: `CLOCK_MONOTONIC`, a peer-ingress source and
/// `0 < T_packet_decoded <= T_ingress`. Any violation refuses the run.
pub fn check_observations(rows: &[table::Observation]) -> Result<Vec<Seen>, String> {
    rows.iter()
        .map(|row| {
            if row.clock_id != 1 {
                return Err(format!(
                    "observation with clock_id {}; only CLOCK_MONOTONIC compares",
                    row.clock_id
                ));
            }
            let name = row.source_name().map_err(|e| e.0)?;
            let Some(source) = SOURCES.iter().find(|s| **s == name) else {
                return Err(format!(
                    "observation source {name:?} is not a peer ingress source"
                ));
            };
            if !(0 < row.t_packet_decoded_ns && row.t_packet_decoded_ns <= row.t_ingress_ns) {
                return Err(format!(
                    "observation of {}: T_packet_decoded after T_ingress",
                    common::hex(&row.tx_hash)
                ));
            }
            Ok(Seen {
                tx_hash: row.tx_hash,
                peer: row.peer,
                source,
                decoded: row.t_packet_decoded_ns,
                ingress: row.t_ingress_ns,
            })
        })
        .collect()
}

/// The capture that wrote the observations: same boot, complete, bound.
pub fn check_capture(
    capture: &[u8],
    observations: &[u8],
    launches: &Launches,
) -> Result<u64, String> {
    let record = decode(&cap::SCHEMA, capture).map_err(|e| format!("capture: {e}"))?;
    let get = |tag| record.u64(tag).map_err(|e| e.0);
    if record.text(cap::HOST_BOOT_ID).map_err(|e| e.0)? != launches.host_boot_id {
        return Err("observations and launch handoffs come from different hosts or boots".into());
    }
    if record.b256(cap::OBSERVATIONS_SHA256).map_err(|e| e.0)? != common::sha256(observations) {
        return Err("the capture record does not bind this observation table".into());
    }
    let (invalid, faults) = (get(cap::INVALID_RECORDS)?, get(cap::LANE_FAULTS)?);
    if invalid != 0 || faults != 0 {
        return Err(format!(
            "the capture is incomplete: {invalid} invalid records, {faults} lane faults"
        ));
    }
    get(cap::ROWS)
}

#[derive(Clone, Debug, Default)]
struct Group {
    decode: Vec<i64>,
    ingress: Vec<i64>,
    validation: Vec<i64>,
    behind_first: Vec<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchResult {
    pub tx_hash: [u8; 32],
    pub handoff_ns: u64,
    pub observed: bool,
    pub excluded: Option<&'static str>,
    pub first: Option<(&'static str, [u8; 32], i64)>,
    /// Ordered by `T_ingress`.
    pub observations: Vec<Seen>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourcePeer {
    pub source: &'static str,
    pub peer: [u8; 32],
    pub launches_observed: u64,
    pub launches_not_observed: i64,
    pub handoff_to_packet_decode: Stats,
    pub handoff_to_t_ingress: Stats,
    pub packet_decode_to_t_ingress: Stats,
    pub t_ingress_behind_first_source: Stats,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HookPath {
    pub network: &'static str,
    pub launches: u64,
    pub launches_observed: u64,
    pub missed: Vec<[u8; 32]>,
    pub before_handoff: Vec<[u8; 32]>,
    pub earliest_decode: Stats,
    pub earliest_ingress: Stats,
    /// Per source kind: T_ingress after the handoff, launches it saw first.
    pub by_kind: Vec<(&'static str, Stats, u64)>,
    pub per_source_and_peer: Vec<SourcePeer>,
    pub per_launch: Vec<LaunchResult>,
}

/// The hook/path measurement. `launches` must have passed [`Launches::check`].
pub fn analyze(launches: &Launches, observations: &[Seen]) -> HookPath {
    let mut by_hash: BTreeMap<[u8; 32], Vec<&Seen>> = BTreeMap::new();
    for row in observations {
        by_hash.entry(row.tx_hash).or_default().push(row);
    }
    let mut per_launch = Vec::new();
    let mut groups: BTreeMap<(&'static str, [u8; 32]), Group> = BTreeMap::new();
    let (mut earliest_ingress, mut earliest_decode) = (Vec::new(), Vec::new());
    let (mut missed, mut before_handoff) = (Vec::new(), Vec::new());
    let mut first_counts: BTreeMap<&'static str, u64> = BTreeMap::new();
    for (tx_hash, handoff) in &launches.launches {
        let mut rows: Vec<&Seen> = by_hash.get(tx_hash).cloned().unwrap_or_default();
        rows.sort_by_key(|row| row.ingress);
        let observations: Vec<Seen> = rows.iter().map(|row| (*row).clone()).collect();
        let Some(first) = rows.first() else {
            missed.push(*tx_hash);
            per_launch.push(LaunchResult {
                tx_hash: *tx_hash,
                handoff_ns: *handoff,
                observed: false,
                excluded: None,
                first: None,
                observations,
            });
            continue;
        };
        if first.decoded < *handoff {
            // The launch was on the wire before the stated handoff: the
            // handoff boundary or the clock is wrong for this launch.
            before_handoff.push(*tx_hash);
            per_launch.push(LaunchResult {
                tx_hash: *tx_hash,
                handoff_ns: *handoff,
                observed: true,
                excluded: Some("observed before the stated handoff"),
                first: None,
                observations,
            });
            continue;
        }
        *first_counts.entry(first.source).or_default() += 1;
        earliest_ingress.push(common::diff(first.ingress, *handoff));
        let decoded = rows
            .iter()
            .map(|row| row.decoded)
            .min()
            .unwrap_or(first.decoded);
        earliest_decode.push(common::diff(decoded, *handoff));
        for row in &rows {
            let group = groups.entry((row.source, row.peer)).or_default();
            group.decode.push(common::diff(row.decoded, *handoff));
            group.ingress.push(common::diff(row.ingress, *handoff));
            group
                .validation
                .push(common::diff(row.ingress, row.decoded));
            group
                .behind_first
                .push(common::diff(row.ingress, first.ingress));
        }
        per_launch.push(LaunchResult {
            tx_hash: *tx_hash,
            handoff_ns: *handoff,
            observed: true,
            excluded: None,
            first: Some((
                first.source,
                first.peer,
                common::diff(first.ingress, *handoff),
            )),
            observations,
        });
    }
    let total = launches.launches.len() as u64;
    let usable = total - missed.len() as u64 - before_handoff.len() as u64;
    let per_source_and_peer = groups
        .iter()
        .map(|((source, peer), group)| SourcePeer {
            source,
            peer: *peer,
            launches_observed: group.ingress.len() as u64,
            launches_not_observed: usable as i64 - group.ingress.len() as i64,
            handoff_to_packet_decode: describe(&group.decode),
            handoff_to_t_ingress: describe(&group.ingress),
            packet_decode_to_t_ingress: describe(&group.validation),
            t_ingress_behind_first_source: describe(&group.behind_first),
        })
        .collect();
    let by_kind = SOURCES
        .iter()
        .map(|source| {
            let values: Vec<i64> = groups
                .iter()
                .filter(|((kind, _), _)| kind == source)
                .flat_map(|(_, group)| group.ingress.iter().copied())
                .collect();
            let first = first_counts.get(source).copied().unwrap_or(0);
            (*source, describe(&values), first)
        })
        .collect();
    HookPath {
        network: network(launches.chain_id).unwrap_or("unknown"),
        launches: total,
        launches_observed: usable,
        missed,
        before_handoff,
        earliest_decode: describe(&earliest_decode),
        earliest_ingress: describe(&earliest_ingress),
        by_kind,
        per_source_and_peer,
        per_launch,
    }
}

/// SHA-256 of the three inputs, of the tool, and the capture row count.
pub struct Bound {
    pub launches: [u8; 32],
    pub observations: [u8; 32],
    pub capture: [u8; 32],
    pub analyzer: [u8; 32],
    pub capture_rows: u64,
}

impl HookPath {
    pub fn to_draft(&self, launches: &Launches, bound: &Bound, observed_unix_ms: u64) -> Draft {
        let mut d = Draft::new(&hp::SCHEMA);
        d.set(hp::RESULT, "MEASURED")
            .set(hp::NETWORK, self.network)
            .set(hp::PROVENANCE_TOOL, "darkbot-bsc-qualification c19b_report")
            .set(hp::PROVENANCE_OBSERVED_UNIX_MS, observed_unix_ms)
            .set(hp::CHAIN_ID, launches.chain_id)
            .set(hp::CLOCK_ID, 1u64)
            .set(hp::HOST_BOOT_ID, launches.host_boot_id.as_str())
            .set(hp::CLOCK_DOMAIN, "CLOCK_MONOTONIC, one host")
            .set(hp::HANDOFF_BOUNDARY, launches.handoff_boundary.as_str())
            .set(hp::LAUNCHES, self.launches)
            .set(hp::LAUNCHES_OBSERVED, self.launches_observed)
            .set(hp::LAUNCHES_MISSED, self.missed.len() as u64)
            .set(
                hp::HANDOFF_TO_EARLIEST_PACKET_DECODE,
                self.earliest_decode
                    .to_value("handoff_to_earliest_packet_decode"),
            )
            .set(
                hp::HANDOFF_TO_EARLIEST_T_INGRESS,
                self.earliest_ingress
                    .to_value("handoff_to_earliest_T_ingress"),
            )
            .set(hp::GATE, "NOT_DECIDED_BY_THIS_TOOL")
            .set(hp::LAUNCHES_SHA256, Value::B256(bound.launches))
            .set(hp::OBSERVATIONS_SHA256, Value::B256(bound.observations))
            .set(hp::CAPTURE_SHA256, Value::B256(bound.capture))
            .set(hp::ANALYZER_SHA256, Value::B256(bound.analyzer))
            .set(hp::CAPTURE_ROWS, bound.capture_rows);
        for hash in &self.missed {
            d.push(hp::MISSED_TX_HASHES, Value::B256(*hash));
        }
        for hash in &self.before_handoff {
            d.push(hp::EXCLUDED_OBSERVED_BEFORE_HANDOFF, Value::B256(*hash));
        }
        for (source, stats, first) in &self.by_kind {
            use hp::source_kind as k;
            let mut kind = Draft::new(&k::SCHEMA);
            kind.set(k::SOURCE, *source)
                .set(
                    k::HANDOFF_TO_T_INGRESS,
                    stats.to_value("handoff_to_T_ingress"),
                )
                .set(k::FIRST_FOR_LAUNCHES, *first);
            d.push(hp::BROADCAST_VS_POOLED, kind.into_nested());
        }
        for group in &self.per_source_and_peer {
            use hp::source_peer as g;
            let mut entry = Draft::new(&g::SCHEMA);
            entry
                .set(g::SOURCE, group.source)
                .set(g::PEER, Value::B256(group.peer))
                .set(g::LAUNCHES_OBSERVED, group.launches_observed)
                .set(g::LAUNCHES_NOT_OBSERVED, group.launches_not_observed)
                .set(
                    g::HANDOFF_TO_PACKET_DECODE,
                    group
                        .handoff_to_packet_decode
                        .to_value("handoff_to_packet_decode"),
                )
                .set(
                    g::HANDOFF_TO_T_INGRESS,
                    group.handoff_to_t_ingress.to_value("handoff_to_T_ingress"),
                )
                .set(
                    g::PACKET_DECODE_TO_T_INGRESS,
                    group
                        .packet_decode_to_t_ingress
                        .to_value("packet_decode_to_T_ingress"),
                )
                .set(
                    g::T_INGRESS_BEHIND_FIRST_SOURCE,
                    group
                        .t_ingress_behind_first_source
                        .to_value("T_ingress_behind_first_source"),
                );
            d.push(hp::PER_SOURCE_AND_PEER, entry.into_nested());
        }
        for launch in &self.per_launch {
            use hp::launch as l;
            let mut entry = Draft::new(&l::SCHEMA);
            entry
                .set(l::TX_HASH, Value::B256(launch.tx_hash))
                .set(l::HANDOFF_NS, launch.handoff_ns)
                .set(l::OBSERVED, launch.observed)
                .set_opt(l::EXCLUDED, launch.excluded);
            if let Some((source, peer, delay)) = launch.first {
                entry
                    .set(l::FIRST_SOURCE, source)
                    .set(l::FIRST_PEER, Value::B256(peer))
                    .set(l::HANDOFF_TO_FIRST_INGRESS_NS, delay);
            }
            for seen in &launch.observations {
                use hp::observation as o;
                let mut row = Draft::new(&o::SCHEMA);
                row.set(o::PEER, Value::B256(seen.peer))
                    .set(o::SOURCE, seen.source)
                    .set(o::T_PACKET_DECODED_NS, seen.decoded)
                    .set(o::T_INGRESS_NS, seen.ingress);
                entry.push(l::OBSERVATIONS, row.into_nested());
            }
            d.push(hp::PER_LAUNCH, entry.into_nested());
        }
        for claim in NON_CLAIMS {
            d.push(hp::NON_CLAIMS, claim);
        }
        d
    }

    /// The derived text view.
    pub fn summary(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "C1.9b hook/path ({}): launches={} observed={} missed={} excluded_before_handoff={}",
            self.network,
            self.launches,
            self.launches_observed,
            self.missed.len(),
            self.before_handoff.len()
        );
        let _ = writeln!(
            out,
            "  {}",
            self.earliest_decode
                .line("handoff_to_earliest_packet_decode")
        );
        let _ = writeln!(
            out,
            "  {}",
            self.earliest_ingress.line("handoff_to_earliest_T_ingress")
        );
        for (source, stats, first) in &self.by_kind {
            let _ = writeln!(
                out,
                "  {source}: first_for_launches={first} {}",
                stats.line("handoff_to_T_ingress")
            );
        }
        for group in &self.per_source_and_peer {
            let _ = writeln!(
                out,
                "  {} peer {}: observed={} not_observed={} {}",
                group.source,
                common::hex(&group.peer),
                group.launches_observed,
                group.launches_not_observed,
                group.handoff_to_t_ingress.line("handoff_to_T_ingress")
            );
        }
        let _ = writeln!(
            out,
            "C1_9B_MEASUREMENT_COMPLETE (Testnet hook/path only; MAINNET_INGRESS_COVERAGE = NOT_PROVEN)"
        );
        out
    }
}

const USAGE: &str = "usage: c19b_report --launches R --observations T --capture R --output R\n\
       c19b_report launches init --out R --chain-id N --clock-id 1 --host-boot-id ID --handoff-boundary TEXT\n\
       c19b_report launches add --launches R --launch TX_HASH HANDOFF_NS [--launch ...]";

fn launches_command(args: &Args) -> Result<String, String> {
    let sub = args.positional.get(1).map(String::as_str);
    if args.positional.len() != 2 {
        return Err("launches takes one subcommand: init or add".into());
    }
    let now = observed_now()?;
    match sub {
        Some("init") => {
            args.only(&[
                "out",
                "chain-id",
                "clock-id",
                "host-boot-id",
                "handoff-boundary",
            ])?;
            let number = |name: &str| -> Result<u64, String> {
                let value = args.one(name)?;
                value
                    .parse()
                    .map_err(|_| format!("--{name} {value:?} is not a number"))
            };
            let launches = Launches {
                chain_id: number("chain-id")?,
                clock_id: number("clock-id")?,
                host_boot_id: args.one("host-boot-id")?.into(),
                handoff_boundary: args.one("handoff-boundary")?.into(),
                launches: Vec::new(),
            };
            launches.check_header()?;
            let out = PathBuf::from(args.one("out")?);
            let bytes = launches.to_draft(now).encode().map_err(|e| e.0)?;
            common::write_new(&out, &bytes)?;
            Ok(format!("launches={} (no launch yet)", out.display()))
        }
        Some("add") => {
            args.only(&["launches", "launch"])?;
            let path = PathBuf::from(args.one("launches")?);
            let mut launches = Launches::decode(&common::read(&path)?)?;
            launches.check_header()?;
            let values = args.many("launch");
            if values.is_empty() || !values.len().is_multiple_of(2) {
                return Err("--launch takes TX_HASH HANDOFF_NS".into());
            }
            for pair in values.chunks_exact(2) {
                let hash = common::parse_b256(pair[0])
                    .ok_or_else(|| format!("launch hash {:?} is malformed", pair[0]))?;
                let handoff: u64 = pair[1]
                    .parse()
                    .map_err(|_| format!("handoff_ns {:?} is not a number", pair[1]))?;
                launches.launches.push((hash, handoff));
            }
            launches.check()?;
            let bytes = launches.to_draft(now).encode().map_err(|e| e.0)?;
            common::write_replace(&path, &bytes)?;
            Ok(format!(
                "launches={} count={}",
                path.display(),
                launches.launches.len()
            ))
        }
        _ => Err("launches takes one subcommand: init or add".into()),
    }
}

/// Builds the report from the three input files.
pub fn report(
    launches: &Path,
    observations: &Path,
    capture: &Path,
) -> Result<(Launches, HookPath, Bound), String> {
    let launch_bytes = common::read(launches)?;
    let launch_data = Launches::decode(&launch_bytes).map_err(|e| format!("launches: {e}"))?;
    launch_data.check()?;
    let table_bytes = common::read(observations)?;
    let capture_bytes = common::read(capture)?;
    let capture_rows = check_capture(&capture_bytes, &table_bytes, &launch_data)?;
    let rows = table::read_all(
        &table::C19B_OBSERVATION,
        &table_bytes,
        table::Observation::read,
    )
    .map_err(|e| e.0)?;
    let seen = check_observations(&rows)?;
    let result = analyze(&launch_data, &seen);
    let bound = Bound {
        launches: common::sha256(&launch_bytes),
        observations: common::sha256(&table_bytes),
        capture: common::sha256(&capture_bytes),
        analyzer: common::own_sha256()?,
        capture_rows,
    };
    Ok((launch_data, result, bound))
}

/// The command line. Exit 0: done; 2: refused (no report); 64: usage.
pub fn main_with(args: Args) -> u8 {
    if args.positional.first().map(String::as_str) == Some("launches") {
        return match launches_command(&args) {
            Ok(done) => {
                println!("{done}");
                0
            }
            Err(problem) => {
                eprintln!("C19B_REFUSED {problem}");
                2
            }
        };
    }
    let parsed = (|| {
        args.only(&["launches", "observations", "capture", "output"])?;
        if !args.positional.is_empty() {
            return Err("unexpected positional argument".to_string());
        }
        Ok((
            PathBuf::from(args.one("launches")?),
            PathBuf::from(args.one("observations")?),
            PathBuf::from(args.one("capture")?),
            PathBuf::from(args.one("output")?),
        ))
    })();
    let (launches, observations, capture, output) = match parsed {
        Ok(parsed) => parsed,
        Err(problem) => {
            eprintln!("c19b_report: {problem}\n{USAGE}");
            return 64;
        }
    };
    let written = report(&launches, &observations, &capture).and_then(|(data, result, bound)| {
        let bytes = result
            .to_draft(&data, &bound, observed_now()?)
            .encode()
            .map_err(|e| e.0)?;
        common::write_new(&output, &bytes)?;
        Ok((result, bytes))
    });
    match written {
        Ok((result, bytes)) => {
            print!("{}", result.summary());
            println!(
                "report={} sha256={}",
                output.display(),
                common::hex(&common::sha256(&bytes))
            );
            0
        }
        Err(problem) => {
            eprintln!("C19B_REFUSED {problem}");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ported from bsc/tools/test_c1_qualification_tools.py (HookPathTest).

    fn launches() -> Launches {
        Launches {
            chain_id: 97,
            clock_id: 1,
            host_boot_id: "boot-1".into(),
            handoff_boundary: "return of the launcher's raw transaction send".into(),
            launches: vec![
                ([0xaa; 32], 1_000),
                ([0xbb; 32], 5_000),
                ([0xcc; 32], 9_000),
            ],
        }
    }

    fn row(
        hash: u8,
        peer: u8,
        source: &str,
        decoded: u64,
        ingress: u64,
        clock: u32,
    ) -> table::Observation {
        let mut name = [0u8; 16];
        name[..source.len()].copy_from_slice(source.as_bytes());
        table::Observation {
            tx_hash: [hash; 32],
            peer: [peer; 32],
            source: name,
            t_packet_decoded_ns: decoded,
            t_ingress_ns: ingress,
            lane_epoch: 1,
            lane_sequence: 0,
            clock_id: clock,
            lane_id: 16,
        }
    }

    #[test]
    fn sources_are_compared_and_misses_are_counted() {
        let rows = [
            row(0xaa, 1, "eth-pooled", 1_300, 1_400, 1),
            row(0xaa, 2, "eth-broadcast", 1_100, 1_150, 1),
            row(0xbb, 1, "eth-pooled", 5_200, 5_260, 1),
        ];
        let data = launches();
        data.check().unwrap();
        let report = analyze(&data, &check_observations(&rows).unwrap());
        assert_eq!(
            (
                report.launches,
                report.launches_observed,
                report.missed.len()
            ),
            (3, 2, 1)
        );
        assert_eq!(report.missed, vec![[0xcc; 32]]);
        assert_eq!(report.earliest_ingress.max_ns, Some(260));
        let first = |source: &str| report.by_kind.iter().find(|k| k.0 == source).unwrap().2;
        assert_eq!(first("eth-broadcast"), 1);
        assert_eq!(first("eth-pooled"), 1);
        let pooled = report
            .per_source_and_peer
            .iter()
            .find(|s| s.peer == [1; 32])
            .unwrap();
        assert_eq!(
            (pooled.launches_observed, pooled.launches_not_observed),
            (2, 0)
        );
        assert_eq!(pooled.t_ingress_behind_first_source.max_ns, Some(250));
        // The record carries it all, with the non-claims.
        let bound = Bound {
            launches: [0; 32],
            observations: [0; 32],
            capture: [0; 32],
            analyzer: [0; 32],
            capture_rows: 3,
        };
        let bytes = report.to_draft(&data, &bound, 1).encode().unwrap();
        let record = decode(&hp::SCHEMA, &bytes).unwrap();
        assert!(
            record
                .texts(hp::NON_CLAIMS)
                .unwrap()
                .contains(&"MAINNET_INGRESS_COVERAGE = NOT_PROVEN")
        );
        assert_eq!(record.nested_all(hp::PER_LAUNCH).unwrap().len(), 3);
    }

    #[test]
    fn mainnet_and_incomparable_clocks_are_refused() {
        let bad = [
            Launches {
                chain_id: 56,
                ..launches()
            },
            Launches {
                clock_id: 2,
                ..launches()
            },
            Launches {
                host_boot_id: String::new(),
                ..launches()
            },
            Launches {
                handoff_boundary: String::new(),
                ..launches()
            },
            Launches {
                launches: Vec::new(),
                ..launches()
            },
            Launches {
                launches: vec![([1; 32], 5), ([1; 32], 6)],
                ..launches()
            },
            Launches {
                launches: vec![([1; 32], 0)],
                ..launches()
            },
        ];
        for data in bad {
            assert!(data.check().is_err(), "{data:?}");
        }
        for observation in [
            row(0xaa, 1, "eth-pooled", 10, 20, 2),
            row(0xaa, 1, "txpool", 10, 20, 1),
            row(0xaa, 1, "eth-pooled", 30, 20, 1),
            row(0xaa, 1, "eth-pooled", 0, 20, 1),
        ] {
            assert!(check_observations(&[observation]).is_err());
        }
        // Another boot, an unbound table or an incomplete capture is refused.
        let table_bytes = table::encode(&table::C19B_OBSERVATION, &[]).unwrap();
        let capture = |boot: &str, sha: [u8; 32], invalid: u64| {
            let mut d = Draft::new(&cap::SCHEMA);
            d.set(cap::HOST_BOOT_ID, boot)
                .set(cap::GENERATION, 1u64)
                .set(cap::PROVENANCE_TOOL, "test")
                .set(cap::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
                .set(cap::PEER_LANES, 4u64)
                .set(cap::ROWS, 0u64)
                .set(cap::INVALID_RECORDS, invalid)
                .set(cap::LANE_FAULTS, 0u64)
                .set(cap::OBSERVATIONS_SHA256, Value::B256(sha));
            d.encode().unwrap()
        };
        let bound = common::sha256(&table_bytes);
        assert_eq!(
            check_capture(&capture("boot-1", bound, 0), &table_bytes, &launches()),
            Ok(0)
        );
        let other = check_capture(&capture("boot-2", bound, 0), &table_bytes, &launches());
        assert!(other.unwrap_err().contains("different hosts or boots"));
        let unbound = check_capture(&capture("boot-1", [9; 32], 0), &table_bytes, &launches());
        assert!(unbound.unwrap_err().contains("does not bind"));
        let incomplete = check_capture(&capture("boot-1", bound, 1), &table_bytes, &launches());
        assert!(incomplete.unwrap_err().contains("incomplete"));
    }

    #[test]
    fn a_launch_seen_before_its_handoff_is_excluded_not_measured() {
        let rows = [row(0xaa, 1, "eth-pooled", 900, 950, 1)];
        let report = analyze(&launches(), &check_observations(&rows).unwrap());
        assert_eq!(report.before_handoff, vec![[0xaa; 32]]);
        assert_eq!(report.earliest_ingress.samples, 0);
    }

    #[test]
    fn the_launches_record_is_built_by_the_command_line() {
        let dir = std::env::temp_dir().join(format!("c19b-launches-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("launches.dbrec");
        let run = |tokens: &[&str]| main_with(Args::parse(tokens.iter().map(|t| t.to_string())));
        let p = path.to_str().unwrap();
        let mainnet = [
            "launches",
            "init",
            "--out",
            p,
            "--chain-id",
            "56",
            "--clock-id",
            "1",
        ];
        let tail = [
            "--host-boot-id",
            "boot-1",
            "--handoff-boundary",
            "send return",
        ];
        assert_eq!(run(&[&mainnet[..], &tail[..]].concat()), 2);
        assert!(!path.exists());
        let testnet = [
            "launches",
            "init",
            "--out",
            p,
            "--chain-id",
            "97",
            "--clock-id",
            "1",
        ];
        assert_eq!(run(&[&testnet[..], &tail[..]].concat()), 0);
        let hash = format!("0x{}", "ab".repeat(32));
        assert_eq!(
            run(&[
                "launches",
                "add",
                "--launches",
                p,
                "--launch",
                &hash,
                "1000"
            ]),
            0
        );
        // The same hash again is refused and leaves the record unchanged.
        assert_eq!(
            run(&[
                "launches",
                "add",
                "--launches",
                p,
                "--launch",
                &hash,
                "2000"
            ]),
            2
        );
        let data = Launches::decode(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(data.launches, vec![([0xab; 32], 1000)]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
