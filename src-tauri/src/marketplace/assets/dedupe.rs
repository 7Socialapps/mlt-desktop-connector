/// Normalize image URL for duplicate detection (strip fragment; keep query for CDN tokens).
pub fn normalize_image_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some((base, _)) = trimmed.split_once('#') {
        base.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

/// Remove duplicate URLs while preserving first-seen order.
pub fn dedupe_image_urls(urls: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for url in urls {
        let normalized = normalize_image_url(url);
        if normalized.is_empty() {
            continue;
        }
        if seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedupe_preserves_order_and_skips_duplicates() {
        let urls = vec![
            "https://cdn.example/a.jpg".into(),
            "https://cdn.example/b.jpg".into(),
            "https://cdn.example/a.jpg#x".into(),
        ];
        let deduped = dedupe_image_urls(&urls);
        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0], "https://cdn.example/a.jpg");
        assert_eq!(deduped[1], "https://cdn.example/b.jpg");
    }
}
