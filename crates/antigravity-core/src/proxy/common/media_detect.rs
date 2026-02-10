use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use tracing::debug;

pub fn detect_image_mime(base64_data: &str, declared: &str) -> String {
    let data = base64_data.trim();
    let mut prefix_len = data.len().min(24);
    prefix_len -= prefix_len % 4;
    if prefix_len == 0 {
        return declared.to_string();
    }

    let decoded = match STANDARD.decode(&data[..prefix_len]) {
        Ok(bytes) => bytes,
        Err(_) => return declared.to_string(),
    };

    let detected = detect_from_bytes(&decoded).unwrap_or(declared);
    if detected != declared {
        debug!(declared = declared, detected = detected, "Overriding image MIME type");
    }
    detected.to_string()
}

fn detect_from_bytes(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 4 && bytes[..4] == [0x89, 0x50, 0x4E, 0x47] {
        return Some("image/png");
    }
    if bytes.len() >= 3 && bytes[..3] == [0xFF, 0xD8, 0xFF] {
        return Some("image/jpeg");
    }
    if bytes.len() >= 4 && bytes[..4] == *b"GIF8" {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && bytes[..4] == *b"RIFF" && bytes[8..12] == *b"WEBP" {
        return Some("image/webp");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::detect_image_mime;

    #[test]
    fn jpeg_overrides_declared_png() {
        let jpeg = "/9j/4AAQSkZJRgAB";
        let detected = detect_image_mime(jpeg, "image/png");
        assert_eq!(detected, "image/jpeg");
    }

    #[test]
    fn png_keeps_declared_png() {
        let png = "iVBORw0KGgo=";
        let detected = detect_image_mime(png, "image/png");
        assert_eq!(detected, "image/png");
    }

    #[test]
    fn unknown_keeps_declared() {
        let unknown = "AAAA";
        let detected = detect_image_mime(unknown, "image/webp");
        assert_eq!(detected, "image/webp");
    }

    #[test]
    fn short_base64_keeps_declared() {
        let detected = detect_image_mime("", "image/gif");
        assert_eq!(detected, "image/gif");
    }
}
