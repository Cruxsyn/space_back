//! Match state and authoritative tick loop

use dashmap::DashMap;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use tokio::time::interval;
use tracing::{info, warn};
use uuid::Uuid;

use crate::util::time::{tick_delta, unix_millis, SIMULATION_TPS, SNAPSHOT_TPS};
use crate::ws::protocol::{
    ClientMsg, GameEvent, MatchStats, PlayerInfo, PlayerMatchStats, ServerMsg, ShipType, ZoneState,
};

use super::combat::{CombatSystem, HitResult, Projectile, WeaponStats};
use super::physics::{PhysicsSystem, ShipStats};
use super::snapshot::SnapshotBuilder;
use super::{PlayerInput, TickInput};

/// Match phase
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchPhase {
    /// Waiting for players
    Waiting,
    /// Countdown before start
    Countdown,
    /// Match in progress
    InProgress,
    /// Match ended
    Ended,
}

/// Player state in a match (authoritative)
#[derive(Debug, Clone)]
pub struct PlayerState {
    pub user_id: Uuid,
    pub display_name: String,
    pub ship_type: ShipType,
    pub flag_skin_id: Option<Uuid>,

    // Position and movement
    pub x: f32,
    pub y: f32,
    pub rotation: f32,
    pub vel_x: f32,
    pub vel_y: f32,

    // Combat
    pub health: f32,
    pub alive: bool,
    pub weapon_cooldown: f32,

    // Input tracking
    pub last_input_seq: u32,
    pub current_input: TickInput,

    // Stats
    pub kills: u32,
    pub damage_dealt: f32,
    pub damage_taken: f32,
    pub shots_fired: u32,
    pub shots_hit: u32,
    pub spawn_time: u64,
    pub death_time: Option<u64>,
}

impl PlayerState {
    pub fn new(
        user_id: Uuid,
        display_name: String,
        ship_type: ShipType,
        flag_skin_id: Option<Uuid>,
        spawn_x: f32,
        spawn_y: f32,
        spawn_rotation: f32,
    ) -> Self {
        let stats = ShipStats::for_type(ship_type);
        Self {
            user_id,
            display_name,
            ship_type,
            flag_skin_id,
            x: spawn_x,
            y: spawn_y,
            rotation: spawn_rotation,
            vel_x: 0.0,
            vel_y: 0.0,
            health: stats.max_health,
            alive: true,
            weapon_cooldown: 0.0,
            last_input_seq: 0,
            current_input: TickInput::default(),
            kills: 0,
            damage_dealt: 0.0,
            damage_taken: 0.0,
            shots_fired: 0,
            shots_hit: 0,
            spawn_time: unix_millis(),
            death_time: None,
        }
    }
}

/// Zone configuration for battle royale shrinking
#[derive(Debug, Clone)]
pub struct ZoneConfig {
    /// Initial zone radius
    pub initial_radius: f32,
    /// Time before first shrink (seconds)
    pub initial_delay: f32,
    /// Shrink phases configuration
    pub phases: Vec<ZonePhase>,
}

#[derive(Debug, Clone)]
pub struct ZonePhase {
    /// Target radius for this phase
    pub target_radius: f32,
    /// Time to shrink to target (seconds)
    pub shrink_duration: f32,
    /// Damage per second outside zone
    pub damage_per_second: f32,
    /// Delay before next phase starts
    pub delay_after: f32,
}

impl Default for ZoneConfig {
    fn default() -> Self {
        Self {
            initial_radius: 1500.0,
            initial_delay: 60.0,
            phases: vec![
                ZonePhase {
                    target_radius: 1000.0,
                    shrink_duration: 30.0,
                    damage_per_second: 5.0,
                    delay_after: 45.0,
                },
                ZonePhase {
                    target_radius: 600.0,
                    shrink_duration: 25.0,
                    damage_per_second: 10.0,
                    delay_after: 30.0,
                },
                ZonePhase {
                    target_radius: 300.0,
                    shrink_duration: 20.0,
                    damage_per_second: 15.0,
                    delay_after: 20.0,
                },
                ZonePhase {
                    target_radius: 50.0,
                    shrink_duration: 15.0,
                    damage_per_second: 25.0,
                    delay_after: 0.0,
                },
            ],
        }
    }
}

