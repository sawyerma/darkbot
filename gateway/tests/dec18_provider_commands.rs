//! DEC-18 §1.3 Nr. 9 over the real HTTP stack (`POST /api/v1/command`, `GET` lookup): provider
//! commands — a credential's secret among them — need the session, the allowed origin, the CSRF
//! token and the protobuf body before anything is recorded or run; an acknowledgement lost with
//! its connection is looked up, never run twice; concurrent inputs reach the one native
//! controller one at a time and a doubled request once. The controller is a stand-in executable
//! at `DARKBOT_CTL`'s place (as in the controller's unit tests): it records each run, its
//! standard input and any overlap, and answers as `p0ctl provider` does.
use darkbot_gateway::{
    config::{GatewayConfig, Limits},
    control::{Control, Paths},
    engine::SourceRegistry,
    http::{self, AppState},
    hub::StateHub,
    proto::{
        expected_version, project0::v1::ProviderCredentialCommand, CommandLookupResponse,
        CommandPhase, CommandRequest, CommandResponse, ExpectedVersion, ProjectId,
    },
    session::SessionStore,
};
use prost::Message as _;
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{atomic::AtomicUsize, Arc},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const SECRET: &str = "PROVIDER-SECRET-139";

struct Server {
    addr: SocketAddr,
    state: Arc<AppState>,
    /// The stand-in controller's own records (outside the gateway's run and log directories).
    probe: PathBuf,
    /// The gateway's run and log directory (its command ledger lives below).
    gateway: PathBuf,
}

