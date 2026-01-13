//! Game simulation modules

pub mod combat;
pub mod r#match;
pub mod physics;
pub mod snapshot;

pub use r#match::{GameMatch, MatchHandle, MatchRegistry, PlayerState};

use crate::ws::protocol::ClientMsg;
use uuid::Uuid;

/// Player input received from WebSocket
#[derive(Debug, Clone)]
pub struct PlayerInput {
    pub user_id: Uuid,
    pub msg: ClientMsg,
    pub received_at: u64,
}

/// Input state for a single tick (processed from ClientMsg::InputTick)
#[derive(Debug, Clone, Default)]
pub struct TickInput {
    pub seq: u32,
    pub throttle: f32,
    pub steer: f32,
    pub shoot: bool,
    pub aim_yaw: f32,
}
