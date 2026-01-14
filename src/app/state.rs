//! Application state shared across routes

use std::sync::Arc;

use crate::config::Config;
use crate::game::MatchRegistry;
use crate::matchmaking::MatchmakingService;
use crate::payments::StripeService;
use crate::store::{InventoryStore, ProfileStore, SupabaseClient};

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub supabase: SupabaseClient,
    pub profile_store: ProfileStore,
    pub inventory_store: InventoryStore,
    pub stripe: StripeService,
    pub matchmaking: Arc<MatchmakingService>,
    pub match_registry: Arc<MatchRegistry>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        let config = Arc::new(config);

        // Initialize Supabase client
        let supabase = SupabaseClient::new(&config);

        // Initialize stores
        let profile_store = ProfileStore::new(supabase.clone());
        let inventory_store = InventoryStore::new(supabase.clone());

        // Initialize Stripe
        let stripe = StripeService::new(&config, supabase.clone());

        // Initialize match registry
        let match_registry = Arc::new(MatchRegistry::new());

        // Initialize matchmaking service (Arc for sharing across cloned AppState)
        let matchmaking = Arc::new(MatchmakingService::new(match_registry.clone()));

        Self {
            config,
            supabase,
            profile_store,
            inventory_store,
            stripe,
            matchmaking,
            match_registry,
        }
    }
}