/// Match state (owned by match task)
pub struct MatchState {
    pub id: Uuid,
    pub seed: u64,
    pub phase: MatchPhase,
    pub tick: u64,
    pub players: HashMap<Uuid, PlayerState>,
    pub zone: ZoneState,
    pub zone_config: ZoneConfig,
    pub zone_timer: f32,
    pub current_zone_phase: usize,
    pub is_shrinking: bool,
    pub projectiles: Vec<Projectile>,
    pub rng: ChaCha8Rng,
    pub start_time: Option<u64>,
    pub countdown_remaining: f32,
    pub min_players: usize,
    pub max_players: usize,
}

impl MatchState {
    pub fn new(id: Uuid, seed: u64, min_players: usize, max_players: usize) -> Self {
        let zone_config = ZoneConfig::default();
        let zone = ZoneState {
            center_x: 0.0,
            center_y: 0.0,
            radius: zone_config.initial_radius,
            target_center_x: 0.0,
            target_center_y: 0.0,
            target_radius: zone_config.initial_radius,
            damage_per_second: zone_config.phases[0].damage_per_second,
            shrink_delay: zone_config.initial_delay,
            phase: 0,
        };

        Self {
            id,
            seed,
            phase: MatchPhase::Waiting,
            tick: 0,
            players: HashMap::new(),
            zone,
            zone_config,
            zone_timer: 0.0,
            current_zone_phase: 0,
            is_shrinking: false,
            projectiles: Vec::new(),
            rng: ChaCha8Rng::seed_from_u64(seed),
            start_time: None,
            countdown_remaining: 5.0, // 5 second countdown
            min_players,
            max_players,
        }
    }

    /// Generate a spawn position for a new player
    pub fn generate_spawn_position(&mut self) -> (f32, f32, f32) {
        let angle = self.rng.gen_range(0.0..std::f32::consts::TAU);
        let distance = self.rng.gen_range(200.0..self.zone.radius * 0.8);
        let x = self.zone.center_x + angle.cos() * distance;
        let y = self.zone.center_y + angle.sin() * distance;
        let rotation = self.rng.gen_range(0.0..std::f32::consts::TAU);
        (x, y, rotation)
    }

    /// Count alive players
    pub fn alive_count(&self) -> usize {
        self.players.values().filter(|p| p.alive).count()
    }
}

/// Handle to a running match
#[derive(Clone)]
pub struct MatchHandle {
    pub id: Uuid,
    pub input_tx: mpsc::Sender<PlayerInput>,
    pub snapshot_tx: broadcast::Sender<ServerMsg>,
    pub player_count: Arc<std::sync::atomic::AtomicUsize>,
}

