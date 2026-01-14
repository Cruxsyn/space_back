//! Matchmaking service - manages queue and match creation

use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Mutex};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::game::{GameMatch, MatchRegistry, PlayerInput};
use crate::ws::protocol::ServerMsg;

use super::queue::{MatchmakingQueue, QueuedPlayer};

/// Player connection handle for routing messages
#[derive(Clone)]
pub struct PlayerConnection {
    pub user_id: Uuid,
    /// Channel to send inputs to current match
    pub input_tx: mpsc::Sender<PlayerInput>,
    /// Channel to receive snapshots from current match
    pub snapshot_rx: broadcast::Sender<ServerMsg>,
}

/// Matchmaking service
pub struct MatchmakingService {
    queue: Arc<Mutex<MatchmakingQueue>>,
    registry: Arc<MatchRegistry>,
    /// Connected players awaiting or in matches
    players: DashMap<Uuid, PlayerConnection>,
    /// Map of player -> current match
    player_matches: DashMap<Uuid, Uuid>,
}

impl MatchmakingService {
    pub fn new(registry: Arc<MatchRegistry>) -> Self {
        Self {
            queue: Arc::new(Mutex::new(MatchmakingQueue::default())),
            registry,
            players: DashMap::new(),
            player_matches: DashMap::new(),
        }
    }

    /// Register a player connection (called when WebSocket connects)
    /// Returns channels for communication
    pub async fn register_player(
        &self,
        user_id: Uuid,
    ) -> (mpsc::Sender<PlayerInput>, broadcast::Receiver<ServerMsg>) {
        // Create personal channels for this player
        let (input_tx, mut input_rx) = mpsc::channel::<PlayerInput>(64);
        let (snapshot_tx, snapshot_rx) = broadcast::channel::<ServerMsg>(64);

        let connection = PlayerConnection {
            user_id,
            input_tx: input_tx.clone(),
            snapshot_rx: snapshot_tx.clone(),
        };

        self.players.insert(user_id, connection);

        // Spawn a task to route messages from personal channel to match channel
        let registry = self.registry.clone();
        let player_matches = self.player_matches.clone();
        let players_for_input = self.players.clone();

        tokio::spawn(async move {
            while let Some(input) = input_rx.recv().await {
                // Find player's current match and forward input
                if let Some(match_id) = player_matches.get(&user_id) {
                    if let Some(match_handle) = registry.get(&match_id) {
                        if match_handle.input_tx.send(input).await.is_err() {
                            warn!(user_id = %user_id, "Failed to send input to match");
                        }
                    }
                }
            }
            // Cleanup when channel closes
            players_for_input.remove(&user_id);
        });

        // Spawn a task to route snapshots from match to player
        let snapshot_tx_clone = snapshot_tx.clone();
        let player_matches_clone = self.player_matches.clone();
        let registry_clone = self.registry.clone();
        let players_for_snapshot = self.players.clone();

        tokio::spawn(async move {
            // This task subscribes to match broadcasts and forwards to player
            let mut current_match_rx: Option<broadcast::Receiver<ServerMsg>> = None;
            let mut current_match_id: Option<Uuid> = None;

            loop {
                // Check if player's match changed
                let new_match_id = player_matches_clone.get(&user_id).map(|r| *r);

                if new_match_id != current_match_id {
                    current_match_id = new_match_id;
                    current_match_rx = new_match_id.and_then(|mid| {
                        registry_clone.get(&mid).map(|h| h.snapshot_tx.subscribe())
                    });
                }

                if let Some(ref mut rx) = current_match_rx {
                    match rx.recv().await {
                        Ok(msg) => {
                            let _ = snapshot_tx_clone.send(msg);
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!(user_id = %user_id, lagged = n, "Snapshot receiver lagged");
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            current_match_rx = None;
                            current_match_id = None;
                        }
                    }
                } else {
                    // No match, wait a bit
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }

                // Check if player disconnected
                if !players_for_snapshot.contains_key(&user_id) {
                    break;
                }
            }
        });

        (input_tx, snapshot_rx)
    }

    /// Unregister a player (called when WebSocket disconnects)
    pub async fn unregister_player(&self, user_id: Uuid) {
        self.players.remove(&user_id);
        self.player_matches.remove(&user_id);

        let mut queue = self.queue.lock().await;
        queue.dequeue(user_id);

        info!(user_id = %user_id, "Player unregistered from matchmaking");
    }

