//! Snapshot building and compression

use std::collections::HashMap;
use uuid::Uuid;

use crate::ws::protocol::{GameEvent, PlayerSnapshot, ServerMsg, ZoneState};

use super::PlayerState;

/// Builds snapshots for network transmission
pub struct SnapshotBuilder {
    /// Tick counter since last snapshot
    ticks_since_snapshot: u32,
    /// Snapshot interval in ticks
    snapshot_interval: u32,
    /// Last snapshot for delta calculation (future use)
    _last_snapshot: Option<SnapshotData>,
}

#[derive(Debug, Clone)]
struct SnapshotData {
    tick: u64,
    players: Vec<PlayerSnapshot>,
}

impl SnapshotBuilder {
    pub fn new(snapshot_interval: u32) -> Self {
        Self {
            ticks_since_snapshot: 0,
            snapshot_interval,
            _last_snapshot: None,
        }
    }

    /// Check if it's time to send a snapshot
    pub fn should_send(&mut self) -> bool {
        self.ticks_since_snapshot += 1;
        if self.ticks_since_snapshot >= self.snapshot_interval {
            self.ticks_since_snapshot = 0;
            true
        } else {
            false
        }
    }

    /// Force snapshot on next check (used for important events)
    pub fn force_next(&mut self) {
        self.ticks_since_snapshot = self.snapshot_interval;
    }

    /// Build a snapshot message
    pub fn build(
        &mut self,
        tick: u64,
        zone: &ZoneState,
        players: &HashMap<Uuid, PlayerState>,
        events: Vec<GameEvent>,
    ) -> ServerMsg {
        let player_snapshots: Vec<PlayerSnapshot> = players
            .values()
            .map(|p| PlayerSnapshot {
                user_id: p.user_id,
                x: p.x,
                y: p.y,
                rotation: p.rotation,
                vel_x: p.vel_x,
                vel_y: p.vel_y,
                health: p.health,
                alive: p.alive,
                last_input_seq: p.last_input_seq,
                weapon_cooldown: p.weapon_cooldown,
            })
            .collect();

        // Store for delta calculation (future optimization)
        self._last_snapshot = Some(SnapshotData {
            tick,
            players: player_snapshots.clone(),
        });

        ServerMsg::Snapshot {
            tick,
            zone: zone.clone(),
            players: player_snapshots,
            events,
        }
    }

    /// Build a minimal snapshot with only changed players (future optimization)
    #[allow(dead_code)]
    pub fn build_delta(
        &self,
        _tick: u64,
        _zone: &ZoneState,
        _players: &HashMap<Uuid, PlayerState>,
        _events: Vec<GameEvent>,
    ) -> ServerMsg {
        // TODO: Implement delta compression
        // For now, always send full snapshots
        unimplemented!("Delta snapshots not yet implemented")
    }
}

/// Snapshot compression stats for debugging
#[derive(Debug, Default)]
pub struct SnapshotStats {
    pub total_snapshots: u64,
    pub total_bytes: u64,
    pub avg_players_per_snapshot: f32,
}

impl SnapshotStats {
    pub fn record(&mut self, player_count: usize, bytes: usize) {
        self.total_snapshots += 1;
        self.total_bytes += bytes as u64;
        
        // Running average
        let n = self.total_snapshots as f32;
        self.avg_players_per_snapshot = 
            self.avg_players_per_snapshot * ((n - 1.0) / n) + (player_count as f32 / n);
    }
}
