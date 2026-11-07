//! BSC-V8 `POST /api/v1/bsc/query` over the real HTTP stack: a bounded read-only query needs the
//! session, the allowed origin, its CSRF token and a protobuf `BscQueryRequest` within the
//! client frame bound before the deployed reporter (a stand-in at `bsc-status-reporter`'s place
//! next to the controller) runs; the answer is the reporter's bytes.
use darkbot_gateway::{
    config::{GatewayConfig, Limits},
    control::{Control, Paths},
    engine::SourceRegistry,
    http::{self, AppState},
    hub::StateHub,
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

struct Server {
    addr: SocketAddr,
    state: Arc<AppState>,
    /// The stand-ins' directory (the controller's, hence the reporter's).
    probe: PathBuf,
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

/// A gateway on loopback; its controller and launchctl are stand-ins that are never asked here.
async fn spawn(tag: &str) -> Server {
    let base = std::env::temp_dir().join(format!("gw-bsc-query-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (probe, gateway) = (base.join("probe"), base.join("gateway"));
    std::fs::create_dir_all(&probe).unwrap();
    std::fs::create_dir_all(gateway.join("run")).unwrap();
    let ctl = probe.join("darkbotctl");
    executable(
        &ctl,
        "#!/bin/sh
exit 9
",
    );
    let launchctl = probe.join("launchctl");
    executable(
        &launchctl,
        "#!/bin/sh
exit 113
",
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
    Server { addr, state, probe }
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

/// The headers of a query POST; each guard can be left out or falsified.
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
        "POST /api/v1/bsc/query HTTP/1.1\r\nHost: {}\r\nContent-Type: {}\r\nContent-Length: {length}\r\nConnection: close\r\n",
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
    let token = s.state.sessions.issue();
    let csrf = s.state.sessions.csrf_of(&token).unwrap();
    Post {
        cookie: Some(token),
        origin: Some(format!("http://{}", s.addr)),
        fetch_site: Some("same-origin"),
        csrf: Some(csrf),
        content_type: "application/x-protobuf",
    }
}

/// BSC-V8: `POST /api/v1/bsc/query` over the real HTTP stack needs the session, the allowed
/// origin, its CSRF token and a protobuf `BscQueryRequest` within the client frame bound before
/// the reporter runs;
/// the answer is the reporter's `BscQueryResponse` as it wrote it.
#[tokio::test]
async fn a_bsc_query_needs_a_session_and_protobuf_and_is_the_reporters_answer() {
    use darkbot_gateway::proto::bsc::v1::{
        bsc_query_request::Query, Availability, BscQueryRequest, BscQueryResponse,
        StrategyCSettingsQuery,
    };
    let s = spawn("query").await;
    let answer = BscQueryResponse {
        request_id: "q1".into(),
        availability: Availability::Available as i32,
        ..Default::default()
    }
    .encode_to_vec();
    std::fs::write(s.probe.join("answer.bin"), &answer).unwrap();
    executable(
        &s.probe.join("bsc-status-reporter"),
        &format!(
            "#!/bin/sh\necho \"$*\" >> '{p}/queries'\ncat > /dev/null\ncat '{p}/answer.bin'\n",
            p = s.probe.display()
        ),
    );
    let request = BscQueryRequest {
        request_id: "q1".into(),
        query: Some(Query::StrategySettings(StrategyCSettingsQuery {
            include_features: false,
        })),
    }
    .encode_to_vec();
    let head = |p: &Post, length: usize| post_head(&s, p, length);
    let ask = |p: Post, body: Vec<u8>| {
        let head = head(&p, body.len());
        async move { exchange(s.addr, &head, &body).await }
    };
    let ok = allowed(&s);
    let refused = [
        (
            Post {
                cookie: None,
                ..ok.clone()
            },
            request.clone(),
        ),
        (
            Post {
                origin: Some("http://evil.invalid".into()),
                ..ok.clone()
            },
            request.clone(),
        ),
        (
            Post {
                origin: None,
                ..ok.clone()
            },
            request.clone(),
        ),
        (
            Post {
                csrf: None,
                ..ok.clone()
            },
            request.clone(),
        ),
        (
            Post {
                fetch_site: Some("cross-site"),
                ..ok.clone()
            },
            request.clone(),
        ),
        (
            Post {
                csrf: Some("not-the-token".into()),
                ..ok.clone()
            },
            request.clone(),
        ),
        (
            Post {
                content_type: "application/json",
                ..ok.clone()
            },
            request.clone(),
        ),
        (ok.clone(), vec![0xff, 0xff]),
        (
            ok.clone(),
            vec![0; s.state.cfg.limits.max_client_frame_bytes + 1],
        ),
    ];
    for (p, body) in refused {
        let (status, _) = ask(p, body).await;
        assert_ne!(status, 200);
    }
    assert!(!s.probe.join("queries").exists(), "the reporter never ran");
    let (status, body) = ask(ok, request).await;
    assert_eq!(status, 200);
    assert_eq!(body, answer);
    assert_eq!(
        std::fs::read_to_string(s.probe.join("queries")).unwrap(),
        "--query\n"
    );
}
