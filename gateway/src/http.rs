use std::{sync::{Arc,atomic::{AtomicUsize,Ordering}},time::{Instant,Duration}};
use axum::{body::Body,extract::{Request,State},http::{HeaderMap,HeaderValue,StatusCode,header::{CACHE_CONTROL,CONTENT_SECURITY_POLICY,REFERRER_POLICY,SET_COOKIE,X_CONTENT_TYPE_OPTIONS,X_FRAME_OPTIONS}},middleware::{self,Next},response::{Response,IntoResponse},routing::get,Router};
use tower::ServiceExt;
use tower_http::{services::{ServeDir,ServeFile},set_header::SetResponseHeaderLayer,trace::TraceLayer};
use crate::{config::GatewayConfig,engine::SourceRegistry,hub::StateHub,session::{self,SessionStore},ws};
pub struct AppState {pub cfg:GatewayConfig,pub hub:Arc<StateHub>,pub registry:Arc<SourceRegistry>,pub sessions:SessionStore,pub clients:AtomicUsize,pub started_at:Instant}
impl AppState {pub fn new(cfg:GatewayConfig,hub:Arc<StateHub>,registry:Arc<SourceRegistry>)->Arc<Self> {Arc::new(Self{cfg,hub,registry,sessions:SessionStore::new(64,Duration::from_secs(12*3600)),clients:AtomicUsize::new(0),started_at:Instant::now()})}}
pub fn router(state:Arc<AppState>)->Router {
 Router::new().route("/ws/v1/gateway",get(ws::upgrade)).route("/api/v1/session",get(session_bootstrap)).route("/healthz",get(healthz)).fallback(spa)
 .layer(middleware::from_fn_with_state(state.clone(),enforce_host))
 .layer(SetResponseHeaderLayer::if_not_present(CONTENT_SECURITY_POLICY,HeaderValue::from_static("default-src 'self'; connect-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self'; font-src 'self' data:; frame-ancestors 'none'; base-uri 'none'; form-action 'self'")))
 .layer(SetResponseHeaderLayer::if_not_present(X_CONTENT_TYPE_OPTIONS,HeaderValue::from_static("nosniff")))
 .layer(SetResponseHeaderLayer::if_not_present(X_FRAME_OPTIONS,HeaderValue::from_static("DENY")))
 .layer(SetResponseHeaderLayer::if_not_present(REFERRER_POLICY,HeaderValue::from_static("no-referrer")))
 .layer(TraceLayer::new_for_http()).with_state(state)
}
async fn enforce_host(State(app):State<Arc<AppState>>,req:Request,next:Next)->Response {
 if let Err(e)=session::check_host(req.headers(),&app.cfg){return(StatusCode::FORBIDDEN,e).into_response()}next.run(req).await
}
async fn healthz(State(app):State<Arc<AppState>>)->Response {
 let sources:Vec<String>=app.registry.snapshot().into_iter().map(|(p,s)|format!("{p:?}={:?}",s.state)).collect();
 (StatusCode::OK,[(CACHE_CONTROL,"no-store")],format!("ok uptime_s={} clients={} sessions={} sources=[{}]",app.started_at.elapsed().as_secs(),app.clients.load(Ordering::Relaxed),app.sessions.len(),sources.join(","))).into_response()
}
async fn session_bootstrap(State(app):State<Arc<AppState>>,headers:HeaderMap)->Response {
 if let Err(e)=session::check_fetch_site(&headers){return(StatusCode::FORBIDDEN,e).into_response()}
 let mut r=StatusCode::NO_CONTENT.into_response();r.headers_mut().insert(CACHE_CONTROL,HeaderValue::from_static("no-store"));
 if !session::has_valid_session(&headers,&app.cfg,&app.sessions){let t=app.sessions.issue();r.headers_mut().append(SET_COOKIE,session::set_cookie(&app.cfg.session_cookie,&t));}r
}
async fn spa(State(app):State<Arc<AppState>>,headers:HeaderMap,req:Request)->Response {
 let Some(dir)=app.cfg.static_dir.clone() else{return(StatusCode::NOT_FOUND,"static dir not configured").into_response()};
 // Do not serve index.html for nonexistent asset paths.
 let svc=ServeDir::new(&dir).not_found_service(ServeFile::new(dir.join("index.html")));
 let mut res=match svc.oneshot(req).await {Ok(r)=>r.map(Body::new),Err(e)=>match e{}};
 if !session::has_valid_session(&headers,&app.cfg,&app.sessions){let t=app.sessions.issue();res.headers_mut().append(SET_COOKIE,session::set_cookie(&app.cfg.session_cookie,&t));}
 res
}
