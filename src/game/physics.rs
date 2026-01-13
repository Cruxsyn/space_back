//! Ship physics and movement constraints

use crate::util::time::tick_delta;
use crate::ws::protocol::ShipType;

/// Ship physics constants per ship type
#[derive(Debug, Clone, Copy)]
pub struct ShipStats {
    /// Maximum forward speed
    pub max_speed: f32,
    /// Acceleration rate
    pub acceleration: f32,
    /// Deceleration/drag coefficient
    pub drag: f32,
    /// Turn rate in radians per second
    pub turn_rate: f32,
    /// Maximum health
    pub max_health: f32,
    /// Ship hitbox radius
    pub hitbox_radius: f32,
}

impl ShipStats {
    pub fn for_type(ship_type: ShipType) -> Self {
        match ship_type {
            ShipType::Scout => Self {
                max_speed: 400.0,
                acceleration: 300.0,
                drag: 0.95,
                turn_rate: 4.0,
                max_health: 60.0,
                hitbox_radius: 15.0,
            },
            ShipType::Fighter => Self {
                max_speed: 300.0,
                acceleration: 250.0,
                drag: 0.93,
                turn_rate: 3.0,
                max_health: 100.0,
                hitbox_radius: 20.0,
            },
            ShipType::Cruiser => Self {
                max_speed: 200.0,
                acceleration: 150.0,
                drag: 0.90,
                turn_rate: 2.0,
                max_health: 150.0,
                hitbox_radius: 30.0,
            },
            ShipType::Destroyer => Self {
                max_speed: 180.0,
                acceleration: 120.0,
                drag: 0.88,
                turn_rate: 1.5,
                max_health: 120.0,
                hitbox_radius: 35.0,
            },
        }
    }
}

/// Physics system for updating ship positions and velocities
pub struct PhysicsSystem;

impl PhysicsSystem {
    /// Update a ship's physics based on input
    /// Returns (new_x, new_y, new_rotation, new_vel_x, new_vel_y)
    pub fn update_ship(
        x: f32,
        y: f32,
        rotation: f32,
        vel_x: f32,
        vel_y: f32,
        throttle: f32,
        steer: f32,
        stats: &ShipStats,
    ) -> (f32, f32, f32, f32, f32) {
        let dt = tick_delta();

        // Clamp inputs
        let throttle = throttle.clamp(-1.0, 1.0);
        let steer = steer.clamp(-1.0, 1.0);

        // Update rotation
        let new_rotation = rotation + steer * stats.turn_rate * dt;
        // Normalize to 0..2π
        let new_rotation = new_rotation.rem_euclid(std::f32::consts::TAU);

        // Calculate thrust direction (forward is rotation direction)
        let thrust_x = new_rotation.cos();
        let thrust_y = new_rotation.sin();

        // Apply throttle (negative = reverse at reduced power)
        let thrust_power = if throttle >= 0.0 {
            throttle * stats.acceleration
        } else {
            throttle * stats.acceleration * 0.5 // Reverse is slower
        };

        // Update velocity with thrust and drag
        let mut new_vel_x = vel_x + thrust_x * thrust_power * dt;
        let mut new_vel_y = vel_y + thrust_y * thrust_power * dt;

        // Apply drag
        new_vel_x *= stats.drag;
        new_vel_y *= stats.drag;

        // Clamp to max speed
        let speed = (new_vel_x * new_vel_x + new_vel_y * new_vel_y).sqrt();
        if speed > stats.max_speed {
            let scale = stats.max_speed / speed;
            new_vel_x *= scale;
            new_vel_y *= scale;
        }

        // Update position
        let new_x = x + new_vel_x * dt;
        let new_y = y + new_vel_y * dt;

        (new_x, new_y, new_rotation, new_vel_x, new_vel_y)
    }

    /// Check if a point is inside the zone
    pub fn is_in_zone(x: f32, y: f32, zone_center_x: f32, zone_center_y: f32, zone_radius: f32) -> bool {
        let dx = x - zone_center_x;
        let dy = y - zone_center_y;
        let dist_sq = dx * dx + dy * dy;
        dist_sq <= zone_radius * zone_radius
    }

    /// Calculate distance from zone edge (negative = inside, positive = outside)
    pub fn zone_distance(x: f32, y: f32, zone_center_x: f32, zone_center_y: f32, zone_radius: f32) -> f32 {
        let dx = x - zone_center_x;
        let dy = y - zone_center_y;
        let dist = (dx * dx + dy * dy).sqrt();
        dist - zone_radius
    }

    /// Check collision between two ships
    pub fn check_ship_collision(
        x1: f32, y1: f32, radius1: f32,
        x2: f32, y2: f32, radius2: f32,
    ) -> bool {
        let dx = x2 - x1;
        let dy = y2 - y1;
        let dist_sq = dx * dx + dy * dy;
        let combined_radius = radius1 + radius2;
        dist_sq <= combined_radius * combined_radius
    }

    /// Resolve collision between two ships (pushes them apart)
    /// Returns ((new_x1, new_y1), (new_x2, new_y2))
    pub fn resolve_ship_collision(
        x1: f32, y1: f32, radius1: f32,
        x2: f32, y2: f32, radius2: f32,
    ) -> ((f32, f32), (f32, f32)) {
        let dx = x2 - x1;
        let dy = y2 - y1;
        let dist = (dx * dx + dy * dy).sqrt();
        
        if dist < 0.001 {
            // Ships are at same position, push apart arbitrarily
            return ((x1 - radius1, y1), (x2 + radius2, y2));
        }

        let combined_radius = radius1 + radius2;
        let overlap = combined_radius - dist;
        
        if overlap <= 0.0 {
            return ((x1, y1), (x2, y2)); // No collision
        }

        // Normalize direction
        let nx = dx / dist;
        let ny = dy / dist;

        // Push apart by half the overlap each
        let push = overlap / 2.0 + 0.1; // Small buffer

        let new_x1 = x1 - nx * push;
        let new_y1 = y1 - ny * push;
        let new_x2 = x2 + nx * push;
        let new_y2 = y2 + ny * push;

        ((new_x1, new_y1), (new_x2, new_y2))
    }
}
