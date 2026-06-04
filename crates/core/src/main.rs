mod alerting;
mod api;
mod auth;
mod dashboard;
mod db;
mod metrics;
mod profiler;
mod resolver;
mod state;

use anyhow::Result;
use axum::Router;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::services::ServeDir;
use tracing::info;

#[derive(Parser)]
#[command(name = "thymus-core", about = "Thymus immune network core")]
struct Args {
    #[arg(long, default_value = "0.0.0.0:9443")]
    listen: String,

    #[arg(long, default_value = "data")]
    data_dir: PathBuf,

    #[arg(long)]
    webhook: Option<String>,

    #[arg(long, default_value = "0.7")]
    webhook_min_score: f64,

    #[arg(long)]
    token: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("thymus_core=info,tower_http=info")
        .init();

    let args = Args::parse();
    let database = Arc::new(db::Db::open(&args.data_dir)?);
    let mut initial_state = state::AppState::load_from_db(&database);
    if let Some(url) = args.webhook {
        info!(url = %url, min_score = args.webhook_min_score, "webhook alerting enabled");
        initial_state.webhook = Some(alerting::WebhookConfig::new(url, args.webhook_min_score));
    }
    let app_state = Arc::new(RwLock::new(initial_state));

    let save_state = app_state.clone();
    let save_db = database.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let mut s = save_state.write().await;
            if s.should_save() {
                s.save_to_db(&save_db);
                info!("state persisted");
            }
        }
    });

    // Clonal selection: optimize memory cells periodically
    let clonal_state = app_state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            let mut s = clonal_state.write().await;
            s.run_clonal_selection();
        }
    });

    // Reverse-DNS: give passive (IP-keyed) devices friendly hostnames
    let resolve_state = app_state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let ips = resolve_state.read().await.unresolved_ips(50);
            if ips.is_empty() {
                continue;
            }
            let resolved = tokio::task::spawn_blocking(move || {
                ips.into_iter()
                    .map(|ip| {
                        let name = ip.parse().ok().and_then(resolver::reverse_lookup);
                        (ip, name)
                    })
                    .collect::<Vec<_>>()
            })
            .await
            .unwrap_or_default();
            resolve_state.write().await.apply_resolution(resolved);
        }
    });

    let static_dir = find_static_dir();

    let core_state = api::CoreState {
        app: app_state.clone(),
        db: database.clone(),
        token: args.token.clone(),
    };
    if args.token.is_some() {
        info!("token auth enabled");
    }

    let app = Router::new()
        .merge(api::router(core_state.clone()))
        .merge(dashboard::router().with_state(core_state.clone()))
        .nest_service("/static", ServeDir::new(static_dir))
        .layer(axum::middleware::from_fn_with_state(
            core_state,
            auth::require_auth,
        ));

    let listener = tokio::net::TcpListener::bind(&args.listen).await?;
    info!(addr = %args.listen, "thymus-core started");

    let shutdown_state = app_state.clone();
    let shutdown_db = database.clone();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            tokio::signal::ctrl_c().await.ok();
            info!("shutting down...");
            let mut s = shutdown_state.write().await;
            s.save_to_db(&shutdown_db);
            info!("state saved, goodbye");
        })
        .await?;

    Ok(())
}

fn find_static_dir() -> PathBuf {
    let candidates = [
        PathBuf::from("crates/core/static"),
        PathBuf::from("static"),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("static")))
            .unwrap_or_default(),
    ];

    for path in &candidates {
        if path.exists() {
            return path.clone();
        }
    }

    PathBuf::from("static")
}
