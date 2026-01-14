//! Configuration module - environment variable parsing

use std::env;
use std::net::SocketAddr;

/// Application configuration loaded from environment variables
#[derive(Clone, Debug)]
pub struct Config {
    /// Server binding address
    pub server_addr: SocketAddr,
    /// Log level (trace, debug, info, warn, error)
    pub log_level: String,

    /// Supabase project URL
    pub supabase_url: String,
    /// Supabase anonymous key (for reference, clients use this)
    pub supabase_anon_key: String,
    /// Supabase service role key (bypasses RLS - server only!)
    pub supabase_service_role_key: String,
    /// Supabase JWT secret for token verification
    pub supabase_jwt_secret: String,

    /// Stripe secret API key
    pub stripe_secret_key: String,
    /// Stripe webhook signing secret
    pub stripe_webhook_secret: String,

    /// Public base URL for callbacks
    pub public_base_url: String,
    /// Allowed client origin for CORS
    pub client_origin: String,
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self, ConfigError> {
        // Render provides PORT env var, fall back to SERVER_ADDR or default
        let server_addr = if let Ok(port) = env::var("PORT") {
            format!("0.0.0.0:{}", port)
        } else {
            env::var("SERVER_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string())
        };

        Ok(Self {
            server_addr: server_addr
                .parse()
                .map_err(|_| ConfigError::InvalidAddress)?,

            log_level: env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),

            supabase_url: env::var("SUPABASE_URL")
                .map_err(|_| ConfigError::Missing("SUPABASE_URL"))?,
            supabase_anon_key: env::var("SUPABASE_ANON_KEY")
                .map_err(|_| ConfigError::Missing("SUPABASE_ANON_KEY"))?,
            supabase_service_role_key: env::var("SUPABASE_SERVICE_ROLE_KEY")
                .map_err(|_| ConfigError::Missing("SUPABASE_SERVICE_ROLE_KEY"))?,
            supabase_jwt_secret: env::var("SUPABASE_JWT_SECRET")
                .map_err(|_| ConfigError::Missing("SUPABASE_JWT_SECRET"))?,

            stripe_secret_key: env::var("STRIPE_SECRET_KEY")
                .map_err(|_| ConfigError::Missing("STRIPE_SECRET_KEY"))?,
            stripe_webhook_secret: env::var("STRIPE_WEBHOOK_SECRET")
                .map_err(|_| ConfigError::Missing("STRIPE_WEBHOOK_SECRET"))?,

            public_base_url: env::var("PUBLIC_BASE_URL")
                .map_err(|_| ConfigError::Missing("PUBLIC_BASE_URL"))?,
            client_origin: env::var("CLIENT_ORIGIN")
                .map_err(|_| ConfigError::Missing("CLIENT_ORIGIN"))?,
        })
    }
}

/// Configuration errors
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Missing required environment variable: {0}")]
    Missing(&'static str),

    #[error("Invalid server address format")]
    InvalidAddress,
}
