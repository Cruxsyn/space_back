//! Stripe checkout session creation

use reqwest::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::Config;
use crate::store::supabase::{NewPurchase, StoreItem, SupabaseClient, SupabaseError};

/// Stripe service for payment operations
#[derive(Clone)]
pub struct StripeService {
    client: Client,
    supabase: SupabaseClient,
    stripe_secret_key: String,
    public_base_url: String,
    client_origin: String,
}

impl StripeService {
    pub fn new(config: &Config, supabase: SupabaseClient) -> Self {
        Self {
            client: Client::new(),
            supabase,
            stripe_secret_key: config.stripe_secret_key.clone(),
            public_base_url: config.public_base_url.clone(),
            client_origin: config.client_origin.clone(),
        }
    }

    /// Create a checkout session for an item
    pub async fn create_checkout_session(
        &self,
        user_id: Uuid,
        item_id: Uuid,
    ) -> Result<CheckoutSessionResponse, StripeError> {
        // Fetch the item from Supabase
        let items: Vec<StoreItem> = self
            .supabase
            .get("items", &format!("id=eq.{}&active=eq.true", item_id))
            .await
            .map_err(StripeError::Database)?;

        let item = items
            .into_iter()
            .next()
            .ok_or(StripeError::ItemNotFound)?;

        // Generate purchase ID
        let purchase_id = Uuid::new_v4();

        // Build Stripe API request
        let success_url = format!(
            "{}/checkout/success?session_id={{CHECKOUT_SESSION_ID}}",
            self.client_origin
        );
        let cancel_url = format!("{}/checkout/cancel", self.client_origin);

        // Create checkout session request body
        let mut form_data: Vec<(&str, String)> = vec![
            ("mode", "payment".to_string()),
            ("success_url", success_url),
            ("cancel_url", cancel_url),
            ("client_reference_id", user_id.to_string()),
            ("metadata[user_id]", user_id.to_string()),
            ("metadata[item_id]", item_id.to_string()),
            ("metadata[purchase_id]", purchase_id.to_string()),
        ];

        // Use existing price ID if available, otherwise create price data
        if let Some(price_id) = &item.stripe_price_id {
            form_data.push(("line_items[0][price]", price_id.clone()));
            form_data.push(("line_items[0][quantity]", "1".to_string()));
        } else {
            form_data.push(("line_items[0][price_data][currency]", "usd".to_string()));
            form_data.push(("line_items[0][price_data][unit_amount]", item.price_usd.to_string()));
            form_data.push(("line_items[0][price_data][product_data][name]", item.name.clone()));
            form_data.push(("line_items[0][price_data][product_data][description]", format!("Ship Game - {}", item.item_type)));
            form_data.push(("line_items[0][quantity]", "1".to_string()));
        }

        // Call Stripe API
        let response = self
            .client
            .post("https://api.stripe.com/v1/checkout/sessions")
            .basic_auth(&self.stripe_secret_key, None::<&str>)
            .form(&form_data)
            .send()
            .await
            .map_err(StripeError::Request)?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(StripeError::Api { status: status.as_u16(), body });
        }

        let session: StripeSession = response.json().await.map_err(StripeError::Request)?;

        let session_id = session.id.clone();
        let session_url = session.url.ok_or(StripeError::NoSessionUrl)?;

        // Create pending purchase record
        let purchase = NewPurchase {
            id: purchase_id,
            user_id,
            stripe_session_id: session_id.clone(),
            item_id,
            status: "pending".to_string(),
        };

        self.supabase
            .insert::<_, serde_json::Value>("purchases", &purchase)
            .await
            .map_err(StripeError::Database)?;

        Ok(CheckoutSessionResponse {
            session_id,
            url: session_url,
        })
    }

    /// Get the Stripe secret key for webhook verification
    pub fn secret_key(&self) -> &str {
        &self.stripe_secret_key
    }
}

/// Stripe checkout session response
#[derive(Debug, Deserialize)]
struct StripeSession {
    id: String,
    url: Option<String>,
}

/// Response from checkout session creation
#[derive(Debug, Clone, Serialize)]
pub struct CheckoutSessionResponse {
    pub session_id: String,
    pub url: String,
}

/// Stripe-related errors
#[derive(Debug, thiserror::Error)]
pub enum StripeError {
    #[error("Database error: {0}")]
    Database(#[from] SupabaseError),

    #[error("Item not found or inactive")]
    ItemNotFound,

    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("Stripe API error (status {status}): {body}")]
    Api { status: u16, body: String },

    #[error("No session URL returned")]
    NoSessionUrl,
}
