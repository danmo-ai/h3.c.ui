use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use clap::Parser;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use h3_api::config::{default_home, AppConfig, Cli};
use h3_api::download::DownloadManager;
use h3_api::engine::Engine;
use h3_api::routes;
use h3_api::state::AppState;
use h3_api::tasks::TaskManager;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("h3_api=info".parse()?))
        .init();

    let cli = Cli::parse();
    let home = cli.home.unwrap_or_else(default_home);
    std::fs::create_dir_all(&home)?;
    std::fs::create_dir_all(home.join("uploads"))?;
    let config = AppConfig::load_or_default(&home);
    config.save(&home)?;

    let media = h3_api::ffmpeg::ensure_env();
    tracing::info!(
        ffmpeg = ?media.ffmpeg_path,
        ffprobe = ?media.ffprobe_path,
        source = ?media.source,
        "media tools"
    );
    if !media.ffmpeg || !media.ffprobe {
        tracing::warn!("FFmpeg/FFprobe incomplete — run `make ffmpeg` or install on PATH");
    }

    let engine = Arc::new(Engine::new());
    let tasks = TaskManager::new(
        engine.clone(),
        config.model_dir.clone(),
        config.output_dir.clone(),
        config.cache_enabled,
    );
    let state = AppState {
        home: home.clone(),
        config: Arc::new(parking_lot::Mutex::new(config)),
        engine,
        downloads: Arc::new(DownloadManager::new()),
        tasks,
    };

    let mut app: Router = routes::router(state).layer(
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any),
    ).layer(TraceLayer::new_for_http());

    if let Some(static_dir) = cli.static_dir {
        if static_dir.is_dir() {
            tracing::info!("serving UI from {}", static_dir.display());
            let index = static_dir.join("index.html");
            let serve = ServeDir::new(static_dir).not_found_service(ServeFile::new(index));
            app = app.fallback_service(serve);
        } else {
            tracing::warn!("static_dir missing: {}", static_dir.display());
        }
    }

    let addr = SocketAddr::new(cli.host.parse()?, cli.port);
    tracing::info!("h3-api listening on http://{addr}");
    tracing::info!("control plane: {}", home.display());
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
