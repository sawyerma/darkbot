use crate::{
    config::GatewayConfig,
    engine::SourceRegistry,
    hub::StateHub,
    session::{self, SessionStore},
    ws,
};
use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Request, State},
    http::{
        header::{
            CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, REFERRER_POLICY, SET_COOKIE,
            X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
        },
        HeaderMap, HeaderValue, StatusCode,
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tower::ServiceExt;
use tower_http::{
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};
pub struct AppState {
    pub cfg: GatewayConfig,
    pub control: crate::control::Control,
    pub hub: Arc<StateHub>,
    pub registry: Arc<SourceRegistry>,
    pub sessions: SessionStore,
    pub clients: AtomicUsize,
    pub started_at: Instant,
}
impl AppState {
    pub fn new(cfg: GatewayConfig, hub: Arc<StateHub>, registry: Arc<SourceRegistry>) -> Arc<Self> {
        Arc::new(Self {
            cfg,
            control: crate::control::Control::from_env(),
            hub,
            registry,
            sessions: SessionStore::new(64, Duration::from_secs(12 * 3600)),
            clients: AtomicUsize::new(0),
            started_at: Instant::now(),
        })
    }
}
pub fn router(state: Arc<AppState>) -> Router {
    Router::new().route("/ws/v1/gateway",get(ws::upgrade)).route("/api/v1/session",get(session_bootstrap)).route("/api/v1/command",get(crate::control::lookup).post(crate::control::command).layer(DefaultBodyLimit::max(4096)))
 .route("/api/v1/project0/process",get(crate::control::process))
 .route("/api/v1/logs",get(crate::control::logs))
 .route("/healthz",get(healthz)).fallback(spa)
 .layer(middleware::from_fn_with_state(state.clone(),enforce_host))
 .layer(SetResponseHeaderLayer::if_not_present(CONTENT_SECURITY_POLICY,HeaderValue::from_static("default-src 'self'; connect-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self'; font-src 'self' data:; frame-ancestors 'none'; base-uri 'none'; form-action 'self'")))
 .layer(SetResponseHeaderLayer::if_not_present(X_CONTENT_TYPE_OPTIONS,HeaderValue::from_static("nosniff")))
 .layer(SetResponseHeaderLayer::if_not_present(X_FRAME_OPTIONS,HeaderValue::from_static("DENY")))
 .layer(SetResponseHeaderLayer::if_not_present(REFERRER_POLICY,HeaderValue::from_static("no-referrer")))
 .layer(TraceLayer::new_for_http()).with_state(state)
}
async fn enforce_host(State(app): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    if let Err(e) = session::check_host(req.headers(), &app.cfg) {
        return (StatusCode::FORBIDDEN, e).into_response();
    }
    next.run(req).await
}
async fn healthz(State(app): State<Arc<AppState>>) -> Response {
    let sources: Vec<String> = app
        .registry
        .snapshot()
        .into_iter()
        .map(|(p, s)| format!("{p:?}={:?}", s.state))
        .collect();
    (
        StatusCode::OK,
        [(CACHE_CONTROL, "no-store")],
        format!(
            "ok uptime_s={} clients={} sessions={} sources=[{}]",
            app.started_at.elapsed().as_secs(),
            app.clients.load(Ordering::Relaxed),
            app.sessions.len(),
            sources.join(",")
        ),
    )
        .into_response()
}
async fn session_bootstrap(State(app): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if let Err(e) = session::check_fetch_site(&headers) {
        return (StatusCode::FORBIDDEN, e).into_response();
    }
    let mut r = StatusCode::NO_CONTENT.into_response();
    r.headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    let existing = session::cookie_value(&headers, &app.cfg.session_cookie)
        .filter(|t| app.sessions.validate(t));
    let token = existing.unwrap_or_else(|| {
        let t = app.sessions.issue();
        r.headers_mut()
            .append(SET_COOKIE, session::set_cookie(&app.cfg.session_cookie, &t));
        t
    });
    if let Some(csrf) = app
        .sessions
        .csrf_of(&token)
        .and_then(|s| HeaderValue::from_str(&s).ok())
    {
        r.headers_mut().insert("x-csrf-token", csrf);
    }
    r
}
async fn spa(State(app): State<Arc<AppState>>, headers: HeaderMap, req: Request) -> Response {
    let Some(dir) = app.cfg.static_dir.clone() else {
        return (StatusCode::NOT_FOUND, "static dir not configured").into_response();
    };
    let path = req.uri().path().to_owned();
    // Do not serve index.html for nonexistent asset paths.
    let svc = ServeDir::new(&dir).not_found_service(ServeFile::new(dir.join("index.html")));
    let mut res = match svc.oneshot(req).await {
        Ok(r) => r.map(Body::new),
        Err(e) => match e {},
    };
    let policy = cache_policy(&path, &res);
    res.headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static(policy));
    if !session::has_valid_session(&headers, &app.cfg, &app.sessions) {
        let t = app.sessions.issue();
        res.headers_mut()
            .append(SET_COOKIE, session::set_cookie(&app.cfg.session_cookie, &t));
    }
    res
}

/// Content-hashed Vite assets never change and may be cached forever. Everything else, above all
/// index.html (it names the bundle and with it the schema id), is revalidated on every load.
fn cache_policy(path: &str, res: &Response) -> &'static str {
    let html = res
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("text/html"));
    if path.starts_with("/assets/") && res.status() == StatusCode::OK && !html {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    fn res(status: StatusCode, ct: &str) -> Response {
        let mut r = Response::new(Body::empty());
        *r.status_mut() = status;
        r.headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_str(ct).unwrap());
        r
    }
    #[test]
    fn index_is_revalidated_and_hashed_assets_are_immutable() {
        assert_eq!(
            cache_policy("/", &res(StatusCode::OK, "text/html")),
            "no-cache"
        );
        assert_eq!(
            cache_policy(
                "/project-0",
                &res(StatusCode::OK, "text/html; charset=utf-8")
            ),
            "no-cache"
        );
        assert_eq!(
            cache_policy(
                "/assets/index-abc.js",
                &res(StatusCode::OK, "text/javascript")
            ),
            "public, max-age=31536000, immutable"
        );
        // A vanished old asset falls back to index.html: never cache that under the asset URL.
        assert_eq!(
            cache_policy("/assets/old.js", &res(StatusCode::OK, "text/html")),
            "no-cache"
        );
    }
}
