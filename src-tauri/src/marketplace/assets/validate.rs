pub const MAX_IMAGE_BYTES: usize = 15 * 1024 * 1024;

pub fn detect_mime(data: &[u8]) -> Option<&'static str> {
    if data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
        return Some("image/jpeg");
    }
    if data.len() >= 8
        && data[0..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
    {
        return Some("image/png");
    }
    if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if data.len() >= 6 && (data[0..6] == *b"GIF87a" || data[0..6] == *b"GIF89a") {
        return Some("image/gif");
    }
    None
}

pub fn extension_for_mime(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "bin",
    }
}

pub fn validate_image_bytes(data: &[u8]) -> Result<&'static str, String> {
    if data.is_empty() {
        return Err("image is empty".into());
    }
    if data.len() > MAX_IMAGE_BYTES {
        return Err(format!("image exceeds {} byte limit", MAX_IMAGE_BYTES));
    }
    detect_mime(data).ok_or_else(|| "unsupported or corrupt image data".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_jpeg_magic_bytes() {
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0x00];
        assert_eq!(validate_image_bytes(&jpeg).unwrap(), "image/jpeg");
    }

    #[test]
    fn rejects_html_error_page_body() {
        let html = b"<!DOCTYPE html><html>";
        assert!(validate_image_bytes(html).is_err());
    }
}
