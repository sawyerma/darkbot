//! REV.1.0 R11: the Messplan §3b A and B cases on the real `bsc_runtime`
//! process (the harness build: its TLS clients also trust the test CA).
//!
//! Loopback mock WSS servers with the test `localhost` certificate stand in
//! for the providers; the canonical endpoint is a closed loopback port, so
//! C2 never has a snapshot. The owner is driven through the CLI as the
//! terminal drives it; what counts is the process's own status, journal and
//! census. No real host, key, wallet or network is used, nothing is signed
//! for a real account and nothing is sent.

mod tls_support;

use alloy_primitives::{Address, B256, U256, hex, keccak256};
use darkbot_bsc_execution::g04_census::census;
use darkbot_bsc_execution::runtime::{RuntimeConfig, journal_max_payload, stop_file};
use darkbot_bsc_record::Record;
use darkbot_bsc_record::schema::bsc_runtime::runtime_status as rs;
use darkbot_bsc_trading::signer::Signer;
use darkbot_bsc_trading::transactions::legacy::{LegacyTx, decode_signed};
use darkbot_bsc_trading::venues::flap::abi::{LaunchParamsV6, encode_new_token_v6};
use darkbot_bsc_trading::venues::flap::deployment::bsc_mainnet_v5_27_2;
use std::collections::VecDeque;
use std::fs;
use std::io;
use std::net::{Shutdown, TcpListener};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tls_support::{Served, ServerAlpn, server_config};
use tungstenite::Message;

const CONFIG: &str = include_str!("../../../../config/ingress.public.env");
/// A liveness bound, not a timing claim: a loaded host starts slowly.
const BOUND: Duration = Duration::from_secs(90);

/// A loopback WSS provider: TLS with the test `localhost` certificate, the
/// WebSocket upgrade, one `eth_subscribe` answered (or refused as rate
/// limited), then the queued frames. `cut` drops the serving connection.
struct Wss {
    port: u16,
    ctl: Arc<Ctl>,
}

#[derive(Default)]
struct Ctl {
    rate_limit: AtomicBool,
    frames: Mutex<VecDeque<String>>,
    /// Upgraded connections, subscriptions answered, connections open now.
    accepted: AtomicUsize,
    subscribed: AtomicUsize,
    open: AtomicUsize,
    cut: AtomicUsize,
}

impl Wss {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let ctl = Arc::new(Ctl::default());
        let config = Arc::new(server_config(Served::Localhost, ServerAlpn::Http11));
        let c = Arc::clone(&ctl);
        thread::spawn(move || {
            for tcp in listener.incoming() {
                let Ok(tcp) = tcp else { return };
                let (c, config) = (Arc::clone(&c), Arc::clone(&config));
                thread::spawn(move || serve(tcp, &config, &c));
            }
        });
        Self { port, ctl }
    }

    fn url(&self) -> String {
        format!("wss://localhost:{}/", self.port)
    }

    fn push(&self, frame: String) {
        self.ctl.frames.lock().unwrap().push_back(frame);
    }

    fn cut(&self) {
        self.ctl.cut.fetch_add(1, Ordering::SeqCst);
    }

    fn count(&self, what: &AtomicUsize) -> usize {
        what.load(Ordering::SeqCst)
    }
}

fn serve(tcp: std::net::TcpStream, config: &Arc<rustls::ServerConfig>, c: &Ctl) {
    let cut = c.cut.load(Ordering::SeqCst);
    let tls = rustls::ServerConnection::new(Arc::clone(config)).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let Ok(mut ws) = tungstenite::accept(rustls::StreamOwned::new(tls, tcp)) else {
        return;
    };
    c.accepted.fetch_add(1, Ordering::SeqCst);
    match ws.read() {
        Ok(Message::Text(t)) if t.contains("eth_subscribe") => {}
        _ => return,
    }
    if c.rate_limit.load(Ordering::SeqCst) {
        let _ = ws.send(Message::Text(
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32005,"message":"limit exceeded"}}"#.into(),
        ));
        while ws.read().is_ok() {}
        return;
    }
    if ws
        .send(Message::Text(
            r#"{"jsonrpc":"2.0","id":1,"result":"0x1"}"#.into(),
        ))
        .is_err()
    {
        return;
    }
    c.subscribed.fetch_add(1, Ordering::SeqCst);
    c.open.fetch_add(1, Ordering::SeqCst);
    ws.get_ref()
        .sock
        .set_read_timeout(Some(Duration::from_millis(10)))
        .unwrap();
    loop {
        if c.cut.load(Ordering::SeqCst) != cut {
            let _ = ws.get_ref().sock.shutdown(Shutdown::Both);
            break;
        }
        let next = c.frames.lock().unwrap().pop_front();
        if let Some(frame) = next {
            if ws.send(Message::Text(frame)).is_err() {
                break;
            }
            continue;
        }
        match ws.read() {
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(_) => break,
        }
    }
    c.open.fetch_sub(1, Ordering::SeqCst);
}

/// A signed legacy transaction of a synthetic account as a provider
/// delivers it in full, and its hash.
fn full_tx(secret: u8, nonce: u64, to: Address, input: &[u8]) -> ([u8; 32], String) {
    let signer = Signer::from_secret_bytes(&[secret; 32]).unwrap();
    let tx = LegacyTx {
        chain_id: 56,
        nonce,
        gas_price: 1_000_000_000,
        gas_limit: 6_000_000,
        to,
        value: U256::ZERO,
        input,
    };
    let (mut scratch, mut raw) = (Vec::new(), Vec::new());
    signer.sign_legacy(&tx, &mut scratch, &mut raw).unwrap();
    let signed = decode_signed(&raw).unwrap();
    let hash = keccak256(&raw).0;
    let q = |v: u64| format!("0x{v:x}");
    let word = |w: &[u8; 32]| format!("{:#x}", U256::from_be_bytes(*w));
    let json = format!(
        r#"{{"type":"0x0","hash":"0x{}","from":"0x{}","to":"0x{}","nonce":"{}","gas":"{}","gasPrice":"{}","value":"0x0","input":"0x{}","v":"{}","r":"{}","s":"{}","chainId":"0x38"}}"#,
        hex::encode(hash),
        hex::encode(signer.address()),
        hex::encode(to),
        q(nonce),
        q(tx.gas_limit),
        q(tx.gas_price),
        hex::encode(input),
        q(35 + 2 * 56 + u64::from(signed.signature.y_parity)),
        word(&signed.signature.r),
        word(&signed.signature.s),
    );
    (hash, json)
}

