//! Q5A: real TLS transport qualification of the frozen production
//! transport against the 48Club and BlockRazor builders.
//!
//! Sends only the safe probe, an `eth_sendBundle` without transactions.
//! Never a transaction, never value.
//!
//! Environment:
//!
//! - `Q5A_BLOCKRAZOR_AUTHORIZATION`: the BlockRazor authorization. A
//!   secret: never printed, never written; the evidence is checked for it
//!   before it is written. Without it, BlockRazor is measured up to the
//!   handshake and stays unqualified.
//! - `Q5A_PROVIDERS`: `club48`, `blockrazor` or both, comma-separated.
//!   Default both. A run with one provider cannot pass.
//! - `Q5A_EVIDENCE_PATH`: where to write the evidence record (R8, BSC-048:
//!   `darkbot.bsc.q5a.tls-transport-evidence.v2`, checked against itself
//!   once written). Default `q5a-tls-evidence-<UTC time>.dbrec`. An
//!   existing file is never overwritten.
//!
//! `u5_tls_qualify --check <file>` checks a written record against itself
//! (offline; exit 0 when it checks, 3 when not).
//!
//! Exit status: 0 PASS, 1 FAIL, 2 INCOMPLETE, 3 the harness could not run
//! (also: `BSC_ECON_SEND` not ENABLED in the native owner state, plan
//! 27.10.) or could not write its evidence.

use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::process::ExitCode;

use darkbot_bsc_execution::submission::tls::{
    AuthorizationToken, BLOCKRAZOR_DOCUMENTED_HOST, CLUB48_DOCUMENTED_HOST, HTTPS_PORT,
    TlsEndpoint, TlsSetup,
};
use darkbot_bsc_execution::submission::transport::MonotonicClock;
use darkbot_bsc_qualification::evidence::{
    ProviderRun, QUALIFICATION_WORKLOAD, check_record, contains_secret, evidence_record,
};
use darkbot_bsc_qualification::identity::{Identity, now_rfc3339};
use darkbot_bsc_qualification::measure::{Auth, Provider, QUALIFICATION_POLICY, run_provider};
use darkbot_bsc_qualification::verdict::{ProviderVerdict, Verdict, evaluate, overall};

const HARNESS_ERROR: u8 = 3;

fn main() -> ExitCode {
    match run() {
        Ok(Verdict::Pass) => ExitCode::from(0),
        Ok(Verdict::Fail) => ExitCode::from(1),
        Ok(Verdict::Incomplete) => ExitCode::from(2),
        Err(message) => {
            eprintln!("u5_tls_qualify: {message}");
            ExitCode::from(HARNESS_ERROR)
        }
    }
}

