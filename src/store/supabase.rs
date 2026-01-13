//! Supabase REST API client using service_role key

use reqwest::Client;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;

use crate::config::Config;

/// Supabase client for server-side database operations
/// Uses service_role key which bypasses RLS - handle with care!
#[derive(Clone)]
pub struct SupabaseClient {
    client: Client,
    base_url: String,
    service_role_key: String,
}

impl SupabaseClient {
    pub fn new(config: &Config) -> Self {
        Self {
            client: Client::new(),
            base_url: config.supabase_url.clone(),
            service_role_key: config.supabase_service_role_key.clone(),
        }
    }

    /// Get the REST API URL for a table
    fn rest_url(&self, table: &str) -> String {
        format!("{}/rest/v1/{}", self.base_url, table)
    }

    /// Make an authenticated GET request
    pub async fn get<T: DeserializeOwned>(
        &self,
        table: &str,
        query: &str,
    ) -> Result<Vec<T>, SupabaseError> {
        let url = format!("{}?{}", self.rest_url(table), query);

        let response = self
            .client
            .get(&url)
            .header("apikey", &self.service_role_key)
            .header("Authorization", format!("Bearer {}", self.service_role_key))
            .header("Content-Type", "application/json")
            .send()
            .await
            .map_err(SupabaseError::Request)?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(SupabaseError::Api { status: status.as_u16(), body });
        }

        response.json().await.map_err(SupabaseError::Parse)
    }

    /// Make an authenticated GET request expecting a single row
    pub async fn get_one<T: DeserializeOwned>(
        &self,
        table: &str,
        query: &str,
    ) -> Result<Option<T>, SupabaseError> {
        let url = format!("{}?{}", self.rest_url(table), query);

        let response = self
            .client
            .get(&url)
            .header("apikey", &self.service_role_key)
            .header("Authorization", format!("Bearer {}", self.service_role_key))
            .header("Content-Type", "application/json")
            .header("Accept", "application/vnd.pgrst.object+json")
            .send()
            .await
            .map_err(SupabaseError::Request)?;

        if response.status() == reqwest::StatusCode::NOT_ACCEPTABLE {
            // No rows found
            return Ok(None);
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(SupabaseError::Api { status: status.as_u16(), body });
        }

        response.json().await.map(Some).map_err(SupabaseError::Parse)
    }

    /// Make an authenticated POST request (insert)
    pub async fn insert<T: Serialize, R: DeserializeOwned>(
        &self,
        table: &str,
        data: &T,
    ) -> Result<R, SupabaseError> {
        let url = self.rest_url(table);

        let response = self
            .client
            .post(&url)
            .header("apikey", &self.service_role_key)
            .header("Authorization", format!("Bearer {}", self.service_role_key))
            .header("Content-Type", "application/json")
            .header("Prefer", "return=representation")
            .json(data)
            .send()
            .await
            .map_err(SupabaseError::Request)?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(SupabaseError::Api { status: status.as_u16(), body });
        }

        // PostgREST returns an array, get first element
        let results: Vec<R> = response.json().await.map_err(SupabaseError::Parse)?;
        results
            .into_iter()
            .next()
            .ok_or(SupabaseError::NoRowReturned)
    }

    /// Make an authenticated PATCH request (update)
    pub async fn update<T: Serialize>(
        &self,
        table: &str,
        query: &str,
        data: &T,
    ) -> Result<(), SupabaseError> {
        let url = format!("{}?{}", self.rest_url(table), query);

        let response = self
            .client
            .patch(&url)
            .header("apikey", &self.service_role_key)
            .header("Authorization", format!("Bearer {}", self.service_role_key))
            .header("Content-Type", "application/json")
            .json(data)
            .send()
            .await
            .map_err(SupabaseError::Request)?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(SupabaseError::Api { status: status.as_u16(), body });
        }

        Ok(())
    }

    /// Upsert (insert or update on conflict)
    pub async fn upsert<T: Serialize>(
        &self,
        table: &str,
        data: &T,
        on_conflict: &str,
    ) -> Result<(), SupabaseError> {
        let url = self.rest_url(table);

        let response = self
            .client
            .post(&url)
            .header("apikey", &self.service_role_key)
            .header("Authorization", format!("Bearer {}", self.service_role_key))
            .header("Content-Type", "application/json")
            .header("Prefer", format!("resolution=merge-duplicates,return=minimal"))
            .header("On-Conflict", on_conflict)
            .json(data)
            .send()
            .await
            .map_err(SupabaseError::Request)?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(SupabaseError::Api { status: status.as_u16(), body });
        }

        Ok(())
    }
}

/// Store item as defined in items table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreItem {
    pub id: Uuid,
    #[serde(rename = "type")]
    pub item_type: String,
    pub name: String,
    pub price_usd: i32,
    pub stripe_price_id: Option<String>,
    pub active: bool,
}

/// Purchase record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Purchase {
    pub id: Uuid,
    pub user_id: Uuid,
    pub stripe_session_id: Option<String>,
    pub stripe_payment_intent: Option<String>,
    pub item_id: Uuid,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// New purchase for insertion
#[derive(Debug, Clone, Serialize)]
pub struct NewPurchase {
    pub id: Uuid,
    pub user_id: Uuid,
    pub stripe_session_id: String,
    pub item_id: Uuid,
    pub status: String,
}

/// Supabase errors
#[derive(Debug, thiserror::Error)]
pub enum SupabaseError {
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("API error (status {status}): {body}")]
    Api { status: u16, body: String },

    #[error("Failed to parse response: {0}")]
    Parse(reqwest::Error),

    #[error("No row returned from insert")]
    NoRowReturned,
}