fn notification(result: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","method":"eth_subscription","params":{{"subscription":"0x1","result":{result}}}}}"#
    )
}

/// A synthetic third party's native Flap self-launch (the witnessed V6
/// parameters), as a provider delivers it in full.
/// The synthetic launcher's secret byte.
const LAUNCHER: u8 = 0x5a;

fn launch(nonce: u64) -> ([u8; 32], String) {
    let mut beneficiary = [0u8; 20];
    beneficiary[19] = 0x1c;
    let params = LaunchParamsV6 {
        name: b"DarkBot R11 Harness",
        symbol: b"DBR11",
        uri: b"ipfs://darkbot-r11-harness",
        dex_thresh: 1,
        salt: B256::from(keccak256(nonce.to_be_bytes())),
        migrator_type: 1,
        quote_token: Address::ZERO,
        quote_amt: U256::ZERO,
        beneficiary: Address::from(beneficiary),
        permit: &[],
        extension_id: B256::ZERO,
        extension_data: &[],
        dex_id: 0,
        lp_fee_profile: 0,
        buy_tax: 100,
        sell_tax: 100,
        tax_duration: 31_536_000,
        anti_farmer_duration: 86_400,
        mkt_bps: 10_000,
        deflation_bps: 0,
        dividend_bps: 0,
        lp_bps: 0,
        minimum_share_balance: U256::ZERO,
        dividend_token: Address::ZERO,
        commission_receiver: Address::ZERO,
        token_version: 6,
    };
    let mut input = Vec::new();
    encode_new_token_v6(&params, &mut input);
    full_tx(LAUNCHER, nonce, bsc_mainnet_v5_27_2().portal, &input)
}

/// A closed loopback port: every connect to it is refused.
fn closed_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn private_dir(path: &Path) {
    fs::DirBuilder::new()
        .mode(0o700)
        .recursive(true)
        .create(path)
        .unwrap();
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_bsc_runtime")
}

