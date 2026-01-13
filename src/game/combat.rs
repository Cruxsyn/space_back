//! Combat system - weapons, damage, hit detection

use uuid::Uuid;

use crate::util::time::tick_delta;
use crate::ws::protocol::ShipType;

/// Weapon stats per ship type
#[derive(Debug, Clone, Copy)]
pub struct WeaponStats {
    /// Damage per hit
    pub damage: f32,
    /// Projectile speed
    pub projectile_speed: f32,
    /// Cooldown between shots (seconds)
    pub cooldown: f32,
    /// Projectile lifetime (seconds)
    pub projectile_lifetime: f32,
    /// Projectile hitbox radius
    pub projectile_radius: f32,
}

impl WeaponStats {
    pub fn for_type(ship_type: ShipType) -> Self {
        match ship_type {
            ShipType::Scout => Self {
                damage: 8.0,
                projectile_speed: 600.0,
                cooldown: 0.15,
                projectile_lifetime: 1.5,
                projectile_radius: 3.0,
            },
            ShipType::Fighter => Self {
                damage: 12.0,
                projectile_speed: 500.0,
                cooldown: 0.25,
                projectile_lifetime: 2.0,
                projectile_radius: 4.0,
            },
            ShipType::Cruiser => Self {
                damage: 15.0,
                projectile_speed: 400.0,
                cooldown: 0.4,
                projectile_lifetime: 2.5,
                projectile_radius: 5.0,
            },
            ShipType::Destroyer => Self {
                damage: 25.0,
                projectile_speed: 350.0,
                cooldown: 0.6,
                projectile_lifetime: 3.0,
                projectile_radius: 8.0,
            },
        }
    }
}

/// Active projectile in the game
#[derive(Debug, Clone)]
pub struct Projectile {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub x: f32,
    pub y: f32,
    pub vel_x: f32,
    pub vel_y: f32,
    pub damage: f32,
    pub radius: f32,
    pub lifetime_remaining: f32,
}

impl Projectile {
    /// Create a new projectile
    pub fn new(
        owner_id: Uuid,
        x: f32,
        y: f32,
        direction: f32,
        stats: &WeaponStats,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            owner_id,
            x,
            y,
            vel_x: direction.cos() * stats.projectile_speed,
            vel_y: direction.sin() * stats.projectile_speed,
            damage: stats.damage,
            radius: stats.projectile_radius,
            lifetime_remaining: stats.projectile_lifetime,
        }
    }

    /// Update projectile position, returns false if expired
    pub fn update(&mut self) -> bool {
        let dt = tick_delta();
        self.x += self.vel_x * dt;
        self.y += self.vel_y * dt;
        self.lifetime_remaining -= dt;
        self.lifetime_remaining > 0.0
    }

    /// Check collision with a target
    pub fn check_hit(&self, target_x: f32, target_y: f32, target_radius: f32) -> bool {
        let dx = self.x - target_x;
        let dy = self.y - target_y;
        let dist_sq = dx * dx + dy * dy;
        let combined_radius = self.radius + target_radius;
        dist_sq <= combined_radius * combined_radius
    }
}

/// Combat system for managing weapons and damage
pub struct CombatSystem;

impl CombatSystem {
    /// Check if a player can fire (cooldown check)
    pub fn can_fire(weapon_cooldown: f32) -> bool {
        weapon_cooldown <= 0.0
    }

    /// Update weapon cooldown
    pub fn update_cooldown(cooldown: f32) -> f32 {
        let dt = tick_delta();
        (cooldown - dt).max(0.0)
    }

    /// Get cooldown to set after firing
    pub fn fire_cooldown(stats: &WeaponStats) -> f32 {
        stats.cooldown
    }

    /// Calculate damage with potential modifiers
    pub fn calculate_damage(base_damage: f32, _modifier: f32) -> f32 {
        // For MVP, just return base damage
        // Could add armor, critical hits, etc. later
        base_damage
    }

    /// Apply damage to health, returns (new_health, is_dead)
    pub fn apply_damage(current_health: f32, damage: f32) -> (f32, bool) {
        let new_health = (current_health - damage).max(0.0);
        (new_health, new_health <= 0.0)
    }

    /// Calculate zone damage per tick
    pub fn zone_damage(damage_per_second: f32) -> f32 {
        damage_per_second * tick_delta()
    }
}

/// Hit result from combat resolution
#[derive(Debug, Clone)]
pub struct HitResult {
    pub projectile_id: Uuid,
    pub shooter_id: Uuid,
    pub target_id: Uuid,
    pub damage: f32,
    pub x: f32,
    pub y: f32,
    pub target_killed: bool,
}
