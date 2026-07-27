use std::collections::HashMap;

use thiserror::Error;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeepLinkRoute {
    Open,
    ConnectFacebook,
    OpenMarketplace,
    OpenVehicleCreate,
    Pair,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDeepLink {
    pub route: DeepLinkRoute,
    pub query: HashMap<String, String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("empty URL")]
    Empty,
    #[error("unsupported URL scheme (expected mlt-desktop)")]
    InvalidScheme,
    #[error("malformed URL: {0}")]
    Malformed(String),
    #[error("unknown route: {0}")]
    UnknownRoute(String),
    #[error("invalid session parameter")]
    InvalidSession,
}

const ALLOWED_ROUTES: &[(&str, DeepLinkRoute)] = &[
    ("open", DeepLinkRoute::Open),
    ("connect-facebook", DeepLinkRoute::ConnectFacebook),
    ("open-marketplace", DeepLinkRoute::OpenMarketplace),
    ("open-vehicle-create", DeepLinkRoute::OpenVehicleCreate),
    ("pair", DeepLinkRoute::Pair),
];

fn normalize_raw_url(raw: &str) -> Result<String, ProtocolError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ProtocolError::Empty);
    }

    if trimmed.starts_with("mlt-desktop://") || trimmed.starts_with("mlt-desktop:/") {
        return Ok(trimmed.to_string());
    }

    if trimmed.starts_with("mlt-desktop:") {
        return Ok(trimmed.replacen("mlt-desktop:", "mlt-desktop://", 1));
    }

    Err(ProtocolError::InvalidScheme)
}

fn validate_session_value(value: &str) -> Result<String, ProtocolError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 128 {
        return Err(ProtocolError::InvalidSession);
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        return Err(ProtocolError::InvalidSession);
    }
    Ok(trimmed.to_string())
}

pub fn parse_deep_link(raw: &str) -> Result<ParsedDeepLink, ProtocolError> {
    let normalized = normalize_raw_url(raw)?;
    let parsed = Url::parse(&normalized).map_err(|e| ProtocolError::Malformed(e.to_string()))?;

    if parsed.scheme() != "mlt-desktop" {
        return Err(ProtocolError::InvalidScheme);
    }

    let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
    let path = parsed.path().trim_start_matches('/').to_ascii_lowercase();
    let route_key = if !host.is_empty() {
        host
    } else if !path.is_empty() {
        path.split('/').next().unwrap_or("").to_string()
    } else {
        return Err(ProtocolError::UnknownRoute(String::new()));
    };

    let route = ALLOWED_ROUTES
        .iter()
        .find(|(name, _)| *name == route_key)
        .map(|(_, route)| route.clone())
        .ok_or_else(|| ProtocolError::UnknownRoute(route_key.clone()))?;

    let mut query = HashMap::new();
    for (key, value) in parsed.query_pairs() {
        let key = key.to_ascii_lowercase();
        if key == "session" {
            query.insert(key, validate_session_value(&value)?);
        } else if key.len() <= 64 && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            query.insert(key, value.into_owned());
        }
    }

    Ok(ParsedDeepLink { route, query })
}

pub fn extract_deep_link_from_argv(argv: &[String]) -> Option<String> {
    argv.iter()
        .find(|arg| arg.starts_with("mlt-desktop:"))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_open_route() {
        let parsed = parse_deep_link("mlt-desktop://open").expect("open");
        assert_eq!(parsed.route, DeepLinkRoute::Open);
        assert!(parsed.query.is_empty());
    }

    #[test]
    fn parses_connect_facebook_with_session() {
        let parsed =
            parse_deep_link("mlt-desktop://connect-facebook?session=abc123_def-456").expect("cf");
        assert_eq!(parsed.route, DeepLinkRoute::ConnectFacebook);
        assert_eq!(parsed.query.get("session"), Some(&"abc123_def-456".to_string()));
    }

    #[test]
    fn rejects_unknown_route() {
        assert_eq!(
            parse_deep_link("mlt-desktop://delete-everything"),
            Err(ProtocolError::UnknownRoute("delete-everything".into()))
        );
    }

    #[test]
    fn rejects_malformed_session() {
        assert_eq!(
            parse_deep_link("mlt-desktop://pair?session=bad value"),
            Err(ProtocolError::InvalidSession)
        );
    }

    #[test]
    fn rejects_non_mlt_scheme() {
        assert_eq!(
            parse_deep_link("https://example.com/open"),
            Err(ProtocolError::InvalidScheme)
        );
    }

    #[test]
    fn extracts_url_from_argv() {
        let argv = vec![
            "/Applications/MLT.app/Contents/MacOS/mlt".into(),
            "mlt-desktop://connect-facebook?session=xyz".into(),
        ];
        assert_eq!(
            extract_deep_link_from_argv(&argv),
            Some("mlt-desktop://connect-facebook?session=xyz".into())
        );
    }

    #[test]
    fn parses_host_style_routes() {
        let parsed = parse_deep_link("mlt-desktop://open-marketplace").expect("marketplace");
        assert_eq!(parsed.route, DeepLinkRoute::OpenMarketplace);
    }
}
