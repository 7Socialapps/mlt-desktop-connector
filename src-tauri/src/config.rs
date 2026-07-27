use std::env;

use thiserror::Error;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub environment: String,
    pub supabase_url: String,
    pub supabase_anon_key: String,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("MLT_ENV must be set to \"staging\" (got: {0})")]
    InvalidEnvironment(String),
    #[error("missing required environment variable: {0}")]
    MissingVar(&'static str),
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let environment = env::var("MLT_ENV").map_err(|_| ConfigError::MissingVar("MLT_ENV"))?;

        if environment != "staging" {
            return Err(ConfigError::InvalidEnvironment(environment));
        }

        let supabase_url =
            env::var("MLT_SUPABASE_URL").map_err(|_| ConfigError::MissingVar("MLT_SUPABASE_URL"))?;
        let supabase_anon_key = env::var("MLT_SUPABASE_ANON_KEY")
            .map_err(|_| ConfigError::MissingVar("MLT_SUPABASE_ANON_KEY"))?;

        if supabase_url.is_empty() || supabase_anon_key.is_empty() {
            return Err(ConfigError::MissingVar("MLT_SUPABASE_URL or MLT_SUPABASE_ANON_KEY"));
        }

        Ok(Self {
            environment,
            supabase_url: supabase_url.trim_end_matches('/').to_string(),
            supabase_anon_key,
        })
    }
}