fn executable(path: &Path, script: &str) {
    use std::os::unix::fs::PermissionsExt;
    let source = path.with_extension("source");
    std::fs::write(&source, script).unwrap();
    // Copied, never written in place: an executable written and run at once can be busy (ETXTBSY).
    assert!(std::process::Command::new("/bin/cp")
        .arg(&source)
        .arg(path)
        .status()
        .unwrap()
        .success());
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A gateway on loopback whose controller is the stand-in: each run is logged, its standard
/// input kept, an overlapping run marked; it takes `delay` seconds, then answers as `p0ctl
/// provider` with an applied receipt.
async fn spawn(tag: &str, delay: &str) -> Server {
    let base = std::env::temp_dir().join(format!("gw-139-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (probe, gateway) = (base.join("probe"), base.join("gateway"));
    std::fs::create_dir_all(&probe).unwrap();
    std::fs::create_dir_all(gateway.join("run")).unwrap();
    let ctl = probe.join("darkbotctl");
    executable(
        &ctl,
        &format!(
            "#!/bin/sh\nP='{p}'\nif ! mkdir \"$P/busy\" 2>/dev/null; then echo overlap >> \"$P/overlaps\"; fi\necho \"$*\" >> \"$P/runs\"\ncat > \"$P/stdin.$$\"\nsleep {delay}\nrmdir \"$P/busy\"\necho done >> \"$P/finished\"\necho 'PROVIDER_RECEIPT request_id=x applied=true version=5 action=\"credential set helius\" detail=\"stored\"'\n",
            p = probe.display()
        ),
    );
    let launchctl = probe.join("launchctl");
    executable(
        &launchctl,
        "#!/bin/sh\necho \"Could not find service \\\"$2\\\" in domain for port\" >&2\nexit 113\n",
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let limits = Limits::default();
    let cfg = GatewayConfig {
        bind: addr,
        static_dir: None,
        allowed_origins: vec![format!("http://{addr}")],
        allowed_hosts: vec![addr.to_string()],
        session_cookie: "darkbot_session".into(),
        limits: limits.clone(),
        source_stale_after_ms: 1000,
        source_detach_after_ms: 2000,
        source_reattach_backoff_ms: 50,
        overview_interval_ms: 60000,
        feeds: vec![],
    };
    let state = Arc::new(AppState {
        control: Control::new(
            Some(Paths {
                ctl,
                rundir: gateway.join("run"),
                logdir: gateway.clone(),
                label: "com.darkbot.test.absent.capture".into(),
                launchctl,
            }),
            &limits,
        ),
        cfg,
        hub: Arc::new(StateHub::new(1024)),
        registry: SourceRegistry::new(),
        sessions: SessionStore::new(64, Duration::from_secs(3600)),
        clients: AtomicUsize::new(0),
        started_at: Instant::now(),
    });
    let served = Arc::clone(&state);
    tokio::spawn(async move {
        axum::serve(listener, http::router(served))
            .await
            .expect("test server")
    });
    Server {
        addr,
        state,
        probe,
        gateway,
    }
}

impl Server {
    fn runs(&self) -> Vec<String> {
        std::fs::read_to_string(self.probe.join("runs"))
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }
    fn finished(&self) -> usize {
        std::fs::read_to_string(self.probe.join("finished"))
            .unwrap_or_default()
            .lines()
            .count()
    }
    fn overlaps(&self) -> usize {
        std::fs::read_to_string(self.probe.join("overlaps"))
            .unwrap_or_default()
            .lines()
            .count()
    }
    /// Every byte the gateway keeps on disk (its ledger, archive and logs).
    fn kept(&self) -> Vec<u8> {
        fn walk(dir: &Path, out: &mut Vec<u8>) {
            for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else {
                    out.extend(std::fs::read(&p).unwrap_or_default());
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.gateway, &mut out);
        out
    }
    fn session(&self) -> (String, String) {
        let token = self.state.sessions.issue();
        let csrf = self.state.sessions.csrf_of(&token).unwrap();
        (token, csrf)
    }
}

fn holds(bytes: &[u8], needle: &str) -> bool {
    bytes.windows(needle.len()).any(|w| w == needle.as_bytes())
}

fn credential_set(n: u8, version: u64) -> CommandRequest {
    CommandRequest {
        project: ProjectId::Project0 as i32,
        request_id: format!("{n:08x}-1390-4390-8390-139013901390"),
        action: "project0.provider.credential.set".into(),
        resource_id: String::new(),
        expected: Some(ExpectedVersion {
            version: Some(expected_version::Version::ConfigVersion(version)),
        }),
        params: ProviderCredentialCommand {
            credential: "helius".into(),
            secret: SECRET.as_bytes().to_vec().into(),
        }
        .encode_to_vec()
        .into(),
        issued_at_unix_ns: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as i64,
    }
}

/// One raw HTTP/1.1 exchange: the status and the body.
async fn exchange(addr: SocketAddr, head: &str, body: &[u8]) -> (u16, Vec<u8>) {
    let mut conn = TcpStream::connect(addr).await.unwrap();
    conn.write_all(head.as_bytes()).await.unwrap();
    conn.write_all(body).await.unwrap();
    let mut raw = Vec::new();
    conn.read_to_end(&mut raw).await.unwrap();
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("a response head");
    let status = std::str::from_utf8(&raw[9..12]).unwrap().parse().unwrap();
    (status, raw[split + 4..].to_vec())
}

/// The headers of a command POST; each guard can be left out or falsified.
#[derive(Clone)]
struct Post {
    cookie: Option<String>,
    origin: Option<String>,
    fetch_site: Option<&'static str>,
    csrf: Option<String>,
    content_type: &'static str,
}

fn post_head(s: &Server, p: &Post, length: usize) -> String {
    let mut head = format!(
        "POST /api/v1/command HTTP/1.1\r\nHost: {}\r\nContent-Type: {}\r\nContent-Length: {length}\r\nConnection: close\r\n",
        s.addr, p.content_type
    );
    if let Some(c) = &p.cookie {
        head += &format!("Cookie: darkbot_session={c}\r\n");
    }
    if let Some(o) = &p.origin {
        head += &format!("Origin: {o}\r\n");
    }
    if let Some(f) = p.fetch_site {
        head += &format!("Sec-Fetch-Site: {f}\r\n");
    }
    if let Some(t) = &p.csrf {
        head += &format!("x-csrf-token: {t}\r\n");
    }
    head + "\r\n"
}

fn allowed(s: &Server) -> Post {
    let (token, csrf) = s.session();
    Post {
        cookie: Some(token),
        origin: Some(format!("http://{}", s.addr)),
        fetch_site: Some("same-origin"),
        csrf: Some(csrf),
        content_type: "application/x-protobuf",
    }
}

async fn post(s: &Server, p: &Post, req: &CommandRequest) -> (u16, Option<CommandResponse>) {
    let body = req.encode_to_vec();
    let (status, bytes) = exchange(s.addr, &post_head(s, p, body.len()), &body).await;
    (
        status,
        (status == 200).then(|| CommandResponse::decode(bytes.as_slice()).unwrap()),
    )
}

async fn lookup(s: &Server, token: &str, id: &str) -> CommandLookupResponse {
    let head = format!(
        "GET /api/v1/command?project={}&request_id={id} HTTP/1.1\r\nHost: {}\r\nCookie: darkbot_session={token}\r\nSec-Fetch-Site: same-origin\r\nConnection: close\r\n\r\n",
        ProjectId::Project0 as i32,
        s.addr
    );
    let (status, bytes) = exchange(s.addr, &head, b"").await;
    assert_eq!(status, 200);
    CommandLookupResponse::decode(bytes.as_slice()).unwrap()
}

/// DEC-18 §1.3 Nr. 9 (Berechtigung): a provider command without its session, from another
/// origin or site, without or with a wrong CSRF token, or not as protobuf is refused before
/// anything is recorded or run — the controller never starts and the secret is kept nowhere;
/// the same request with every guard runs once.
#[tokio::test]
async fn a_provider_command_needs_session_origin_csrf_and_protobuf_before_anything_runs() {
    let s = spawn("auth", "0").await;
    let ok = allowed(&s);
    let req = credential_set(1, 4);
    for (name, p, status) in [
        (
            "no session",
            Post {
                cookie: None,
                ..ok.clone()
            },
            401,
        ),
        (
            "unknown session",
            Post {
                cookie: Some("forged".into()),
                ..ok.clone()
            },
            401,
        ),
        (
            "foreign origin",
            Post {
                origin: Some("http://evil.example".into()),
                ..ok.clone()
            },
            403,
        ),
        (
            "no origin",
            Post {
                origin: None,
                ..ok.clone()
            },
            403,
        ),
        (
            "cross-site",
            Post {
                fetch_site: Some("cross-site"),
                ..ok.clone()
            },
            403,
        ),
        (
            "no csrf",
            Post {
                csrf: None,
                ..ok.clone()
            },
            403,
        ),
        (
            "wrong csrf",
            Post {
                csrf: Some("0".repeat(32)),
                ..ok.clone()
            },
            403,
        ),
        (
            "not protobuf",
            Post {
                content_type: "application/json",
                ..ok.clone()
            },
            415,
        ),
    ] {
        let (got, _) = post(&s, &p, &req).await;
        assert_eq!(got, status, "{name}");
    }
    assert!(
        s.runs().is_empty(),
        "no refused request reached the controller"
    );
    let kept = s.kept();
    assert!(
        !holds(&kept, SECRET) && !holds(&kept, &req.request_id),
        "nothing recorded"
    );
    let (status, response) = post(&s, &ok, &req).await;
    assert_eq!(status, 200);
    assert_eq!(response.unwrap().phase, CommandPhase::Completed as i32);
    assert_eq!(s.runs().len(), 1);
    let kept = s.kept();
    // The ledger the checks above read is where the gateway records (a positive control).
    assert!(
        holds(&kept, &req.request_id),
        "the accepted request is recorded"
    );
    assert!(!holds(&kept, SECRET), "the ledger holds no secret");
}

/// DEC-18 §1.3 Nr. 9 (verlorenes ACK): a credential command whose connection is gone before its
/// answer still completes — the operator's lookup by request id finds its receipt, a resend is
/// that receipt, and the controller ran once.
#[tokio::test]
async fn a_lost_acknowledgement_is_looked_up_and_the_command_never_runs_twice() {
    let s = spawn("lost", "1").await;
    let ok = allowed(&s);
    let req = credential_set(2, 4);
    let body = req.encode_to_vec();
    {
        // The request leaves; the connection is gone before any answer.
        let mut conn = TcpStream::connect(s.addr).await.unwrap();
        conn.write_all(post_head(&s, &ok, body.len()).as_bytes())
            .await
            .unwrap();
        conn.write_all(&body).await.unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while s.runs().is_empty() && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(s.runs().len(), 1, "the controller started");
    }
    // The command outlives its connection. (A lookup waits for the running command — the one
    // controller lane — so it answers the command's end: COMPLETED, or PENDING_RECONCILE for a
    // command cut off.)
    let token = ok.cookie.clone().unwrap();
    let found = lookup(&s, &token, &req.request_id).await;
    assert!(found.found);
    assert_eq!(
        found.phase,
        CommandPhase::Completed as i32,
        "the lost acknowledgement's command completed: {}",
        found.detail
    );
    assert_eq!(s.finished(), 1, "the controller was not cut off");
    assert!(!found.detail.contains(SECRET));
    // A resend of the same request is its receipt; the controller is not run again.
    let (status, response) = post(&s, &ok, &req).await;
    assert_eq!(status, 200);
    assert_eq!(response.unwrap().phase, CommandPhase::Completed as i32);
    assert_eq!(s.runs().len(), 1);
    assert!(!holds(&s.kept(), SECRET));
}

/// DEC-18 §1.3 Nr. 9 (konkurrierende Eingaben): concurrent provider commands reach the one
/// native controller one at a time — never overlapping, each with its own receipt — and the
/// same request sent twice at once runs once.
#[tokio::test]
async fn concurrent_provider_inputs_run_one_at_a_time_and_a_doubled_request_once() {
    let s = Arc::new(spawn("concurrent", "0.2").await);
    let ok = allowed(&s);
    let mut tasks = Vec::new();
    for n in 0..4u8 {
        let (s, ok) = (Arc::clone(&s), ok.clone());
        tasks.push(tokio::spawn(async move {
            post(&s, &ok, &credential_set(10 + n, 4)).await
        }));
    }
    // The same request — the same bytes — sent twice at once (a double submission).
    let doubled = credential_set(20, 4);
    for _ in 0..2 {
        let (s, ok, doubled) = (Arc::clone(&s), ok.clone(), doubled.clone());
        tasks.push(tokio::spawn(async move { post(&s, &ok, &doubled).await }));
    }
    for t in tasks {
        let (status, response) = t.await.unwrap();
        assert_eq!(status, 200);
        assert_eq!(response.unwrap().phase, CommandPhase::Completed as i32);
    }
    assert_eq!(s.overlaps(), 0, "one controller run at a time");
    let runs = s.runs();
    assert_eq!(
        runs.len(),
        5,
        "four commands and the doubled one once: {runs:?}"
    );
    for n in [10u8, 11, 12, 13, 20] {
        let id = format!("{n:08x}-1390-4390-8390-139013901390");
        assert_eq!(
            runs.iter().filter(|r| r.contains(&id)).count(),
            1,
            "{id} ran once"
        );
    }
}
