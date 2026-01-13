//! User profile management

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::supabase::{SupabaseClient, SupabaseError};

/// User profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub id: Uuid,
    pub display_name: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// New profile for insertion
#[derive(Debug, Clone, Serialize)]
pub struct NewProfile {
    pub id: Uuid,
    pub display_name: String,
}

/// Profile update
#[derive(Debug, Clone, Serialize)]
pub struct ProfileUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

/// Profile store operations
#[derive(Clone)]
pub struct ProfileStore {
    client: SupabaseClient,
}

impl ProfileStore {
    pub fn new(client: SupabaseClient) -> Self {
        Self { client }
    }

    /// Get a user profile by ID
    pub async fn get_profile(&self, user_id: Uuid) -> Result<Option<UserProfile>, SupabaseError> {
        let query = format!("id=eq.{}", user_id);
        self.client.get_one("profiles", &query).await
    }

    /// Create a new user profile
    pub async fn create_profile(
        &self,
        user_id: Uuid,
        display_name: &str,
    ) -> Result<UserProfile, SupabaseError> {
        let profile = NewProfile {
            id: user_id,
            display_name: display_name.to_string(),
        };
        self.client.insert("profiles", &profile).await
    }

    /// Update a user profile
    pub async fn update_profile(
        &self,
        user_id: Uuid,
        update: ProfileUpdate,
    ) -> Result<(), SupabaseError> {
        let query = format!("id=eq.{}", user_id);
        self.client.update("profiles", &query, &update).await
    }

    /// Get or create profile (ensures profile exists)
    pub async fn ensure_profile(
        &self,
        user_id: Uuid,
        default_name: &str,
    ) -> Result<UserProfile, SupabaseError> {
        match self.get_profile(user_id).await? {
            Some(profile) => Ok(profile),
            None => self.create_profile(user_id, default_name).await,
        }
    }
}
