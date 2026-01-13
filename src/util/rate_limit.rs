//! Rate limiting utilities

use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use std::num::NonZeroU32;
use std::sync::Arc;

/// Rate limiter type alias
pub type Limiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

/// Create a rate limiter with the specified requests per second
pub fn create_limiter(requests_per_second: u32) -> Arc<Limiter> {
    let quota = Quota::per_second(NonZeroU32::new(requests_per_second).unwrap_or(NonZeroU32::MIN));
    Arc::new(RateLimiter::direct(quota))
}

/// Input rate limiter for WebSocket messages (per player)
pub const INPUT_RATE_LIMIT: u32 = 30; // Max 30 input messages per second

/// Matchmaking join rate limit
pub const MATCHMAKING_RATE_LIMIT: u32 = 5; // Max 5 join attempts per second

/// Inventory API rate limit
pub const INVENTORY_RATE_LIMIT: u32 = 10; // Max 10 requests per second

/// Per-player rate limiter state
#[derive(Clone)]
pub struct PlayerRateLimiter {
    input_limiter: Arc<Limiter>,
}

impl PlayerRateLimiter {
    pub fn new() -> Self {
        Self {
            input_limiter: create_limiter(INPUT_RATE_LIMIT),
        }
    }

    /// Check if an input message is allowed (returns true if allowed)
    pub fn check_input(&self) -> bool {
        self.input_limiter.check().is_ok()
    }
}

impl Default for PlayerRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}
