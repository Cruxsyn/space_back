//! Stripe webhook handler with signature verification

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::app::AppState;
use crate::store::supabase::SupabaseError;

type HmacSha256 = Hmac<Sha256>;

/// Handle Stripe webhook events
pub async fn stripe_webhook_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, WebhookError> {
    // Get the Stripe-Signature header
    let signature = headers
        .get("Stripe-Signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(WebhookError::MissingSignature)?;

    // Get the raw body as string for verification
    let payload = std::str::from_utf8(&body).map_err(|_| WebhookError::InvalidPayload)?;

    // Verify webhook signature
    verify_stripe_signature(payload, signature, &state.config.stripe_webhook_secret)?;

    // Parse the event
    let event: StripeEvent = serde_json::from_str(payload)
        .map_err(|e| {
            error!(error = %e, "Failed to parse Stripe event");
            WebhookError::InvalidPayload
        })?;

    info!(
        event_type = %event.event_type,
        event_id = %event.id,
        "Received Stripe webhook"
    );

    // Handle the event
    match event.event_type.as_str() {
        "checkout.session.completed" => {
            if let Some(session) = event.data.object.as_checkout_session() {
                handle_checkout_completed(&state, session).await?;
            }
        }
        "payment_intent.succeeded" => {
            info!("Payment intent succeeded (handled via checkout session)");
        }
        "payment_intent.payment_failed" => {
            if let Some(intent) = event.data.object.as_payment_intent() {
                handle_payment_failed(&state, &intent.id).await?;
            }
        }
        _ => {
            info!(event_type = %event.event_type, "Unhandled event type");
        }
    }

    Ok(StatusCode::OK)
}

/// Verify Stripe webhook signature
fn verify_stripe_signature(
    payload: &str,
    signature_header: &str,
    secret: &str,
) -> Result<(), WebhookError> {
    // Parse signature header
    let mut timestamp: Option<&str> = None;
    let mut signatures: Vec<&str> = Vec::new();

    for part in signature_header.split(',') {
        let mut kv = part.splitn(2, '=');
        if let (Some(key), Some(value)) = (kv.next(), kv.next()) {
            match key {
                "t" => timestamp = Some(value),
                "v1" => signatures.push(value),
                _ => {}
            }
        }
    }

    let timestamp = timestamp.ok_or(WebhookError::InvalidSignature)?;
    if signatures.is_empty() {
        return Err(WebhookError::InvalidSignature);
    }

    // Create signed payload
    let signed_payload = format!("{}.{}", timestamp, payload);

    // Compute expected signature
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| WebhookError::InvalidSignature)?;
    mac.update(signed_payload.as_bytes());
    let expected = hex::encode(mac.finalize().into_bytes());

    // Check if any signature matches
    let valid = signatures.iter().any(|sig| *sig == expected);
    if !valid {
        return Err(WebhookError::InvalidSignature);
    }

    // Optional: Check timestamp to prevent replay attacks (within 5 minutes)
    if let Ok(ts) = timestamp.parse::<i64>() {
        let now = chrono::Utc::now().timestamp();
        if (now - ts).abs() > 300 {
            warn!("Webhook timestamp is too old");
            // For MVP, we'll allow it but log a warning
        }
    }

    Ok(())
}

/// Handle successful checkout session
async fn handle_checkout_completed(
    state: &AppState,
    session: &CheckoutSessionData,
) -> Result<(), WebhookError> {
    info!(session_id = %session.id, "Processing checkout completion");

    // Extract metadata
    let user_id: Uuid = session
        .metadata
        .get("user_id")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| {
            error!("Missing user_id in session metadata");
            WebhookError::InvalidMetadata
        })?;

    let item_id: Uuid = session
        .metadata
        .get("item_id")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| {
            error!("Missing item_id in session metadata");
            WebhookError::InvalidMetadata
        })?;

    // Check if already processed (idempotency)
    let existing: Vec<PurchaseStatus> = state
        .supabase
        .get(
            "purchases",
            &format!("stripe_session_id=eq.{}", session.id),
        )
        .await
        .map_err(WebhookError::Database)?;

    if let Some(purchase) = existing.first() {
        if purchase.status == "paid" {
            info!(session_id = %session.id, "Purchase already processed (idempotent)");
            return Ok(());
        }
    }

    // Update purchase status to paid
    #[derive(serde::Serialize)]
    struct PurchaseUpdate {
        status: String,
        stripe_payment_intent: Option<String>,
    }

    state
        .supabase
        .update(
            "purchases",
            &format!("stripe_session_id=eq.{}", session.id),
            &PurchaseUpdate {
                status: "paid".to_string(),
                stripe_payment_intent: session.payment_intent.clone(),
            },
        )
        .await
        .map_err(WebhookError::Database)?;

    // Grant item to user
    state
        .inventory_store
        .grant_item(user_id, item_id)
        .await
        .map_err(WebhookError::Database)?;

    info!(
        user_id = %user_id,
        item_id = %item_id,
        session_id = %session.id,
        "Item granted successfully"
    );

    Ok(())
}

/// Handle failed payment
async fn handle_payment_failed(state: &AppState, payment_intent_id: &str) -> Result<(), WebhookError> {
    warn!(payment_intent_id = %payment_intent_id, "Payment failed");

    #[derive(serde::Serialize)]
    struct PurchaseUpdate {
        status: String,
    }

    let _ = state
        .supabase
        .update(
            "purchases",
            &format!("stripe_payment_intent=eq.{}", payment_intent_id),
            &PurchaseUpdate {
                status: "failed".to_string(),
            },
        )
        .await;

    Ok(())
}

// ============================================================================
// Stripe Event Types
// ============================================================================

#[derive(Debug, Deserialize)]
struct StripeEvent {
    id: String,
    #[serde(rename = "type")]
    event_type: String,
    data: StripeEventData,
}

#[derive(Debug, Deserialize)]
struct StripeEventData {
    object: StripeObject,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum StripeObject {
    CheckoutSession(CheckoutSessionData),
    PaymentIntent(PaymentIntentData),
    Unknown(serde_json::Value),
}

impl StripeObject {
    fn as_checkout_session(&self) -> Option<&CheckoutSessionData> {
        match self {
            StripeObject::CheckoutSession(s) => Some(s),
            _ => None,
        }
    }

    fn as_payment_intent(&self) -> Option<&PaymentIntentData> {
        match self {
            StripeObject::PaymentIntent(p) => Some(p),
            _ => None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct CheckoutSessionData {
    id: String,
    payment_intent: Option<String>,
    #[serde(default)]
    metadata: std::collections::HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct PaymentIntentData {
    id: String,
}

#[derive(Debug, Deserialize)]
struct PurchaseStatus {
    status: String,
}

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum WebhookError {
    #[error("Missing Stripe-Signature header")]
    MissingSignature,

    #[error("Invalid request payload")]
    InvalidPayload,

    #[error("Invalid webhook signature")]
    InvalidSignature,

    #[error("Invalid metadata in session")]
    InvalidMetadata,

    #[error("Database error: {0}")]
    Database(#[from] SupabaseError),
}

impl IntoResponse for WebhookError {
    fn into_response(self) -> axum::response::Response {
        let status = match &self {
            WebhookError::MissingSignature => StatusCode::BAD_REQUEST,
            WebhookError::InvalidPayload => StatusCode::BAD_REQUEST,
            WebhookError::InvalidSignature => StatusCode::UNAUTHORIZED,
            WebhookError::InvalidMetadata => StatusCode::BAD_REQUEST,
            WebhookError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        (status, self.to_string()).into_response()
    }
}