fn cli(args: &[&str]) -> (i32, String) {
    let out = Command::new(bin()).args(args).output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

/// The shipped configuration for the harness: the given WSS endpoints
/// (none else), status, reconcile and backoff short, the canonical
/// follower at a closed loopback port (no C2 snapshot ever), its second
/// source and fallbacks dropped, the LIVE keys empty as shipped (no LIVE
/// path), the PAPER identity at
/// `identity`. Each `(key, value)` of `set` replaces or adds a line.
fn harness_config(endpoints: &[(&str, &str)], identity: &Path, set: &[(&str, &str)]) -> String {
    let dead = format!("https://localhost:{}", closed_port());
    let mut lines: Vec<(String, String)> = CONFIG
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .filter(|(k, _)| {
            !k.starts_with("BSC_INGRESS_")
                && *k != "BSC_CANONICAL_CONFIRM_RPC"
                && *k != "BSC_CANONICAL_RPC_FALLBACK"
        })
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let mut put = |k: &str, v: &str| match lines.iter_mut().find(|(key, _)| key == k) {
        Some(line) => line.1 = v.to_string(),
        None => lines.push((k.to_string(), v.to_string())),
    };
    for (source, url) in endpoints {
        put(&format!("BSC_INGRESS_{source}_WSS"), url);
    }
    for (k, v) in [
        ("BSC_RUNTIME_STATUS_MS", "100"),
        ("BSC_RUNTIME_RECONCILE_MS", "20"),
        ("BSC_RUNTIME_BACKOFF_INITIAL_MS", "100"),
        ("BSC_RUNTIME_BACKOFF_MAX_MS", "1000"),
        ("BSC_RUNTIME_STABLE_AFTER_MS", "1000"),
        ("BSC_RUNTIME_QUIET_MS", "600000"),
        ("BSC_RUNTIME_IDLE_TIMEOUT_MS", "3600000"),
        ("BSC_RUNTIME_CENSUS_MS", "1000"),
        ("BSC_CANONICAL_RPC", dead.as_str()),
        ("BSC_EXEC_PAPER_IDENTITY", identity.to_str().unwrap()),
    ] {
        put(k, v);
    }
    for (k, v) in set {
        put(k, v);
    }
    // No line may name a real host (a key filled in later included).
    for (k, v) in &lines {
        assert!(
            v.split(',')
                .filter(|u| u.contains("://"))
                .all(|u| u.contains("://localhost:")),
            "{k}={v} names a host other than the loopback"
        );
    }
    lines.iter().map(|(k, v)| format!("{k}={v}\n")).collect()
}

/// One runtime process on its own owner state, run directory and checkout.
struct Rt {
    base: PathBuf,
    /// The run directory (status, journal); `base` unless placed elsewhere.
    rundir: PathBuf,
    state: PathBuf,
    config: PathBuf,
    child: Option<Child>,
    generation: u64,
    requests: u64,
}

impl Rt {
    /// Writes the configuration under `<base>/repo/bsc/config/` (the
    /// checkout holds the frozen gas evidence it names), initializes the
    /// owner and the PAPER identity outside the checkout, and starts.
    fn start(tag: &str, endpoints: &[(&str, &str)], set: &[(&str, &str)]) -> Self {
        Self::start_in(tag, endpoints, set, None)
    }

    fn start_in(
        tag: &str,
        endpoints: &[(&str, &str)],
        set: &[(&str, &str)],
        rundir: Option<&Path>,
    ) -> Self {
        let base = std::env::temp_dir().join(format!("bsc-r11-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        private_dir(&base.join("repo/bsc/config"));
        private_dir(&base.join("keys"));
        let identity = base.join("keys/paper_identity.key");
        let text = harness_config(endpoints, &identity, set);
        RuntimeConfig::parse(&text).unwrap();
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        let evidence = text
            .lines()
            .find_map(|l| l.strip_prefix("BSC_STRATEGY_C_GAS_EVIDENCE="))
            .unwrap();
        for p in evidence.split(',') {
            let to = base.join("repo").join(p);
            fs::create_dir_all(to.parent().unwrap()).unwrap();
            fs::copy(repo.join(p), to).unwrap();
        }
        let config = base.join("repo/bsc/config/run.env");
        fs::write(&config, &text).unwrap();
        let state = base.join("bsc-sources");
        let (code, out) = cli(&["source", "init", "--state", state.to_str().unwrap()]);
        assert_eq!(code, 0, "{out}");
        let (code, out) = cli(&[
            "paper-identity",
            "init",
            "--config",
            config.to_str().unwrap(),
        ]);
        assert_eq!(code, 0, "{out}");
        let mut rt = Self {
            rundir: rundir.map_or_else(|| base.clone(), Path::to_path_buf),
            base,
            state,
            config,
            child: None,
            generation: 0,
            requests: 0,
        };
        rt.run();
        rt
    }

    fn run(&mut self) {
        let started = Instant::now();
        let status = self.status_path();
        let _ = fs::remove_file(&status);
        self.child = Some(
            Command::new(bin())
                .arg("run")
                .arg("--sources")
                .arg(&self.state)
                .arg("--rundir")
                .arg(&self.rundir)
                .arg("--config")
                .arg(&self.config)
                .stderr(
                    fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(self.base.join("stderr.log"))
                        .unwrap(),
                )
                .spawn()
                .unwrap(),
        );
        while !status.exists() {
            assert!(started.elapsed() < BOUND, "no status");
            thread::sleep(Duration::from_millis(20));
        }
        self.generation = self.show_generation();
    }

    /// What the processes of this runtime wrote to stderr (its STOP reason).
    fn stderr(&self) -> String {
        fs::read_to_string(self.base.join("stderr.log")).unwrap_or_default()
    }

    fn status_path(&self) -> PathBuf {
        self.rundir.join("bsc-runtime/status.dbrec")
    }

    fn show(&self) -> String {
        let (code, out) = cli(&[
            "source",
            "show",
            "--state",
            self.state.to_str().unwrap(),
            "--config",
            self.config.to_str().unwrap(),
        ]);
        assert_eq!(code, 0, "{out}");
        out
    }

    fn show_generation(&self) -> u64 {
        self.show()
            .lines()
            .find_map(|l| l.strip_prefix("generation="))
            .unwrap()
            .parse()
            .unwrap()
    }

    /// One owner request through the CLI, at the current generation: its
    /// receipt line (the request must apply).
    fn request(&mut self, id: &str, mode: &str) -> String {
        self.requests += 1;
        let (g, rid) = (
            self.generation.to_string(),
            format!("r11-{}", self.requests),
        );
        let (code, out) = cli(&[
            "source",
            "request",
            id,
            mode,
            "--expect-generation",
            &g,
            "--request-id",
            &rid,
            "--state",
            self.state.to_str().unwrap(),
            "--config",
            self.config.to_str().unwrap(),
        ]);
        assert_eq!(code, 0, "{id} {mode}: {out}");
        self.generation = out
            .split_whitespace()
            .find_map(|w| w.strip_prefix("generation="))
            .unwrap()
            .parse()
            .unwrap();
        out
    }

    fn status(&self) -> Record {
        darkbot_bsc_record::decode(&rs::SCHEMA, &fs::read(self.status_path()).unwrap()).unwrap()
    }

    /// The first status written after now for which `ok` holds.
    fn until(&self, what: &str, ok: impl Fn(&Record) -> bool) -> Record {
        let started = Instant::now();
        let after = fs::metadata(self.status_path())
            .unwrap()
            .modified()
            .unwrap();
        loop {
            let fresh = fs::metadata(self.status_path())
                .and_then(|m| m.modified())
                .is_ok_and(|m| m > after);
            if fresh
                && let Ok(bytes) = fs::read(self.status_path())
                && let Ok(r) = darkbot_bsc_record::decode(&rs::SCHEMA, &bytes)
                && ok(&r)
            {
                return r;
            }
            assert!(started.elapsed() < BOUND, "never: {what}");
            thread::sleep(Duration::from_millis(20));
        }
    }

    /// `n` more status intervals than now (each a whole reconcile pass).
    fn settle(&self, n: u64) -> Record {
        let mut r = self.status();
        for _ in 0..n {
            r = self.until("a later status", |_| true);
        }
        r
    }

    /// Stops through the stop file; the process exits cleanly.
    fn stop(&mut self) {
        fs::write(stop_file(&self.rundir), b"stop\n").unwrap();
        let exit = self.exited("no clean stop");
        assert!(exit.success(), "{exit:?}");
    }

    /// Waits up to BOUND for the process to exit by itself. It stays in
    /// `child` until then, so a failed wait still kills it on drop.
    fn exited(&mut self, why: &str) -> ExitStatus {
        let started = Instant::now();
        loop {
            if let Some(exit) = self.child.as_mut().unwrap().try_wait().unwrap() {
                self.child = None;
                return exit;
            }
            assert!(started.elapsed() < BOUND, "{why}");
            thread::sleep(Duration::from_millis(20));
        }
    }

    /// SIGKILL: no stop path runs.
    fn kill(&mut self) {
        let mut child = self.child.take().unwrap();
        child.kill().unwrap();
        child.wait().unwrap();
    }

    fn census_states(&self) -> Vec<&'static str> {
        let text = fs::read_to_string(&self.config).unwrap();
        let config = RuntimeConfig::parse(&text).unwrap();
        census(
            &self.rundir.join("bsc-runtime/journal"),
            journal_max_payload(&config),
            None,
        )
        .unwrap()
        .windows
        .iter()
        .map(|w| w.state)
        .collect()
    }
}

impl Drop for Rt {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if !thread::panicking() {
            let _ = fs::remove_dir_all(&self.base);
        }
    }
}

/// One source's status row.
fn source(r: &Record, id: &str) -> Record {
    r.nested_all(rs::SOURCES)
        .unwrap()
        .into_iter()
        .find(|row| row.text(rs::source::SOURCE_ID).unwrap() == id)
        .unwrap()
}

fn n(r: &Record, id: &str, tag: u16) -> u64 {
    source(r, id).u64(tag).unwrap()
}

fn health(r: &Record, id: &str) -> String {
    source(r, id).text(rs::source::HEALTH).unwrap().to_string()
}

const PN: &str = "BSC_INGRESS_PUBLICNODE";
const DR: &str = "BSC_INGRESS_DRPC";
const BR: &str = "BSC_INGRESS_BLOCKREQ";

/// §3b A01–A04, A09, A11 and A15: a source connects and subscribes only
/// while its mode runs it, each counts its own frames, an identical full
/// transaction from both is one candidate, and switched off and on again
/// a source leaves no adapter behind.
#[test]
fn the_ingress_runs_exactly_the_sources_their_modes_name() {
    let (p, d, b) = (Wss::start(), Wss::start(), Wss::start());
    let mut rt = Rt::start(
        "modes",
        &[
            ("PUBLICNODE", &p.url()),
            ("DRPC", &d.url()),
            ("BLOCKREQ", &b.url()),
        ],
        &[],
    );
    // A01: every source OFF: no adapter connects.
    let r = rt.settle(3);
    for id in [PN, DR, BR] {
        assert_eq!(health(&r, id), "OFF", "{id}");
    }
    for w in [&p, &d, &b] {
        assert_eq!(w.count(&w.ctl.accepted), 0);
    }
    // A02: PublicNode SHADOW alone: only it subscribes and counts.
    rt.request(PN, "SHADOW");
    rt.until("PublicNode subscribed", |r| {
        n(r, PN, rs::source::CONNECTS) == 1
    });
    let (_, tx1) = full_tx(0x11, 1, Address::repeat_byte(0x22), b"");
    p.push(notification(&tx1));
    let r = rt.until("PublicNode's full transaction", |r| {
        n(r, PN, rs::source::FULL) == 1
    });
    assert_eq!(health(&r, PN), "HEALTHY");
    assert_eq!(health(&r, DR), "OFF");
    assert_eq!((d.count(&d.ctl.accepted), b.count(&b.ctl.accepted)), (0, 0));
    // A04/A09: both SHADOW; the same full transaction from each is one
    // candidate: the later counts as a duplicate, never a second first.
    rt.request(DR, "SHADOW");
    rt.until("dRPC subscribed", |r| n(r, DR, rs::source::CONNECTS) == 1);
    let (_, tx2) = full_tx(0x11, 2, Address::repeat_byte(0x22), b"");
    p.push(notification(&tx2));
    rt.until("the first sighting", |r| n(r, PN, rs::source::FULL) == 2);
    d.push(notification(&tx2));
    let r = rt.until("the later sighting", |r| n(r, DR, rs::source::FULL) == 1);
    assert_eq!(n(&r, DR, rs::source::DUPLICATES), 1);
    assert_eq!(n(&r, DR, rs::source::FIRST_FULL), 0);
    assert_eq!(n(&r, PN, rs::source::FIRST_FULL), 2);
    // A03: PublicNode OFF: its adapter closes, dRPC alone runs on.
    rt.request(PN, "OFF");
    let r = rt.until("PublicNode stopped", |r| health(r, PN) == "OFF");
    let closed = Instant::now();
    while p.count(&p.ctl.open) != 0 {
        assert!(
            closed.elapsed() < BOUND,
            "a PublicNode connection stays open"
        );
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(health(&r, DR), "HEALTHY");
    let (_, tx3) = full_tx(0x11, 3, Address::repeat_byte(0x22), b"");
    d.push(notification(&tx3));
    rt.until("dRPC alone", |r| n(r, DR, rs::source::FULL) == 2);
    // A15: OFF → SHADOW → OFF → SHADOW: one new subscription each time,
    // no adapter of an earlier generation left (one connection open).
    for round in 0..2 {
        rt.request(PN, "SHADOW");
        rt.until("PublicNode again", |r| {
            health(r, PN) == "HEALTHY" || health(r, PN) == "DEGRADED"
        });
        let since = Instant::now();
        while p.count(&p.ctl.subscribed) != 2 + round {
            assert!(since.elapsed() < BOUND, "no new subscription");
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(p.count(&p.ctl.open), 1);
        if round == 0 {
            rt.request(PN, "OFF");
            rt.until("PublicNode off", |r| health(r, PN) == "OFF");
            let closed = Instant::now();
            while p.count(&p.ctl.open) != 0 {
                assert!(closed.elapsed() < BOUND, "the old connection stays open");
                thread::sleep(Duration::from_millis(20));
            }
        }
    }
    // A11: BlockReq SHADOW with an (isolated mock) endpoint: handshake,
    // subscription, a disconnect and the recovery, counted as its own.
    rt.request(BR, "SHADOW");
    rt.until("BlockReq subscribed", |r| {
        n(r, BR, rs::source::CONNECTS) == 1
    });
    b.cut();
    let r = rt.until("BlockReq recovered", |r| {
        n(r, BR, rs::source::CONNECTS) == 2
    });
    assert_eq!(n(&r, BR, rs::source::DISCONNECTS), 1);
    assert_eq!(n(&r, DR, rs::source::DISCONNECTS), 0);
    rt.stop();
}

/// §3b A05–A08 and A16: a disconnect or a rate limit is counted at the
/// source it hit and spares the other one; nothing it did not deliver in
/// full and verified counts as a full transaction or a launch; a hash seen
/// first never makes its source the launch's first.
#[test]
fn a_failing_source_is_counted_as_its_own_and_never_actionable() {
    let (p, d) = (Wss::start(), Wss::start());
    let mut rt = Rt::start(
        "faults",
        &[("PUBLICNODE", &p.url()), ("DRPC", &d.url())],
        &[],
    );
    rt.request(PN, "SHADOW");
    rt.request(DR, "SHADOW");
    rt.until("both subscribed", |r| {
        n(r, PN, rs::source::CONNECTS) == 1 && n(r, DR, rs::source::CONNECTS) == 1
    });
    // A05: PublicNode drops; dRPC delivers on, PublicNode recovers.
    p.cut();
    let (_, tx) = full_tx(0x11, 1, Address::repeat_byte(0x22), b"");
    d.push(notification(&tx));
    let r = rt.until("PublicNode recovered", |r| {
        n(r, PN, rs::source::CONNECTS) == 2 && n(r, DR, rs::source::FULL) == 1
    });
    assert_eq!(n(&r, PN, rs::source::DISCONNECTS), 1);
    assert_eq!(
        (
            n(&r, DR, rs::source::DISCONNECTS),
            n(&r, DR, rs::source::CONNECTS)
        ),
        (0, 1)
    );
    // A06: the other way round.
    d.cut();
    let (_, tx) = full_tx(0x11, 2, Address::repeat_byte(0x22), b"");
    p.push(notification(&tx));
    let r = rt.until("dRPC recovered", |r| {
        n(r, DR, rs::source::CONNECTS) == 2 && n(r, PN, rs::source::FULL) == 1
    });
    assert_eq!(n(&r, DR, rs::source::DISCONNECTS), 1);
    assert_eq!(n(&r, PN, rs::source::DISCONNECTS), 1);
    // A08: the launch's hash from PublicNode first, then the verified full
    // launch from dRPC: dRPC's is the launch's first sighting; the hash
    // alone counts as no launch and as no full transaction.
    let (hash, full) = launch(1);
    p.push(notification(&format!("\"0x{}\"", hex::encode(hash))));
    rt.until("the hash", |r| n(r, PN, rs::source::HASH_ONLY) == 1);
    d.push(notification(&full));
    let r = rt.until("the full launch", |r| {
        n(r, DR, rs::source::LAUNCH_SIGHTINGS) == 1
    });
    assert_eq!(n(&r, DR, rs::source::LAUNCH_FIRST), 1);
    assert_eq!(n(&r, PN, rs::source::FIRST_ANY), 2);
    assert_eq!(
        (
            n(&r, PN, rs::source::LAUNCH_SIGHTINGS),
            n(&r, PN, rs::source::LAUNCH_FIRST),
            n(&r, PN, rs::source::FULL)
        ),
        (0, 0, 1)
    );
    assert_eq!(r.u64(rs::LAUNCHES).unwrap(), 1);
    // A16: well-formed full transactions whose hash or signer does not
    // hold (only verification refuses them), and one of a type the source
    // does not take, are refused and counted; none becomes a full
    // transaction or a launch.
    let (hash, good) = launch(2);
    let mut other = hash;
    other[31] ^= 1;
    let forged_hash = good.replace(&hex::encode(hash), &hex::encode(other));
    let signer = hex::encode(
        Signer::from_secret_bytes(&[LAUNCHER; 32])
            .unwrap()
            .address(),
    );
    let forged_signer = good.replace(&signer, &"33".repeat(20));
    let typed = good.replace(r#""type":"0x0""#, r#""type":"0x7""#);
    for frame in [&forged_hash, &forged_signer, &typed] {
        assert_ne!(frame, &good);
        p.push(notification(frame));
    }
    let r = rt.until("three refusals", |r| n(r, PN, rs::source::REJECTED) == 3);
    assert_eq!(
        (
            n(&r, PN, rs::source::FULL),
            n(&r, PN, rs::source::LAUNCH_SIGHTINGS)
        ),
        (1, 0)
    );
    assert_eq!(r.u64(rs::LAUNCHES).unwrap(), 1);
    // A07: both providers rate limit every subscription: the health says
    // so, the counts rise, and no transaction is invented.
    p.ctl.rate_limit.store(true, Ordering::SeqCst);
    d.ctl.rate_limit.store(true, Ordering::SeqCst);
    p.cut();
    d.cut();
    let r = rt.until("both rate limited", |r| {
        health(r, PN) == "RATE_LIMITED" && health(r, DR) == "RATE_LIMITED"
    });
    assert!(n(&r, PN, rs::source::RATE_LIMITED) >= 1);
    assert!(n(&r, DR, rs::source::RATE_LIMITED) >= 1);
    assert_eq!(
        (n(&r, PN, rs::source::FULL), n(&r, DR, rs::source::FULL)),
        (1, 2)
    );
    rt.stop();
}

/// §3b A10, A12–A14: a source without an endpoint, the geth source
/// without its arena and 48Club without provider access never connect; a
/// LIVE request of any source without a grant stays BLOCKED, ECON_LIVE
/// enabled or not, and connects nothing either.
#[test]
fn unqualified_and_live_requested_sources_connect_nothing() {
    let (p, d) = (Wss::start(), Wss::start());
    let mut rt = Rt::start(
        "blocked",
        &[("PUBLICNODE", &p.url()), ("DRPC", &d.url())],
        &[],
    );
    // A10/A12: SHADOW without an endpoint (BlockReq) or arena (geth).
    for id in [BR, "BSC_INGRESS_GETH"] {
        let line = rt.request(id, "SHADOW");
        assert!(line.contains("effective=SHADOW"), "{line}");
    }
    let r = rt.until("both unqualified", |r| {
        health(r, BR) == "UNQUALIFIED" && health(r, "BSC_INGRESS_GETH") == "UNQUALIFIED"
    });
    assert_eq!(n(&r, BR, rs::source::CONNECTS), 0);
    // A13: 48Club SHADOW: blocked by the owner, no provider bought.
    let line = rt.request("BSC_INGRESS_48CLUB", "SHADOW");
    assert!(
        line.contains("effective=BLOCKED block_reason=SOURCE_ACCESS_UNQUALIFIED"),
        "{line}"
    );
    // A14/C12: each source, the runtime mode, each component and venue
    // LIVE, first with ECON_LIVE DISABLED, then ENABLED: without a grant
    // every one stays BLOCKED, for its exact reason.
    let all = [PN, DR, BR, "BSC_INGRESS_GETH", "BSC_INGRESS_48CLUB"];
    let others = [
        "BSC_RUNTIME_MODE",
        "BSC_STATE_C2",
        "BSC_EXEC",
        "BSC_STRATEGY_C",
        "BSC_VENUE_FLAP",
        "BSC_VENUE_FOUR_CLASSIC",
        "BSC_VENUE_OPENFOUR",
    ];
    let (mut got, mut want) = (Vec::new(), Vec::new());
    for econ_live in ["DISABLED", "ENABLED"] {
        if econ_live == "ENABLED" {
            rt.request("BSC_ECON_LIVE", "ENABLED");
        }
        for id in all.iter().chain(&others) {
            let line = rt.request(id, "LIVE");
            let blocked = line
                .split_whitespace()
                .find_map(|w| w.strip_prefix("block_reason="))
                .filter(|_| line.contains("effective=BLOCKED"));
            got.push(format!("{id} {econ_live} {blocked:?}"));
            // The economic switch, then the missing grant (plan 28.10.:
            // LIVE only with a grant; SC4 d: the Four.meme venues too).
            let reason = match (*id, econ_live) {
                (_, "DISABLED") => "ECONOMIC_LIVE_DISABLED",
                _ => "LIVE_GRANT_MISSING",
            };
            want.push(format!("{id} {econ_live} {:?}", Some(reason)));
        }
    }
    assert_eq!(got, want);
    let r = rt.settle(3);
    for id in all {
        assert_eq!(health(&r, id), "OFF", "{id}");
    }
    assert_eq!((p.count(&p.ctl.accepted), d.count(&d.ctl.accepted)), (0, 0));
    rt.stop();
}

/// Strategy C's included launches while it runs (its status section exists
/// only then: switched off, its ledger ends). It takes a launch only once
/// the chain includes it (the canonical join), never a mempool sighting.
fn strategy(r: &Record) -> Option<u64> {
    r.nested(rs::STRATEGY)
        .ok()
        .map(|s| s.u64(rs::strategy::LAUNCHES).unwrap())
}

fn exec_state(r: &Record) -> String {
    r.nested(rs::EXEC)
        .map(|e| e.text(rs::exec::STATE).unwrap().to_string())
        .unwrap_or_else(|_| "ABSENT".into())
}

/// §3b B01–B06, B08 and B10–B13 and B16 as far as the process shows them
/// without a chain (what the strategy and C3 do with an included launch
/// needs a C2 snapshot: the mock-chain cases): the runtime mode, the
/// venues and Strategy C decide what runs; C3 is wanted only under PAPER
/// and arms nothing without the chain; Four Classic and OpenFour SHADOW are
/// admitted (R12c/R12d: their C3 PAPER path); a launch read from the
/// mempool reaches no ledger; switching the runtime OFF ends the
/// strategy's ledger and the run replays exactly.
#[test]
fn the_runtime_mode_and_the_venues_decide_what_runs() {
    let p = Wss::start();
    let mut rt = Rt::start("matrix", &[("PUBLICNODE", &p.url())], &[]);
    let mut nonce = 0;
    // One launch from PublicNode, read by the ingress; the first status
    // that counts it.
    let mut deliver = |rt: &Rt| {
        nonce += 1;
        let (_, full) = launch(nonce);
        p.push(notification(&full));
        rt.until("the launch", |r| r.u64(rs::LAUNCHES).unwrap() == nonce)
    };
    // B01: everything OFF.
    let r = rt.settle(2);
    assert_eq!((strategy(&r), exec_state(&r).as_str()), (None, "OFF"));
    // B02: runtime OFF, PublicNode and C2 SHADOW: the launch is read,
    // neither the strategy nor C3 runs.
    rt.request(PN, "SHADOW");
    rt.request("BSC_STATE_C2", "SHADOW");
    let r = deliver(&rt);
    assert_eq!((strategy(&r), exec_state(&r).as_str()), (None, "OFF"));
    // B03: runtime SHADOW, Flap and Strategy C SHADOW: the strategy's
    // PAPER ledger runs (a mempool sighting is no included launch), C3
    // does not.
    rt.request("BSC_RUNTIME_MODE", "SHADOW");
    rt.request("BSC_VENUE_FLAP", "SHADOW");
    rt.request("BSC_STRATEGY_C", "SHADOW");
    rt.until("the strategy running", |r| strategy(r) == Some(0));
    let r = deliver(&rt);
    assert_eq!((strategy(&r), exec_state(&r).as_str()), (Some(0), "OFF"));
    // B04: EXEC SHADOW under runtime SHADOW: no C3 hand-off.
    rt.request("BSC_EXEC", "SHADOW");
    assert_eq!(exec_state(&rt.settle(2)), "OFF");
    // B05: runtime PAPER: C3 is wanted; the chain is unreachable here, so
    // its venue probe fails and it arms nothing (fail closed).
    rt.request("BSC_RUNTIME_MODE", "PAPER");
    rt.until("C3 refused without the chain", |r| {
        exec_state(r) == "PROBE_FAILED"
    });
    // B10: EXEC OFF under PAPER: no C3; B06: the ledger runs on.
    rt.request("BSC_EXEC", "OFF");
    let r = rt.until("C3 off", |r| exec_state(r) == "OFF");
    assert_eq!(strategy(&r), Some(0));
    // B11–B13: Four Classic and OpenFour SHADOW are admitted (R12c/R12d:
    // the C3 PAPER path; C3 stays off with EXEC OFF); Flap is untouched.
    for venue in ["BSC_VENUE_FOUR_CLASSIC", "BSC_VENUE_OPENFOUR"] {
        let line = rt.request(venue, "SHADOW");
        assert!(line.contains("effective=SHADOW"), "{line}");
    }
    assert_eq!(strategy(&deliver(&rt)), Some(0));
    // B08: Flap OFF: SC4 the strategy's ledger runs on while a Four.meme
    // venue is admitted (started over on them), and ends with them OFF; C3
    // stays off.
    rt.request("BSC_VENUE_FLAP", "OFF");
    assert!(strategy(&rt.settle(2)).is_some());
    for venue in ["BSC_VENUE_FOUR_CLASSIC", "BSC_VENUE_OPENFOUR"] {
        rt.request(venue, "OFF");
    }
    rt.until("the ledger ended", |r| strategy(r).is_none());
    let r = deliver(&rt);
    assert_eq!((strategy(&r), exec_state(&r).as_str()), (None, "OFF"));
    // B16: runtime OFF → SHADOW → PAPER → OFF with a launch after each:
    // only SHADOW and PAPER run the ledger; OFF ends it again.
    rt.request("BSC_VENUE_FLAP", "SHADOW");
    for (mode, running) in [
        ("OFF", false),
        ("SHADOW", true),
        ("PAPER", true),
        ("OFF", false),
    ] {
        rt.request("BSC_RUNTIME_MODE", mode);
        let g = rt.generation;
        rt.until(mode, |r| {
            r.u64(rs::GENERATION).unwrap() == g && strategy(r).is_some() == running
        });
        let r = deliver(&rt);
        assert_eq!(strategy(&r).is_some(), running, "{mode}");
        assert_eq!(exec_state(&r), "OFF", "{mode}");
    }
    rt.stop();
    // The journal of the run replays to what the process reported.
    let (code, out) = cli(&[
        "replay-proof",
        "--rundir",
        rt.rundir.to_str().unwrap(),
        "--config",
        rt.config.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains(": PASS"), "{out}");
    assert_eq!(rt.census_states(), ["COMPLETE"]);
}

/// §3b C15 and the crash case: a killed runtime ran no stop path; started
/// again, the owner has every wallet and economic switch DISABLED
/// (RESET_BY_RESTART) while the daily limit persists, the killed run's
/// journal is read as what it is, and the new run ends cleanly.
#[test]
fn a_killed_runtime_restarts_disabled_with_its_limit_and_journal() {
    let p = Wss::start();
    let mut rt = Rt::start("crash", &[("PUBLICNODE", &p.url())], &[]);
    rt.request(PN, "SHADOW");
    let switches = [
        "BSC_WALLET_LAUNCHER",
        "BSC_WALLET_BUYER",
        "BSC_ECON_SIGN",
        "BSC_ECON_ARM",
        "BSC_ECON_SEND",
        "BSC_ECON_ATTEMPT",
        "BSC_ECON_LIVE",
    ];
    for id in switches {
        rt.request(id, "ENABLED");
    }
    rt.request("BSC_BUY_ATTEMPTS_PER_DAY", "3");
    let (_, tx) = full_tx(0x11, 1, Address::repeat_byte(0x22), b"");
    p.push(notification(&tx));
    rt.until("the transaction", |r| n(r, PN, rs::source::FULL) == 1);
    rt.kill();
    assert!(!stop_file(&rt.rundir).exists());
    rt.run();
    let show = rt.show();
    for id in switches {
        let line = show
            .lines()
            .find(|l| l.starts_with(&format!("{id} ")))
            .unwrap();
        assert!(
            line.contains("requested=DISABLED effective=DISABLED")
                || line.contains("block_reason=RESET_BY_RESTART"),
            "{line}"
        );
    }
    let limit = show
        .lines()
        .find(|l| l.starts_with("BSC_BUY_ATTEMPTS_PER_DAY "))
        .unwrap();
    assert!(limit.contains("requested=3"), "{limit}");
    // The source mode survives the restart; the new run reconnects.
    rt.until("PublicNode again", |r| n(r, PN, rs::source::CONNECTS) == 1);
    rt.stop();
    let states = rt.census_states();
    assert_eq!(states.len(), 2, "{states:?}");
    assert_eq!(states[1], "COMPLETE", "{states:?}");
    assert_ne!(states[0], "COMPLETE", "{states:?}");
}

/// §3b C17–C19 on the real binaries: two requests on one generation from
/// separate processes (terminal and gateway alike call the CLI) apply
/// exactly once; a request ID replayed with the same request returns its
/// receipt, with another request it is refused; an owner the running
/// runtime can no longer read stops it (its sources close) instead of
/// running on an unread state.
#[test]
fn owner_races_replays_and_an_unreadable_owner_fail_closed() {
    use std::os::unix::fs::PermissionsExt;
    let p = Wss::start();
    let mut rt = Rt::start("owner", &[("PUBLICNODE", &p.url())], &[]);
    let state = rt.state.to_str().unwrap().to_string();
    let config = rt.config.to_str().unwrap().to_string();
    // C17: five rounds of two concurrent requests on the same generation.
    for round in 0..5 {
        let g = rt.show_generation().to_string();
        let spawn = |id: &str, mode: &str, rid: String| {
            Command::new(bin())
                .args(["source", "request", id, mode, "--expect-generation", &g])
                .args(["--request-id", &rid, "--state", &state, "--config", &config])
                .output()
                .unwrap()
        };
        let (a, b) = thread::scope(|s| {
            let a = s.spawn(|| spawn("BSC_ECON_SIGN", "ENABLED", format!("cli-{round}")));
            let b = s.spawn(|| spawn("BSC_ECON_SIGN", "DISABLED", format!("gui-{round}")));
            (a.join().unwrap(), b.join().unwrap())
        });
        let outs = [&a, &b].map(|o| String::from_utf8_lossy(&o.stdout).to_string());
        let applied = outs
            .iter()
            .filter(|o| o.contains("outcome=APPLIED"))
            .count();
        let stale = outs
            .iter()
            .filter(|o| o.contains("outcome=REJECTED_STALE"))
            .count();
        assert_eq!((applied, stale), (1, 1), "{outs:?}");
        assert_eq!(a.status.success(), outs[0].contains("outcome=APPLIED"));
        assert_eq!(rt.show_generation(), g.parse::<u64>().unwrap() + 1);
    }
    // C18: the same request ID and request again: its receipt, nothing
    // applied twice; the same ID with another request: refused.
    rt.generation = rt.show_generation();
    let g = rt.generation.to_string();
    let request = |mode: &str| {
        cli(&[
            "source",
            "request",
            PN,
            mode,
            "--expect-generation",
            &g,
            "--request-id",
            "c18",
            "--state",
            &state,
            "--config",
            &config,
        ])
    };
    let (code, first) = request("SHADOW");
    assert_eq!(code, 0, "{first}");
    let (code, again) = request("SHADOW");
    assert_eq!(code, 0, "{again}");
    let generation = |o: &str| {
        o.split_whitespace()
            .find_map(|w| w.strip_prefix("generation="))
            .map(str::to_string)
    };
    assert_eq!(generation(&first), generation(&again), "{first} {again}");
    assert_eq!(rt.show_generation(), rt.generation + 1);
    let (code, other) = request("OFF");
    assert_ne!(code, 0, "{other}");
    assert!(
        other.contains("already belongs to another request"),
        "{other}"
    );
    // C19: the owner turns unreadable under the running runtime.
    rt.until("PublicNode subscribed", |r| {
        n(r, PN, rs::source::CONNECTS) == 1
    });
    let dir = rt.state.clone();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o000)).unwrap();
    let mut child = rt.child.take().unwrap();
    let started = Instant::now();
    let exit = loop {
        if let Some(exit) = child.try_wait().unwrap() {
            break exit;
        }
        if started.elapsed() > BOUND {
            let _ = child.kill();
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
            panic!("the runtime ran on with an unreadable owner");
        }
        thread::sleep(Duration::from_millis(20));
    };
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(!exit.success(), "{exit:?}");
    let why = rt.stderr();
    assert!(
        why.contains("STOP") && why.contains("Permission denied"),
        "{why}"
    );
    let closed = Instant::now();
    while p.count(&p.ctl.open) != 0 {
        assert!(closed.elapsed() < BOUND, "a source stayed connected");
        thread::sleep(Duration::from_millis(20));
    }
}

/// A disk image of its own, attached for one test and detached after.
#[cfg(target_os = "macos")]
struct Volume {
    mount: PathBuf,
}

#[cfg(target_os = "macos")]
impl Volume {
    fn attach(dir: &Path, megabytes: u32) -> Self {
        let (image, mount) = (dir.join("volume.dmg"), dir.join("volume"));
        fs::create_dir_all(&mount).unwrap();
        let ok = |args: &[&str]| {
            let out = Command::new("/usr/bin/hdiutil")
                .args(args)
                .output()
                .unwrap();
            assert!(out.status.success(), "hdiutil {args:?}: {out:?}");
        };
        ok(&[
            "create",
            "-quiet",
            "-size",
            &format!("{megabytes}m"),
            "-fs",
            "HFS+",
            "-volname",
            "bscr11",
            "-type",
            "UDIF",
            image.to_str().unwrap(),
        ]);
        ok(&[
            "attach",
            "-quiet",
            "-nobrowse",
            "-noautoopen",
            "-mountpoint",
            mount.to_str().unwrap(),
            image.to_str().unwrap(),
        ]);
        Self { mount }
    }

    /// Writes filler files until the volume refuses another byte.
    fn fill(&self) {
        use std::io::Write;
        let mut files: Vec<_> = (0..3)
            .map(|i| fs::File::create(self.mount.join(format!("filler-{i}"))).unwrap())
            .collect();
        for (f, chunk) in files.iter_mut().zip([1 << 20, 4096, 512]) {
            while f.write_all(&vec![0u8; chunk]).is_ok() {}
        }
    }

    fn free(&self) {
        for i in 0..3 {
            let _ = fs::remove_file(self.mount.join(format!("filler-{i}")));
        }
    }
}

#[cfg(target_os = "macos")]
impl Drop for Volume {
    fn drop(&mut self) {
        let _ = Command::new("/usr/bin/hdiutil")
            .args(["detach", "-quiet", "-force"])
            .arg(&self.mount)
            .status();
    }
}

/// The disk-full case on an isolated test volume (Messplan §9.3 F/I): the
/// run directory fills up under a running runtime that keeps receiving.
/// It stops at once (it never runs on without its status or journal), its
/// source closes, the cut run reads as incomplete (cut mid-frame or after
/// one) and no replay proves it;
/// with the space back, the next start runs and ends sealed.
#[cfg(target_os = "macos")]
#[test]
fn a_full_disk_stops_the_runtime_and_its_run_reads_incomplete() {
    let scratch = std::env::temp_dir().join(format!("bsc-r11-disk-{}", std::process::id()));
    let _ = fs::remove_dir_all(&scratch);
    private_dir(&scratch);
    let volume = Volume::attach(&scratch, 16);
    let rundir = volume.mount.join("run");
    private_dir(&rundir);
    let p = Wss::start();
    let mut rt = Rt::start_in(
        "disk",
        &[("PUBLICNODE", &p.url())],
        &[
            ("BSC_RUNTIME_CAPTURE_PENDING", "ALL"),
            ("BSC_RUNTIME_JOURNAL_SYNC_RECORDS", "1"),
        ],
        Some(&rundir),
    );
    rt.request(PN, "SHADOW");
    rt.until("subscribed", |r| n(r, PN, rs::source::CONNECTS) == 1);
    for i in 0..5 {
        let (_, tx) = full_tx(0x11, i, Address::repeat_byte(0x22), b"");
        p.push(notification(&tx));
    }
    rt.until("five", |r| n(r, PN, rs::source::FULL) == 5);
    volume.fill();
    for i in 5..50 {
        let (_, tx) = full_tx(0x11, i, Address::repeat_byte(0x22), &[0xab; 2048]);
        p.push(notification(&tx));
    }
    let exit = rt.exited("the runtime ran on with a full disk");
    assert!(!exit.success(), "{exit:?}");
    let why = rt.stderr();
    assert!(
        why.contains("STOP") && why.contains("No space left on device"),
        "{why}"
    );
    let closed = Instant::now();
    while p.count(&p.ctl.open) != 0 {
        assert!(closed.elapsed() < BOUND, "the source stayed connected");
        thread::sleep(Duration::from_millis(20));
    }
    // Cut mid-frame or between frames: incomplete either way, never COMPLETE.
    let cut = rt.census_states();
    assert!(
        cut.len() == 1 && ["TAIL_INCOMPLETE", "UNTERMINATED"].contains(&cut[0]),
        "{cut:?}"
    );
    let proof = |rt: &Rt| {
        cli(&[
            "replay-proof",
            "--rundir",
            rundir.to_str().unwrap(),
            "--config",
            rt.config.to_str().unwrap(),
        ])
    };
    volume.free();
    let (code, out) = proof(&rt);
    assert_ne!(code, 0, "{out}");
    assert!(out.contains("not sealed"), "{out}");
    rt.run();
    rt.until("subscribed again", |r| n(r, PN, rs::source::CONNECTS) == 1);
    rt.stop();
    assert_eq!(rt.census_states(), [cut[0], "COMPLETE"]);
    let (code, out) = proof(&rt);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains(": PASS"), "{out}");
    drop(rt);
    drop(volume);
    let _ = fs::remove_dir_all(&scratch);
}
