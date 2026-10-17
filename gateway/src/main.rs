use darkbot_gateway::{
    config::GatewayConfig,
    engine::{EngineFeed, SourceRegistry},
    http::{self, AppState},
    hub::StateHub,
    overview,
};
use std::sync::Arc;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;
fn engine_feeds(_cfg: &GatewayConfig) -> Vec<Arc<dyn EngineFeed>> {
    Vec::new()
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,tower_http=warn")),
        )
        .init();
    let cfg = GatewayConfig::from_env()?;
    info!(bind=%cfg.bind,static_dir=?cfg.static_dir,"gateway configuration");
    if cfg.static_dir.is_none() {
        warn!("no SPA static dir; use Vite proxy");
    }
    let hub = Arc::new(StateHub::new(cfg.limits.max_state_full_bytes as usize));
    let registry = SourceRegistry::new();
    for feed in engine_feeds(&cfg) {
        darkbot_gateway::engine::spawn_supervisor(feed, hub.clone(), registry.clone(), &cfg);
    }
    let generation = overview::gateway_generation()?;
    overview::spawn_publisher(hub.clone(), registry.clone(), &cfg, generation);
    let app = http::router(AppState::new(cfg.clone(), hub, registry));
    let listener = tokio::net::TcpListener::bind(cfg.bind).await?;
    info!(bind=%cfg.bind,"gateway listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
