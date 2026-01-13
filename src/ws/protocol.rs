//! WebSocket protocol message definitions
//! These are the wire types for client-server communication

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Ship types available in the game
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShipType {
    /// Fast but fragile
    Scout,
    /// Balanced stats
    Fighter,
    /// Slow but tanky
    Cruiser,
    /// High damage, low mobility
    Destroyer,
}

impl Default for ShipType {
    fn default() -> Self {
        Self::Fighter
    }
}

/// Messages sent from client to server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    /// Request to join a match
    JoinMatch {
        /// Optional specific match ID, otherwise matchmaking assigns one
        match_id: Option<Uuid>,
        /// Ship type selection
        ship_type: ShipType,
    },

    /// Player input for current tick
    InputTick {
        /// Sequence number for client-side prediction reconciliation
        seq: u32,
        /// Throttle input (-1.0 = full reverse, 1.0 = full forward)
        throttle: f32,
        /// Steering input (-1.0 = full left, 1.0 = full right)
        steer: f32,
        /// Fire weapon this tick
        shoot: bool,
        /// Aim direction in radians
        aim_yaw: f32,
    },

    /// Ping for latency measurement
    Ping {
        /// Client timestamp
        t: u64,
    },

    /// Leave current match
    LeaveMatch,
}

/// Messages sent from server to client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    /// Welcome message after connection
    Welcome {
        user_id: Uuid,
        server_time: u64,
    },

    /// Confirmation of match join
    MatchJoined {
        match_id: Uuid,
        /// Seed for deterministic random generation
        seed: u64,
        /// All players in the match at join time
        players: Vec<PlayerInfo>,
    },

    /// Player joined the match
    PlayerJoined {
        player: PlayerInfo,
    },

    /// Player left the match
    PlayerLeft {
        user_id: Uuid,
        reason: String,
    },

    /// Game state snapshot (sent at regular intervals)
    Snapshot {
        /// Server tick number
        tick: u64,
        /// Current zone state
        zone: ZoneState,
        /// All player states
        players: Vec<PlayerSnapshot>,
        /// Events that occurred since last snapshot
        events: Vec<GameEvent>,
    },

    /// Match countdown starting
    MatchCountdown {
        seconds_remaining: u32,
    },

    /// Match has started
    MatchStarted {
        tick: u64,
    },

    /// Match has ended
    MatchEnd {
        winner_user_id: Option<Uuid>,
        /// Match statistics
        stats: MatchStats,
    },

    /// Error message
    Error {
        code: String,
        message: String,
    },

    /// Pong response
    Pong {
        /// Echo back client timestamp
        t: u64,
    },
}

/// Player info for lobby/join
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerInfo {
    pub user_id: Uuid,
    pub display_name: String,
    pub ship_type: ShipType,
    /// Equipped flag skin ID (if any)
    pub flag_skin_id: Option<Uuid>,
}

/// Zone (shrinking play area) state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneState {
    /// Current zone center X
    pub center_x: f32,
    /// Current zone center Y  
    pub center_y: f32,
    /// Current zone radius
    pub radius: f32,
    /// Target zone center X (shrinking towards)
    pub target_center_x: f32,
    /// Target zone center Y
    pub target_center_y: f32,
    /// Target radius
    pub target_radius: f32,
    /// Damage per second outside zone
    pub damage_per_second: f32,
    /// Seconds until zone starts shrinking
    pub shrink_delay: f32,
    /// Current shrink phase (0 = initial, increases each shrink)
    pub phase: u32,
}

impl Default for ZoneState {
    fn default() -> Self {
        Self {
            center_x: 0.0,
            center_y: 0.0,
            radius: 1000.0,
            target_center_x: 0.0,
            target_center_y: 0.0,
            target_radius: 1000.0,
            damage_per_second: 5.0,
            shrink_delay: 60.0,
            phase: 0,
        }
    }
}

/// Player state in a snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerSnapshot {
    pub user_id: Uuid,
    /// Position X
    pub x: f32,
    /// Position Y
    pub y: f32,
    /// Rotation in radians
    pub rotation: f32,
    /// Current velocity X
    pub vel_x: f32,
    /// Current velocity Y
    pub vel_y: f32,
    /// Health (0-100)
    pub health: f32,
    /// Is player alive
    pub alive: bool,
    /// Last processed input sequence
    pub last_input_seq: u32,
    /// Weapon cooldown remaining (0 = can fire)
    pub weapon_cooldown: f32,
}

/// Game events (damage, kills, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum GameEvent {
    /// Projectile fired
    Shot {
        shooter_id: Uuid,
        projectile_id: Uuid,
        x: f32,
        y: f32,
        direction: f32,
        speed: f32,
    },
    
    /// Hit registered
    Hit {
        shooter_id: Uuid,
        target_id: Uuid,
        damage: f32,
        x: f32,
        y: f32,
    },

    /// Player killed
    Kill {
        killer_id: Option<Uuid>,
        victim_id: Uuid,
        /// "shot", "zone", "collision"
        cause: String,
    },

    /// Zone damage tick
    ZoneDamage {
        user_id: Uuid,
        damage: f32,
    },

    /// Zone phase change
    ZoneShrink {
        phase: u32,
        new_center_x: f32,
        new_center_y: f32,
        new_radius: f32,
    },
}

/// Match statistics at end
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchStats {
    pub duration_secs: u32,
    pub total_players: u32,
    pub player_stats: Vec<PlayerMatchStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerMatchStats {
    pub user_id: Uuid,
    pub kills: u32,
    pub damage_dealt: f32,
    pub damage_taken: f32,
    pub shots_fired: u32,
    pub shots_hit: u32,
    pub placement: u32,
    pub alive_time_secs: u32,
}
