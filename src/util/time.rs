//! Time utilities for game simulation

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Get current Unix timestamp in milliseconds
pub fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64
}

/// Get current Unix timestamp in microseconds (for high precision)
pub fn unix_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_micros() as u64
}

/// Server start time for uptime tracking
static SERVER_START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// Initialize server start time (call once at startup)
pub fn init_server_time() {
    SERVER_START.get_or_init(Instant::now);
}

/// Get server uptime in seconds
pub fn uptime_secs() -> u64 {
    SERVER_START
        .get()
        .map(|start| start.elapsed().as_secs())
        .unwrap_or(0)
}

/// Tick rate configuration
pub const SIMULATION_TPS: u32 = 30; // 30 ticks per second
pub const SNAPSHOT_TPS: u32 = 20; // 20 snapshots per second
pub const TICK_DURATION_MICROS: u64 = 1_000_000 / SIMULATION_TPS as u64;
pub const SNAPSHOT_INTERVAL_MICROS: u64 = 1_000_000 / SNAPSHOT_TPS as u64;

/// Calculate delta time for physics (in seconds)
pub fn tick_delta() -> f32 {
    1.0 / SIMULATION_TPS as f32
}

/// A simple timer for measuring durations
#[derive(Debug, Clone)]
pub struct Timer {
    start: Instant,
}

impl Timer {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }

    pub fn elapsed_micros(&self) -> u64 {
        self.start.elapsed().as_micros() as u64
    }

    pub fn reset(&mut self) {
        self.start = Instant::now();
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}