fn run() -> Result<Verdict, String> {
    // R8 review M3: `--check <file>` checks a written evidence record
    // against itself, offline (nothing sent).
    let args: Vec<String> = env::args().collect();
    if let [_, flag, path] = args.as_slice()
        && flag == "--check"
    {
        let bytes = std::fs::read(path)
            .map_err(|error| format!("cannot read {path}: {:?}", error.kind()))?;
        check_record(&bytes).map_err(|e| format!("{path} does not check: {e}"))?;
        // The authorization still in the environment is searched for too.
        if env::var("Q5A_BLOCKRAZOR_AUTHORIZATION")
            .is_ok_and(|secret| contains_secret(&bytes, &secret))
        {
            return Err(format!("{path} contains the authorization"));
        }
        println!("u5_tls_qualify: {path} checks against its samples");
        return Ok(Verdict::Pass);
    }
    let identity = Identity::of_this_build();
    let selected = env::var("Q5A_PROVIDERS").unwrap_or_else(|_| "club48,blockrazor".to_owned());
    let wanted: Vec<&str> = selected.split(',').map(str::trim).collect();
    if wanted
        .iter()
        .any(|name| *name != "club48" && *name != "blockrazor")
    {
        return Err("Q5A_PROVIDERS may only name club48 and blockrazor".to_owned());
    }
    // Plan 27.10.: every Mainnet bundle submission, this empty probe too,
    // needs BSC_ECON_SEND ENABLED in the native owner state of this boot.
    darkbot_bsc_execution::sources::require_enabled_here(&[
        darkbot_bsc_execution::sources::SwitchId::Send,
    ])
    .map_err(|e| format!("refused by the native switches: {e}; nothing sent"))?;
    let blockrazor_auth = match env::var("Q5A_BLOCKRAZOR_AUTHORIZATION") {
        Err(_) => Auth::Missing,
        Ok(value) => AuthorizationToken::new(value).map_or(Auth::Invalid, Auth::Token),
    };
    let secret = match &blockrazor_auth {
        Auth::Token(token) => Some(token.expose().to_owned()),
        _ => None,
    };
    let path = env::var("Q5A_EVIDENCE_PATH").unwrap_or_else(|_| {
        let stamp: String = now_rfc3339()
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .collect();
        format!("q5a-tls-evidence-{stamp}.dbrec")
    });
    // Reserve the evidence file first: an existing one is never replaced,
    // and a run that cannot write its evidence does not start.
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| format!("cannot create evidence file {path}: {:?}", error.kind()))?;

    let setup = TlsSetup::web_pki_roots().map_err(|error| error.to_string())?;
    let clock = MonotonicClock::start(1);
    let mut providers = Vec::new();
    if wanted.contains(&"club48") {
        providers.push(Provider {
            name: "club48",
            endpoint: endpoint(CLUB48_DOCUMENTED_HOST)?,
            auth: Auth::NotRequired,
        });
    }
    if wanted.contains(&"blockrazor") {
        providers.push(Provider {
            name: "blockrazor",
            endpoint: endpoint(BLOCKRAZOR_DOCUMENTED_HOST)?,
            auth: blockrazor_auth,
        });
    }

    let mut runs: Vec<ProviderRun> = Vec::new();
    for provider in &providers {
        println!(
            "u5_tls_qualify: measuring {} ({})",
            provider.name,
            provider.endpoint.host()
        );
        runs.push(run_provider(
            provider,
            &setup,
            QUALIFICATION_POLICY,
            QUALIFICATION_WORKLOAD,
            &clock,
        ));
    }
    let verdicts: Vec<ProviderVerdict> = runs.iter().map(|run| evaluate(run, &identity)).collect();
    let verdict = overall(&runs, &verdicts);

    let now_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        });
    let record = evidence_record(&identity, &runs, &verdicts, verdict, now_unix_ms)?;
    if secret
        .as_deref()
        .is_some_and(|secret| contains_secret(&record, secret))
    {
        // Never written; the reserved file stays empty.
        return Err("the evidence would contain the authorization; not written".to_owned());
    }
    file.write_all(&record)
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("cannot write evidence file {path}: {:?}", error.kind()))?;
    // R8: what was written is read back and checked against itself.
    let written = std::fs::read(&path)
        .map_err(|error| format!("cannot read evidence file {path}: {:?}", error.kind()))?;
    // Review LOW-2: the summary first, whatever the check says.
    for (run, verdict) in runs.iter().zip(&verdicts) {
        print_summary(run, verdict);
    }
    check_record(&written).map_err(|e| format!("evidence file {path} does not check: {e}"))?;
    println!(
        "u5_tls_qualify: TLS_TRANSPORT_QUALIFICATION = {}",
        verdict.name()
    );
    println!("u5_tls_qualify: evidence written to {path}");
    Ok(verdict)
}

fn endpoint(host: &str) -> Result<TlsEndpoint, String> {
    TlsEndpoint::new(host, HTTPS_PORT).map_err(|error| error.to_string())
}

fn print_summary(run: &ProviderRun, verdict: &ProviderVerdict) {
    let ns = |value: Option<u64>| value.map_or_else(|| "-".to_owned(), |v| v.to_string());
    let summary = &verdict.summary;
    println!(
        "{}: transport {} | api {} | handed off {} skipped {} unknown {} | \
         p50 {} p95 {} p99 {} max {} ns | misses {}",
        run.provider,
        verdict.transport.name(),
        verdict.api.name(),
        verdict.handed_off,
        verdict.skipped,
        verdict.unknown,
        ns(summary.p50),
        ns(summary.p95),
        ns(summary.p99),
        ns(summary.max),
        summary.at_or_above_limit,
    );
    for reason in &verdict.reasons {
        println!(
            "  {} ({})",
            reason.name(),
            if reason.fails() { "fail" } else { "incomplete" }
        );
    }
}
