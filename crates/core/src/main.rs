mod api;
mod db;
mod profiler;
mod state;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

#[derive(Parser)]
#[command(name = "thymos-core", about = "Thymos immune network core")]
struct Args {
    #[arg(long, default_value = "0.0.0.0:9443")]
    listen: String,

    #[arg(long, default_value = "data")]
    data_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("thymos_core=info,tower_http=info")
        .init();

    let args = Args::parse();
    let database = Arc::new(db::Db::open(&args.data_dir)?);
    let app_state = Arc::new(RwLock::new(state::AppState::load_from_db(&database)));

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

    let app = api::router(app_state.clone(), database.clone());
    let listener = tokio::net::TcpListener::bind(&args.listen).await?;
    info!(addr = %args.listen, data = %args.data_dir.display(), "thymos-core started");

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
