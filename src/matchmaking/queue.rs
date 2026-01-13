//! Matchmaking queue implementation

use std::collections::VecDeque;
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::ws::protocol::ShipType;

/// Player in the matchmaking queue
#[derive(Debug, Clone)]
pub struct QueuedPlayer {
    pub user_id: Uuid,
    pub display_name: String,
    pub ship_type: ShipType,
    pub flag_skin_id: Option<Uuid>,
    pub queued_at: Instant,
}

impl QueuedPlayer {
    pub fn new(user_id: Uuid, display_name: String, ship_type: ShipType) -> Self {
        Self {
            user_id,
            display_name,
            ship_type,
            flag_skin_id: None,
            queued_at: Instant::now(),
        }
    }

    /// How long this player has been waiting
    pub fn wait_time(&self) -> Duration {
        self.queued_at.elapsed()
    }
}

/// The matchmaking queue
pub struct MatchmakingQueue {
    queue: VecDeque<QueuedPlayer>,
    /// Minimum players to start a match
    min_players: usize,
    /// Maximum players per match
    max_players: usize,
    /// Max time to wait before starting with fewer players
    max_wait_time: Duration,
}

impl MatchmakingQueue {
    pub fn new(min_players: usize, max_players: usize, max_wait_secs: u64) -> Self {
        Self {
            queue: VecDeque::new(),
            min_players,
            max_players,
            max_wait_time: Duration::from_secs(max_wait_secs),
        }
    }

    /// Add a player to the queue
    pub fn enqueue(&mut self, player: QueuedPlayer) {
        // Remove if already in queue (rejoin)
        self.queue.retain(|p| p.user_id != player.user_id);
        self.queue.push_back(player);
    }

    /// Remove a player from the queue
    pub fn dequeue(&mut self, user_id: Uuid) -> Option<QueuedPlayer> {
        if let Some(pos) = self.queue.iter().position(|p| p.user_id == user_id) {
            self.queue.remove(pos)
        } else {
            None
        }
    }

    /// Check if a player is in the queue
    pub fn contains(&self, user_id: &Uuid) -> bool {
        self.queue.iter().any(|p| &p.user_id == user_id)
    }

    /// Get queue length
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Try to form a match from queued players
    /// Returns players to be put in a match, or None if not enough
    pub fn try_form_match(&mut self) -> Option<Vec<QueuedPlayer>> {
        if self.queue.len() >= self.min_players {
            // Have enough players, form a full match
            let count = self.queue.len().min(self.max_players);
            let players: Vec<QueuedPlayer> = self.queue.drain(..count).collect();
            return Some(players);
        }

        // Check if anyone has waited too long
        if !self.queue.is_empty() {
            let oldest_wait = self.queue.front().map(|p| p.wait_time()).unwrap_or_default();
            if oldest_wait >= self.max_wait_time && self.queue.len() >= 1 {
                // Start with whoever we have (could be just 1 for testing)
                let players: Vec<QueuedPlayer> = self.queue.drain(..).collect();
                return Some(players);
            }
        }

        None
    }

    /// Get min players setting
    pub fn min_players(&self) -> usize {
        self.min_players
    }

    /// Get max players setting
    pub fn max_players(&self) -> usize {
        self.max_players
    }
}

impl Default for MatchmakingQueue {
    fn default() -> Self {
        Self::new(2, 20, 30) // 2-20 players, 30 second max wait
    }
}