    /// Join matchmaking queue
    pub async fn join_queue(&self, player: QueuedPlayer) -> Result<(), String> {
        let user_id = player.user_id;

        // Check if already in a match
        if self.player_matches.contains_key(&user_id) {
            return Err("Already in a match".to_string());
        }

        let mut queue = self.queue.lock().await;
        queue.enqueue(player);

        info!(user_id = %user_id, queue_size = queue.len(), "Player joined matchmaking queue");

        // Don't try to form match immediately - let the run() loop handle it
        // This gives time for WebSocket connections to be established
        // The run() loop will only include connected players when forming matches

        Ok(())
    }

    /// Leave matchmaking queue
    pub async fn leave_queue(&self, user_id: Uuid) {
        let mut queue = self.queue.lock().await;
        queue.dequeue(user_id);
    }

    /// Create a match with the given players
    async fn create_match(&self, players: Vec<QueuedPlayer>) {
        let match_id = Uuid::new_v4();
        let seed = rand::random::<u64>();
        let min_players = 2;
        let max_players = 20;

        let (game_match, handle) = GameMatch::new(match_id, seed, min_players, max_players);

        // Register match
        self.registry.insert(handle.clone());

        // Associate players with match
        for player in &players {
            self.player_matches.insert(player.user_id, match_id);
        }

        info!(
            match_id = %match_id,
            player_count = players.len(),
            "Created new match"
        );

        // Spawn match task
        let registry = self.registry.clone();
        let player_matches = self.player_matches.clone();
        let match_player_ids: Vec<Uuid> = players.iter().map(|p| p.user_id).collect();

        tokio::spawn(async move {
            game_match.run().await;

            // Cleanup after match ends
            registry.remove(&match_id);
            for pid in match_player_ids {
                player_matches.remove(&pid);
            }

            info!(match_id = %match_id, "Match removed from registry");
        });

        // Send join commands to move players into the match
        for player in players {
            if let Some(conn) = self.players.get(&player.user_id) {
                let join_input = PlayerInput {
                    user_id: player.user_id,
                    msg: crate::ws::protocol::ClientMsg::JoinMatch {
                        match_id: Some(match_id),
                        ship_type: player.ship_type,
                    },
                    received_at: crate::util::time::unix_millis(),
                };

                if let Some(match_handle) = self.registry.get(&match_id) {
                    if match_handle.input_tx.send(join_input).await.is_err() {
                        error!(user_id = %player.user_id, "Failed to send join to match");
                    }
                }

                drop(conn);
            }
        }
    }

    /// Run the matchmaking service (periodic queue processing)
    pub async fn run(&self) {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));

        loop {
            interval.tick().await;

            // Get connected player IDs
            let connected_ids: std::collections::HashSet<Uuid> = 
                self.players.iter().map(|entry| *entry.key()).collect();

            // Try to form matches from queue with connected players only
            let mut queue = self.queue.lock().await;
            
            // Filter queue to only include connected players for match formation
            let connected_count = queue.iter().filter(|p| connected_ids.contains(&p.user_id)).count();
            
            let min_players = queue.min_players();
            let max_players = queue.max_players();
            let waited_too_long = queue.has_waited_too_long(&connected_ids);
            
            if connected_count >= min_players || (connected_count >= 1 && waited_too_long) {
                // Extract connected players for match
                let players: Vec<QueuedPlayer> = queue
                    .drain_connected(&connected_ids, max_players)
                    .collect();
                
                if !players.is_empty() {
                    drop(queue); // Release lock for match creation
                    self.create_match(players).await;
                }
            }
        }
    }

    /// Get current queue size
    pub async fn queue_size(&self) -> usize {
        self.queue.lock().await.len()
    }

    /// Check if player is in queue
    pub async fn is_in_queue(&self, user_id: &Uuid) -> bool {
        self.queue.lock().await.contains(user_id)
    }

    /// Get player's current match ID
    pub fn get_player_match(&self, user_id: &Uuid) -> Option<Uuid> {
        self.player_matches.get(user_id).map(|r| *r)
    }
}

impl Clone for MatchmakingService {
    fn clone(&self) -> Self {
        Self {
            queue: self.queue.clone(),
            registry: self.registry.clone(),
            players: self.players.clone(),
            player_matches: self.player_matches.clone(),
        }
    }
}
