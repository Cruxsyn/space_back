//! HTTP route definitions

use axum::{
    extract::{Extension, State},
    http::{header, Method, StatusCode},
    middleware,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use tower_http::{
    compression::CompressionLayer,
    cors::CorsLayer,
    trace::TraceLayer,
};
use uuid::Uuid;

use crate::app::AppState;
use crate::http::middleware::{require_auth, AuthenticatedUser};
use crate::matchmaking::queue::QueuedPlayer;
use crate::payments::webhook::stripe_webhook_handler;
use crate::util::time::uptime_secs;
use crate::ws::handler::ws_handler;
use crate::ws::protocol::ShipType;

/// Build the application router
pub fn build_router(state: AppState) -> Router {
    // CORS configuration - support multiple origins (comma-separated in CLIENT_ORIGIN)
    let allowed_origins: Vec<header::HeaderValue> = state
        .config
        .client_origin
        .split(',')
        .filter_map(|s| s.trim().parse::<header::HeaderValue>().ok())
        .collect();
    
    let cors = CorsLayer::new()
        .allow_origin(allowed_origins)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
        .allow_credentials(true);

    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/health", get(health_handler))
        .route("/ws", get(ws_handler))
        .route("/payments/webhook", post(stripe_webhook_handler));

    // Protected routes (auth required)
    let protected_routes = Router::new()
        .route("/matchmaking/join", post(matchmaking_join_handler))
        .route("/payments/checkout", post(checkout_handler))
        .route("/inventory", get(inventory_handler))
        .route("/inventory/equip", post(equip_handler))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}

// ============================================================================
// Health endpoint
// ============================================================================

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    uptime_secs: u64,
    active_matches: usize,
    active_players: usize,
    queue_size: usize,
}

async fn health_handler(State(state): State<AppState>) -> Json<HealthResponse> {
    let queue_size = state.matchmaking.queue_size().await;

    Json(HealthResponse {
        status: "ok",
        uptime_secs: uptime_secs(),
        active_matches: state.match_registry.active_matches(),
        active_players: state.match_registry.total_players(),
        queue_size,
    })
}

// ============================================================================
// Matchmaking endpoints
// ============================================================================

#[derive(Deserialize)]
struct JoinMatchRequest {
    ship_type: ShipType,
}

#[derive(Serialize)]
struct JoinMatchResponse {
    status: &'static str,
    message: String,
    ws_url: String,
}

async fn matchmaking_join_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedUser>,
    Json(req): Json<JoinMatchRequest>,
) -> Result<Json<JoinMatchResponse>, AppError> {
    let player = QueuedPlayer::new(
        auth.user_id,
        format!("Player_{}", &auth.user_id.to_string()[..8]),
        req.ship_type,
    );

    state
        .matchmaking
        .join_queue(player)
        .await
        .map_err(|e| AppError::BadRequest(e))?;

    // Generate WebSocket URL with token
    // In production, you'd generate a short-lived token here
    let ws_url = format!("{}/ws", state.config.public_base_url.replace("https://", "wss://").replace("http://", "ws://"));

    Ok(Json(JoinMatchResponse {
        status: "queued",
        message: "Added to matchmaking queue".to_string(),
        ws_url,
    }))
}

// ============================================================================
// Payment endpoints
// ============================================================================

#[derive(Deserialize)]
struct CheckoutRequest {
    item_id: Uuid,
}

#[derive(Serialize)]
struct CheckoutResponse {
    session_id: String,
    url: String,
}

async fn checkout_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedUser>,
    Json(req): Json<CheckoutRequest>,
) -> Result<Json<CheckoutResponse>, AppError> {
    let response = state
        .stripe
        .create_checkout_session(auth.user_id, req.item_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(CheckoutResponse {
        session_id: response.session_id,
        url: response.url,
    }))
}

// ============================================================================
// Inventory endpoints
// ============================================================================

#[derive(Serialize)]
struct InventoryResponse {
    items: Vec<InventoryItem>,
}

#[derive(Serialize)]
struct InventoryItem {
    item_id: Uuid,
    name: String,
    item_type: String,
    owned: bool,
    equipped: bool,
}

async fn inventory_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedUser>,
) -> Result<Json<InventoryResponse>, AppError> {
    let items = state
        .inventory_store
        .get_user_inventory_with_details(auth.user_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let response_items: Vec<InventoryItem> = items
        .into_iter()
        .filter_map(|i| {
            i.item.map(|details| InventoryItem {
                item_id: i.item_id,
                name: details.name,
                item_type: details.item_type,
                owned: i.owned,
                equipped: i.equipped,
            })
        })
        .collect();

    Ok(Json(InventoryResponse {
        items: response_items,
    }))
}

#[derive(Deserialize)]
struct EquipRequest {
    item_id: Uuid,
}

#[derive(Serialize)]
struct EquipResponse {
    success: bool,
    message: String,
}

async fn equip_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthenticatedUser>,
    Json(req): Json<EquipRequest>,
) -> Result<Json<EquipResponse>, AppError> {
    // Check if user owns the item
    let owns = state
        .inventory_store
        .user_owns_item(auth.user_id, req.item_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if !owns {
        return Err(AppError::BadRequest("You don't own this item".to_string()));
    }

    state
        .inventory_store
        .equip_item(auth.user_id, req.item_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(EquipResponse {
        success: true,
        message: "Item equipped".to_string(),
    }))
}

// ============================================================================
// Error handling
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match &self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized".to_string()),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
        };

        let body = serde_json::json!({
            "error": message
        });

        (status, Json(body)).into_response()
    }
}