impl MatchHandle {
    pub fn player_count(&self) -> usize {
        self.player_count.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Registry of all active matches
pub struct MatchRegistry {
    matches: DashMap<Uuid, MatchHandle>,
}

impl MatchRegistry {
    pub fn new() -> Self {
        Self {
            matches: DashMap::new(),
        }
    }

    pub fn get(&self, id: &Uuid) -> Option<MatchHandle> {
        self.matches.get(id).map(|m| m.value().clone())
    }

    pub fn insert(&self, handle: MatchHandle) {
        self.matches.insert(handle.id, handle);
    }

    pub fn remove(&self, id: &Uuid) -> Option<MatchHandle> {
        self.matches.remove(id).map(|(_, h)| h)
    }

    pub fn active_matches(&self) -> usize {
        self.matches.len()
    }

    pub fn total_players(&self) -> usize {
        self.matches
            .iter()
            .map(|m| m.value().player_count())
            .sum()
    }

    /// Find a match with available slots
    pub fn find_available_match(&self, max_players: usize) -> Option<MatchHandle> {
        for entry in self.matches.iter() {
            if entry.value().player_count() < max_players {
                return Some(entry.value().clone());
            }
        }
        None
    }
}

impl Default for MatchRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// The authoritative game match
pub struct GameMatch {
    state: MatchState,
    input_rx: mpsc::Receiver<PlayerInput>,
    snapshot_tx: broadcast::Sender<ServerMsg>,
    snapshot_builder: SnapshotBuilder,
    player_count: Arc<std::sync::atomic::AtomicUsize>,
}

impl GameMatch {
    /// Create a new match
    pub fn new(
        id: Uuid,
        seed: u64,
        min_players: usize,
        max_players: usize,
    ) -> (Self, MatchHandle) {
        let (input_tx, input_rx) = mpsc::channel(256);
        let (snapshot_tx, _) = broadcast::channel(64);
        let player_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let handle = MatchHandle {
            id,
            input_tx,
            snapshot_tx: snapshot_tx.clone(),
            player_count: player_count.clone(),
        };

        let snapshot_interval = SIMULATION_TPS / SNAPSHOT_TPS;
        let game_match = Self {
            state: MatchState::new(id, seed, min_players, max_players),
            input_rx,
            snapshot_tx,
            snapshot_builder: SnapshotBuilder::new(snapshot_interval),
            player_count,
        };

        (game_match, handle)
    }

    /// Run the authoritative tick loop
    pub async fn run(mut self) {
        info!(match_id = %self.state.id, "Match started");

        let tick_duration = Duration::from_micros(1_000_000 / SIMULATION_TPS as u64);
        let mut tick_interval = interval(tick_duration);
        tick_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tick_interval.tick().await;

            // Drain input queue
            self.process_inputs();

            // Run simulation tick
            let events = self.run_tick();

            // Build and broadcast snapshot if needed
            if self.snapshot_builder.should_send() {
                let snapshot = self.snapshot_builder.build(
                    self.state.tick,
                    &self.state.zone,
                    &self.state.players,
                    events,
                );

                // Broadcast to all connected clients
                let _ = self.snapshot_tx.send(snapshot);
            }

            // Check for match end
            if self.state.phase == MatchPhase::Ended {
                info!(match_id = %self.state.id, "Match ended");
                break;
            }

            // Check if all players disconnected
            if self.state.players.is_empty() && self.state.phase != MatchPhase::Waiting {
                info!(match_id = %self.state.id, "All players left, ending match");
                break;
            }
        }

        // Send final match end message
        let winner = self
            .state
            .players
            .values()
            .find(|p| p.alive)
            .map(|p| p.user_id);

        let stats = self.build_match_stats();
        let _ = self.snapshot_tx.send(ServerMsg::MatchEnd {
            winner_user_id: winner,
            stats,
        });
    }

    /// Process all pending inputs from players
    fn process_inputs(&mut self) {
        while let Ok(input) = self.input_rx.try_recv() {
            match input.msg {
                ClientMsg::JoinMatch { ship_type, .. } => {
                    self.handle_join(input.user_id, ship_type);
                }
                ClientMsg::InputTick {
                    seq,
                    throttle,
                    steer,
                    shoot,
                    aim_yaw,
                } => {
                    self.handle_input(input.user_id, seq, throttle, steer, shoot, aim_yaw);
                }
                ClientMsg::Ping { t } => {
                    let _ = self.snapshot_tx.send(ServerMsg::Pong { t });
                }
                ClientMsg::LeaveMatch => {
                    self.handle_leave(input.user_id);
                }
            }
        }
    }

    /// Handle player join request
    fn handle_join(&mut self, user_id: Uuid, ship_type: ShipType) {
        if self.state.players.contains_key(&user_id) {
            warn!(user_id = %user_id, "Player already in match");
            return;
        }

        if self.state.players.len() >= self.state.max_players {
            let _ = self.snapshot_tx.send(ServerMsg::Error {
                code: "match_full".to_string(),
                message: "Match is full".to_string(),
            });
            return;
        }

        let (spawn_x, spawn_y, spawn_rotation) = self.state.generate_spawn_position();
        let player = PlayerState::new(
            user_id,
            format!("Player_{}", &user_id.to_string()[..8]),
            ship_type,
            None,
            spawn_x,
            spawn_y,
            spawn_rotation,
        );

        let player_info = PlayerInfo {
            user_id: player.user_id,
            display_name: player.display_name.clone(),
            ship_type: player.ship_type,
            flag_skin_id: player.flag_skin_id,
        };

        self.state.players.insert(user_id, player);
        self.player_count
            .store(self.state.players.len(), std::sync::atomic::Ordering::Relaxed);

        // Notify all players of the new player
        let _ = self.snapshot_tx.send(ServerMsg::PlayerJoined {
            player: player_info.clone(),
        });

        // Send match joined to the new player
        let players: Vec<PlayerInfo> = self
            .state
            .players
            .values()
            .map(|p| PlayerInfo {
                user_id: p.user_id,
                display_name: p.display_name.clone(),
                ship_type: p.ship_type,
                flag_skin_id: p.flag_skin_id,
            })
            .collect();

        let _ = self.snapshot_tx.send(ServerMsg::MatchJoined {
            match_id: self.state.id,
            seed: self.state.seed,
            players,
        });

        info!(
            match_id = %self.state.id,
            user_id = %user_id,
            player_count = self.state.players.len(),
            "Player joined match"
        );

        // Check if we should start countdown
        if self.state.phase == MatchPhase::Waiting
            && self.state.players.len() >= self.state.min_players
        {
            self.state.phase = MatchPhase::Countdown;
            self.state.countdown_remaining = 5.0;
            let _ = self.snapshot_tx.send(ServerMsg::MatchCountdown {
                seconds_remaining: 5,
            });
        }
    }

    /// Handle player input
    fn handle_input(
        &mut self,
        user_id: Uuid,
        seq: u32,
        throttle: f32,
        steer: f32,
        shoot: bool,
        aim_yaw: f32,
    ) {
        if let Some(player) = self.state.players.get_mut(&user_id) {
            if player.alive && seq > player.last_input_seq {
                player.last_input_seq = seq;
                player.current_input = TickInput {
                    seq,
                    throttle: throttle.clamp(-1.0, 1.0),
                    steer: steer.clamp(-1.0, 1.0),
                    shoot,
                    aim_yaw,
                };
            }
        }
    }

    /// Handle player leave
    fn handle_leave(&mut self, user_id: Uuid) {
        if let Some(player) = self.state.players.remove(&user_id) {
            self.player_count
                .store(self.state.players.len(), std::sync::atomic::Ordering::Relaxed);

            let _ = self.snapshot_tx.send(ServerMsg::PlayerLeft {
                user_id,
                reason: "disconnected".to_string(),
            });

            info!(
                match_id = %self.state.id,
                user_id = %user_id,
                "Player left match"
            );

            // Check win condition
            self.check_win_condition();

            drop(player); // Silence unused warning
        }
    }

    /// Run a single simulation tick
    fn run_tick(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();
        self.state.tick += 1;

        match self.state.phase {
            MatchPhase::Waiting => {
                // Do nothing, wait for players
            }
            MatchPhase::Countdown => {
                self.state.countdown_remaining -= tick_delta();
                if self.state.countdown_remaining <= 0.0 {
                    self.state.phase = MatchPhase::InProgress;
                    self.state.start_time = Some(unix_millis());
                    self.state.zone_timer = self.state.zone_config.initial_delay;
                    let _ = self.snapshot_tx.send(ServerMsg::MatchStarted {
                        tick: self.state.tick,
                    });
                    info!(match_id = %self.state.id, "Match started!");
                }
            }
            MatchPhase::InProgress => {
                // Update physics
                self.update_physics();

                // Process shooting and update projectiles
                events.extend(self.update_combat());

                // Update zone
                events.extend(self.update_zone());

                // Apply zone damage
                events.extend(self.apply_zone_damage());

                // Check win condition
                self.check_win_condition();
            }
            MatchPhase::Ended => {
                // Match is over
            }
        }

        events
    }

    /// Update ship physics
    fn update_physics(&mut self) {
        let player_positions: Vec<(Uuid, f32, f32, f32)> = self
            .state
            .players
            .values()
            .filter(|p| p.alive)
            .map(|p| {
                let stats = ShipStats::for_type(p.ship_type);
                (p.user_id, p.x, p.y, stats.hitbox_radius)
            })
            .collect();

        for player in self.state.players.values_mut() {
            if !player.alive {
                continue;
            }

            let stats = ShipStats::for_type(player.ship_type);
            let input = &player.current_input;

            let (new_x, new_y, new_rot, new_vel_x, new_vel_y) = PhysicsSystem::update_ship(
                player.x,
                player.y,
                player.rotation,
                player.vel_x,
                player.vel_y,
                input.throttle,
                input.steer,
                &stats,
            );

            player.x = new_x;
            player.y = new_y;
            player.rotation = new_rot;
            player.vel_x = new_vel_x;
            player.vel_y = new_vel_y;
        }

        // Resolve ship-to-ship collisions
        for i in 0..player_positions.len() {
            for j in (i + 1)..player_positions.len() {
                let (id1, x1, y1, r1) = player_positions[i];
                let (id2, x2, y2, r2) = player_positions[j];

                if PhysicsSystem::check_ship_collision(x1, y1, r1, x2, y2, r2) {
                    let ((new_x1, new_y1), (new_x2, new_y2)) =
                        PhysicsSystem::resolve_ship_collision(x1, y1, r1, x2, y2, r2);

                    if let Some(p1) = self.state.players.get_mut(&id1) {
                        p1.x = new_x1;
                        p1.y = new_y1;
                    }
                    if let Some(p2) = self.state.players.get_mut(&id2) {
                        p2.x = new_x2;
                        p2.y = new_y2;
                    }
                }
            }
        }
    }

    /// Update combat (shooting, projectiles, hits)
    fn update_combat(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let mut new_projectiles = Vec::new();

        // Process shooting
        for player in self.state.players.values_mut() {
            if !player.alive {
                continue;
            }

            // Update weapon cooldown
            player.weapon_cooldown = CombatSystem::update_cooldown(player.weapon_cooldown);

            // Check for shooting
            if player.current_input.shoot && CombatSystem::can_fire(player.weapon_cooldown) {
                let weapon_stats = WeaponStats::for_type(player.ship_type);
                let ship_stats = ShipStats::for_type(player.ship_type);

                // Spawn projectile at ship front
                let spawn_offset = ship_stats.hitbox_radius + 5.0;
                let spawn_x = player.x + player.current_input.aim_yaw.cos() * spawn_offset;
                let spawn_y = player.y + player.current_input.aim_yaw.sin() * spawn_offset;

                let projectile = Projectile::new(
                    player.user_id,
                    spawn_x,
                    spawn_y,
                    player.current_input.aim_yaw,
                    &weapon_stats,
                );

                events.push(GameEvent::Shot {
                    shooter_id: player.user_id,
                    projectile_id: projectile.id,
                    x: spawn_x,
                    y: spawn_y,
                    direction: player.current_input.aim_yaw,
                    speed: weapon_stats.projectile_speed,
                });

                new_projectiles.push(projectile);
                player.weapon_cooldown = CombatSystem::fire_cooldown(&weapon_stats);
                player.shots_fired += 1;
            }
        }

        self.state.projectiles.extend(new_projectiles);

        // Update projectiles and check hits
        let mut hits: Vec<HitResult> = Vec::new();
        let mut expired_projectiles: Vec<usize> = Vec::new();

        for (idx, projectile) in self.state.projectiles.iter_mut().enumerate() {
            if !projectile.update() {
                expired_projectiles.push(idx);
                continue;
            }

            // Check hits against all alive players (except owner)
            for player in self.state.players.values() {
                if !player.alive || player.user_id == projectile.owner_id {
                    continue;
                }

                let ship_stats = ShipStats::for_type(player.ship_type);
                if projectile.check_hit(player.x, player.y, ship_stats.hitbox_radius) {
                    hits.push(HitResult {
                        projectile_id: projectile.id,
                        shooter_id: projectile.owner_id,
                        target_id: player.user_id,
                        damage: projectile.damage,
                        x: projectile.x,
                        y: projectile.y,
                        target_killed: false,
                    });
                    expired_projectiles.push(idx);
                    break;
                }
            }
        }

        // Remove expired/hit projectiles (in reverse order to maintain indices)
        expired_projectiles.sort_unstable();
        expired_projectiles.dedup();
        for idx in expired_projectiles.into_iter().rev() {
            if idx < self.state.projectiles.len() {
                self.state.projectiles.remove(idx);
            }
        }

        // Apply damage from hits
        for mut hit in hits {
            if let Some(target) = self.state.players.get_mut(&hit.target_id) {
                let (new_health, killed) = CombatSystem::apply_damage(target.health, hit.damage);
                target.health = new_health;
                target.damage_taken += hit.damage;
                hit.target_killed = killed;

                if killed {
                    target.alive = false;
                    target.death_time = Some(unix_millis());
                }
            }

            // Update shooter stats
            if let Some(shooter) = self.state.players.get_mut(&hit.shooter_id) {
                shooter.shots_hit += 1;
                shooter.damage_dealt += hit.damage;
                if hit.target_killed {
                    shooter.kills += 1;
                }
            }

            events.push(GameEvent::Hit {
                shooter_id: hit.shooter_id,
                target_id: hit.target_id,
                damage: hit.damage,
                x: hit.x,
                y: hit.y,
            });

            if hit.target_killed {
                events.push(GameEvent::Kill {
                    killer_id: Some(hit.shooter_id),
                    victim_id: hit.target_id,
                    cause: "shot".to_string(),
                });
            }
        }

        events
    }

    /// Update zone shrinking
    fn update_zone(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let dt = tick_delta();

        self.state.zone_timer -= dt;

        if self.state.zone_timer <= 0.0 {
            if self.state.is_shrinking {
                // Finished shrinking, set up next phase
                self.state.zone.radius = self.state.zone.target_radius;
                self.state.zone.center_x = self.state.zone.target_center_x;
                self.state.zone.center_y = self.state.zone.target_center_y;
                self.state.is_shrinking = false;

                let phase_idx = self.state.current_zone_phase;
                if phase_idx < self.state.zone_config.phases.len() {
                    self.state.zone_timer = self.state.zone_config.phases[phase_idx].delay_after;
                    self.state.current_zone_phase += 1;
                }
            } else if self.state.current_zone_phase < self.state.zone_config.phases.len() {
                // Start new shrink phase
                let phase = &self.state.zone_config.phases[self.state.current_zone_phase];

                // Randomize new zone center (within current zone)
                let angle = self.state.rng.gen_range(0.0..std::f32::consts::TAU);
                let max_offset = (self.state.zone.radius - phase.target_radius).max(0.0) * 0.5;
                let offset = self.state.rng.gen_range(0.0..max_offset);

                self.state.zone.target_center_x = self.state.zone.center_x + angle.cos() * offset;
                self.state.zone.target_center_y = self.state.zone.center_y + angle.sin() * offset;
                self.state.zone.target_radius = phase.target_radius;
                self.state.zone.damage_per_second = phase.damage_per_second;
                self.state.zone.phase = self.state.current_zone_phase as u32;
                self.state.zone_timer = phase.shrink_duration;
                self.state.is_shrinking = true;

                events.push(GameEvent::ZoneShrink {
                    phase: self.state.zone.phase,
                    new_center_x: self.state.zone.target_center_x,
                    new_center_y: self.state.zone.target_center_y,
                    new_radius: self.state.zone.target_radius,
                });
            }
        }

        // Interpolate zone if shrinking
        if self.state.is_shrinking {
            let phase_idx = self.state.current_zone_phase;
            if phase_idx < self.state.zone_config.phases.len() {
                let phase = &self.state.zone_config.phases[phase_idx];
                let progress = 1.0 - (self.state.zone_timer / phase.shrink_duration).clamp(0.0, 1.0);

                // Linear interpolation
                let start_radius = if phase_idx == 0 {
                    self.state.zone_config.initial_radius
                } else {
                    self.state.zone_config.phases[phase_idx - 1].target_radius
                };

                self.state.zone.radius =
                    start_radius + (phase.target_radius - start_radius) * progress;
            }
        }

        self.state.zone.shrink_delay = self.state.zone_timer;

        events
    }

    /// Apply zone damage to players outside the zone
    fn apply_zone_damage(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let zone = &self.state.zone;
        let damage = CombatSystem::zone_damage(zone.damage_per_second);

        let mut deaths: Vec<Uuid> = Vec::new();

        for player in self.state.players.values_mut() {
            if !player.alive {
                continue;
            }

            if !PhysicsSystem::is_in_zone(
                player.x,
                player.y,
                zone.center_x,
                zone.center_y,
                zone.radius,
            ) {
                let (new_health, killed) = CombatSystem::apply_damage(player.health, damage);
                player.health = new_health;
                player.damage_taken += damage;

                events.push(GameEvent::ZoneDamage {
                    user_id: player.user_id,
                    damage,
                });

                if killed {
                    player.alive = false;
                    player.death_time = Some(unix_millis());
                    deaths.push(player.user_id);
                }
            }
        }

        for victim_id in deaths {
            events.push(GameEvent::Kill {
                killer_id: None,
                victim_id,
                cause: "zone".to_string(),
            });
        }

        events
    }

    /// Check win condition
    fn check_win_condition(&mut self) {
        if self.state.phase != MatchPhase::InProgress {
            return;
        }

        let alive = self.state.alive_count();
        if alive <= 1 {
            self.state.phase = MatchPhase::Ended;
            self.snapshot_builder.force_next();
        }
    }

    /// Build match stats
    fn build_match_stats(&self) -> MatchStats {
        let duration = self
            .state
            .start_time
            .map(|start| ((unix_millis() - start) / 1000) as u32)
            .unwrap_or(0);

        let mut player_stats: Vec<PlayerMatchStats> = self
            .state
            .players
            .values()
            .map(|p| {
                let alive_time = p
                    .death_time
                    .map(|death| ((death - p.spawn_time) / 1000) as u32)
                    .unwrap_or(duration);

                PlayerMatchStats {
                    user_id: p.user_id,
                    kills: p.kills,
                    damage_dealt: p.damage_dealt,
                    damage_taken: p.damage_taken,
                    shots_fired: p.shots_fired,
                    shots_hit: p.shots_hit,
                    placement: 0, // Will be calculated below
                    alive_time_secs: alive_time,
                }
            })
            .collect();

        // Calculate placements based on alive time (longer = better)
        player_stats.sort_by(|a, b| b.alive_time_secs.cmp(&a.alive_time_secs));
        for (i, stat) in player_stats.iter_mut().enumerate() {
            stat.placement = (i + 1) as u32;
        }

        MatchStats {
            duration_secs: duration,
            total_players: self.state.players.len() as u32,
            player_stats,
        }
    }
}
