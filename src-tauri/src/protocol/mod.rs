mod parser;

pub use parser::{
    extract_deep_link_from_argv, parse_deep_link, DeepLinkRoute, ParsedDeepLink, ProtocolError,
};
