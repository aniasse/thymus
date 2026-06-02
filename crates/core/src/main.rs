mod api;
mod profiler;
mod state;

use anyhow::Result;
use clap::Parser;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

#[derive(Parser)]
#[command(name = "thymos-core", about = "Thymos immune network core")]
struct Args {
    #[arg(long, default_value = "0.0.0.0:9443")]
    listen: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("thymos_core=info,tower_http=info")
        .init();

    let args = Args::parse();
    let app_state = Arc::new(RwLock::new(state::AppState::new()));
    let app = api::router(app_state);

    let listener = tokio::net::TcpListener::bind(&args.listen).await?;
    info!(addr = %args.listen, "thymos-core started");

    axum::serve(listener, app).await?;

    Ok(())
}
