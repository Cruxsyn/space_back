//! Ship Game Server - Authoritative multiplayer game server
//!
//! This is the main entry point for the game server. It handles:
//! - WebSocket connections for real-time gameplay
//! - HTTP endpoints for matchmaking, inventory, and payments
//! - Stripe webhook processing
//! - Supabase integration for user data

mod app;
mod config;
mod game;
mod http;
mod matchmaking;
mod payments;
mod store;
mod util;
mod ws;

use std::net::SocketAddr;

use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::app::AppState;
use crate::config::Config;
use crate::http::build_router;
use crate::util::time::init_server_time;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load environment variables
    dotenvy::dotenv().ok();

    // Load configuration
    let config = Config::from_env()?;

    // Initialize tracing
    init_tracing(&config.log_level);

    // Initialize server time tracking
    init_server_time();

    info!("Starting Ship Game Server");
    info!("Server address: {}", config.server_addr);

    // Create application state
    let state = AppState::new(config.clone());

    // Spawn matchmaking service
    let matchmaking = state.matchmaking.clone();
    tokio::spawn(async move {
        matchmaking.run().await;
    });

    // Build router
    let router = build_router(state);

    // Start server
    let addr: SocketAddr = config.server_addr;
    let listener = TcpListener::bind(addr).await?;

    info!("Server listening on {}", addr);
    info!("Health check: http://{}/health", addr);
    info!("WebSocket endpoint: ws://{}/ws", addr);

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("Server shutdown complete");
    Ok(())
}

/// Initialize tracing/logging
fn init_tracing(log_level: &str) {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().with_target(true))
        .init();
}

/// Graceful shutdown signal handler
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, starting graceful shutdown");
        }
        _ = terminate => {
            info!("Received terminate signal, starting graceful shutdown");
        }
    }
}
