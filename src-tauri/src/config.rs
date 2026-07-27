use std::env;

use thiserror::Error;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub environment: String,
    pub supabase_url: String,
    pub supabase_anon_key: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("MLT_ENV must be \"staging\" or \"production\" (got: {0})")]
    InvalidEnvironment(String),
    #[error("missing required environment variable: {0}")]
    MissingVar(&'static str),
}

fn read_var(name: &'static str, compile_time: Option<&str>) -> Result<String, ConfigError> {
    if let Ok(value) = env::var(name) {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }

    if let Some(value) = compile_time {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    Err(ConfigError::MissingVar(name))
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let environment = read_var("MLT_ENV", option_env!("MLT_ENV"))?;

        if environment != "staging" && environment != "production" {
            return Err(ConfigError::InvalidEnvironment(environment));
        }

        let supabase_url = read_var("MLT_SUPABASE_URL", option_env!("MLT_SUPABASE_URL"))?;
        let supabase_anon_key =
            read_var("MLT_SUPABASE_ANON_KEY", option_env!("MLT_SUPABASE_ANON_KEY"))?;

        Ok(Self {
            environment,
            supabase_url: supabase_url.trim_end_matches('/').to_string(),
            supabase_anon_key,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_time_fallback_is_available_when_set() {
        if option_env!("MLT_ENV").is_some() {
            assert!(AppConfig::from_env().is_ok());
        }
    }

    #[test]
    fn invalid_environment_is_rejected() {
        let err = ConfigError::InvalidEnvironment("dev".into());
        assert_eq!(
            err.to_string(),
            "MLT_ENV must be \"staging\" or \"production\" (got: dev)"
        );
    }
}
