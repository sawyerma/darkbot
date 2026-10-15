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
//! - `Q5A_EVIDENCE_PATH`: where to write the evidence. Default
//!   `q5a-tls-evidence-<UTC time>.json`. An existing file is never
//!   overwritten.
//!
//! Exit status: 0 PASS, 1 FAIL, 2 INCOMPLETE, 3 the harness could not run
//! or could not write its evidence.

use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::process::ExitCode;

use darkbot_qualify::evidence::{
    ProviderRun, QUALIFICATION_WORKLOAD, contains_secret, evidence_json,
};
use darkbot_qualify::identity::{Identity, now_rfc3339};
use darkbot_qualify::measure::{Auth, Provider, QUALIFICATION_POLICY, run_provider};
use darkbot_qualify::verdict::{ProviderVerdict, Verdict, evaluate, overall};
use darkbot_runner::inclusion::tls::{
    AuthorizationToken, BLOCKRAZOR_DOCUMENTED_HOST, CLUB48_DOCUMENTED_HOST, HTTPS_PORT,
    TlsEndpoint, TlsSetup,
};
use darkbot_runner::inclusion::transport::MonotonicClock;

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
    let identity = Identity::of_this_build();
    let selected = env::var("Q5A_PROVIDERS").unwrap_or_else(|_| "club48,blockrazor".to_owned());
    let wanted: Vec<&str> = selected.split(',').map(str::trim).collect();
    if wanted
        .iter()
        .any(|name| *name != "club48" && *name != "blockrazor")
    {
        return Err("Q5A_PROVIDERS may only name club48 and blockrazor".to_owned());
    }
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
        format!("q5a-tls-evidence-{stamp}.json")
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

    let text = evidence_json(&identity, &runs, &verdicts, verdict).render();
    if secret
        .as_deref()
        .is_some_and(|secret| contains_secret(&text, secret))
    {
        // Never written; the reserved file stays empty.
        return Err("the evidence would contain the authorization; not written".to_owned());
    }
    file.write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("cannot write evidence file {path}: {:?}", error.kind()))?;

    for (run, verdict) in runs.iter().zip(&verdicts) {
        print_summary(run, verdict);
    }
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
