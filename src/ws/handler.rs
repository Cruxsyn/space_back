//! WebSocket upgrade handler

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    response::Response,
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::app::AppState;
use crate::game::PlayerInput;
use crate::http::middleware::verify_jwt;
use crate::util::rate_limit::PlayerRateLimiter;
use crate::util::time::unix_millis;
use crate::ws::protocol::{ClientMsg, ServerMsg};

/// Query parameters for WebSocket connection
#[derive(Debug, Deserialize)]
pub struct WsQuery {
    /// JWT token for authentication
    pub token: String,
}

/// WebSocket upgrade handler
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<WsQuery>,
    State(state): State<AppState>,
) -> Response {
    // Verify JWT token before upgrading
    match verify_jwt(&query.token, &state.config.supabase_jwt_secret) {
        Ok(claims) => {
            info!(user_id = %claims.sub, "WebSocket upgrade for authenticated user");
            ws.on_upgrade(move |socket| handle_socket(socket, claims.sub, state))
        }
        Err(e) => {
            error!(error = %e, "WebSocket auth failed");
            Response::builder()
                .status(401)
                .body("Unauthorized".into())
                .unwrap()
        }
    }
}

/// Handle the upgraded WebSocket connection
async fn handle_socket(socket: WebSocket, user_id: Uuid, state: AppState) {
    info!(user_id = %user_id, "New WebSocket connection");

    let (mut ws_sink, ws_stream) = socket.split();

    // Get user profile for display name
    let display_name = match state.profile_store.get_profile(user_id).await {
        Ok(Some(profile)) => profile.display_name.unwrap_or_else(|| "Unknown".to_string()),
        Ok(None) => {
            let name = format!("Player_{}", &user_id.to_string()[..8]);
            let _ = state.profile_store.create_profile(user_id, &name).await;
            name
        }
        Err(e) => {
            error!(user_id = %user_id, error = %e, "Failed to fetch profile");
            format!("Player_{}", &user_id.to_string()[..8])
        }
    };

    // Send welcome message
    let welcome = ServerMsg::Welcome {
        user_id,
        server_time: unix_millis(),
    };

    if let Err(e) = send_msg(&mut ws_sink, &welcome).await {
        error!(user_id = %user_id, error = %e, "Failed to send welcome");
        return;
    }

    // Register with matchmaking to get channels
    let (input_tx, snapshot_rx) = state.matchmaking.register_player(user_id).await;

    // Run the session with split read/write
    run_session(user_id, display_name, ws_sink, ws_stream, input_tx, snapshot_rx).await;

    // Cleanup on disconnect
    state.matchmaking.unregister_player(user_id).await;

    info!(user_id = %user_id, "WebSocket connection closed");
}

/// Run the WebSocket session with read/write split
async fn run_session(
    user_id: Uuid,
    display_name: String,
    mut ws_sink: futures::stream::SplitSink<WebSocket, Message>,
    mut ws_stream: futures::stream::SplitStream<WebSocket>,
    input_tx: mpsc::Sender<PlayerInput>,
    mut snapshot_rx: broadcast::Receiver<ServerMsg>,
) {
    let rate_limiter = PlayerRateLimiter::new();

    // Spawn writer task: broadcast snapshots -> WebSocket
    let writer_user_id = user_id;
    let writer_handle = tokio::spawn(async move {
        loop {
            match snapshot_rx.recv().await {
                Ok(msg) => {
                    if let Err(e) = send_msg(&mut ws_sink, &msg).await {
                        debug!(user_id = %writer_user_id, error = %e, "WebSocket send failed");
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(
                        user_id = %writer_user_id,
                        lagged_count = n,
                        "Client lagged, skipping {} snapshots", n
                    );
                    // Continue - don't disconnect for lag
                }
                Err(broadcast::error::RecvError::Closed) => {
                    debug!(user_id = %writer_user_id, "Snapshot channel closed");
                    break;
                }
            }
        }
    });

    // Reader loop: WebSocket -> match loop
    while let Some(result) = ws_stream.next().await {
        match result {
            Ok(Message::Text(text)) => {
                if !rate_limiter.check_input() {
                    warn!(user_id = %user_id, "Rate limited input message");
                    continue;
                }

                match serde_json::from_str::<ClientMsg>(&text) {
                    Ok(client_msg) => {
                        let input = PlayerInput {
                            user_id,
                            msg: client_msg,
                            received_at: unix_millis(),
                        };

                        if input_tx.send(input).await.is_err() {
                            debug!(user_id = %user_id, "Input channel closed");
                            break;
                        }
                    }
                    Err(e) => {
                        warn!(user_id = %user_id, error = %e, "Failed to parse client message");
                    }
                }
            }
            Ok(Message::Binary(_)) => {
                warn!(user_id = %user_id, "Received binary message, ignoring");
            }
            Ok(Message::Ping(_)) => {
                debug!(user_id = %user_id, "Received ping");
            }
            Ok(Message::Pong(_)) => {
                debug!(user_id = %user_id, "Received pong");
            }
            Ok(Message::Close(_)) => {
                info!(user_id = %user_id, "Client initiated close");
                break;
            }
            Err(e) => {
                error!(user_id = %user_id, error = %e, "WebSocket error");
                break;
            }
        }
    }

    // Signal disconnect to match loop
    let _ = input_tx
        .send(PlayerInput {
            user_id,
            msg: ClientMsg::LeaveMatch,
            received_at: unix_millis(),
        })
        .await;

    // Abort writer task
    writer_handle.abort();

    let _ = display_name; // Used for logging context
}

/// Send a message over WebSocket
async fn send_msg(
    sink: &mut futures::stream::SplitSink<WebSocket, Message>,
    msg: &ServerMsg,
) -> Result<(), String> {
    let json = serde_json::to_string(msg).map_err(|e| e.to_string())?;
    sink.send(Message::Text(json))
        .await
        .map_err(|e| e.to_string())
}
