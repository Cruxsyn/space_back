//! Authentication middleware and JWT verification

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

use crate::app::AppState;

type HmacSha256 = Hmac<Sha256>;

/// JWT claims from Supabase auth token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    /// Subject (user ID)
    pub sub: Uuid,
    /// Audience
    #[serde(default)]
    pub aud: Option<String>,
    /// Expiration time (Unix timestamp)
    pub exp: u64,
    /// Issued at (Unix timestamp)
    #[serde(default)]
    pub iat: u64,
    /// Email (if available)
    #[serde(default)]
    pub email: Option<String>,
    /// Role
    #[serde(default)]
    pub role: Option<String>,
}

/// Verify a JWT token and extract claims
pub fn verify_jwt(token: &str, secret: &str) -> Result<JwtClaims, AuthError> {
    // Split token into parts
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(AuthError::InvalidToken);
    }

    let header_b64 = parts[0];
    let payload_b64 = parts[1];
    let signature_b64 = parts[2];

    // Verify signature (HMAC-SHA256)
    let message = format!("{}.{}", header_b64, payload_b64);
    
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| AuthError::InvalidToken)?;
    mac.update(message.as_bytes());
    
    let expected_signature = mac.finalize().into_bytes();
    let provided_signature = URL_SAFE_NO_PAD
        .decode(signature_b64)
        .map_err(|_| AuthError::InvalidToken)?;
    
    if expected_signature.as_slice() != provided_signature.as_slice() {
        return Err(AuthError::InvalidToken);
    }

    // Decode payload
    let payload_json = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|_| AuthError::InvalidToken)?;
    
    let claims: JwtClaims = serde_json::from_slice(&payload_json)
        .map_err(|_| AuthError::InvalidToken)?;

    // Check expiration
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    if claims.exp < now {
        return Err(AuthError::TokenExpired);
    }

    Ok(claims)
}

/// Extract JWT from Authorization header
pub fn extract_bearer_token(auth_header: &str) -> Option<&str> {
    auth_header.strip_prefix("Bearer ")
}

/// Authentication error types
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Missing authorization header")]
    MissingHeader,

    #[error("Invalid authorization header format")]
    InvalidFormat,

    #[error("Invalid token")]
    InvalidToken,

    #[error("Token expired")]
    TokenExpired,

    #[error("Invalid audience")]
    InvalidAudience,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let status = match &self {
            AuthError::MissingHeader => StatusCode::UNAUTHORIZED,
            AuthError::InvalidFormat => StatusCode::BAD_REQUEST,
            AuthError::InvalidToken => StatusCode::UNAUTHORIZED,
            AuthError::TokenExpired => StatusCode::UNAUTHORIZED,
            AuthError::InvalidAudience => StatusCode::UNAUTHORIZED,
        };

        (status, self.to_string()).into_response()
    }
}

/// Authenticated user extractor result
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user_id: Uuid,
    pub claims: JwtClaims,
}

/// Middleware to require authentication
pub async fn require_auth(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let auth_header = request
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or(AuthError::MissingHeader)?;

    let token = extract_bearer_token(auth_header).ok_or(AuthError::InvalidFormat)?;

    let claims = verify_jwt(token, &state.config.supabase_jwt_secret)?;

    let auth_user = AuthenticatedUser {
        user_id: claims.sub,
        claims,
    };

    // Insert into request extensions for handlers to access
    request.extensions_mut().insert(auth_user);

    Ok(next.run(request).await)
}

/// Extract authenticated user from request extensions
pub fn get_auth_user(request: &Request) -> Option<&AuthenticatedUser> {
    request.extensions().get::<AuthenticatedUser>()
}
