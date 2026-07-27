mod deep_link_setup;
mod parser;

pub use deep_link_setup::{
    enqueue_startup_deep_links, listen_for_deep_links, register_deep_links_if_supported,
    should_register_deep_links_at_runtime,
};
pub use parser::{
    extract_deep_link_from_argv, parse_deep_link, DeepLinkRoute, ParsedDeepLink, ProtocolError,
};
