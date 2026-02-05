use base64::{engine::general_purpose, Engine as _};
use std::path::Path;

pub struct AudioProcessor;

const MAX_SIZE: usize = 15 * 1024 * 1024; // 15MB

impl AudioProcessor {
    /// Detect MIME type from file magic bytes (signature)
    #[allow(clippy::missing_asserts_for_indexing, reason = "Length checked at function start")]
    pub fn detect_mime_type_from_bytes(data: &[u8]) -> Option<String> {
        if data.len() < 12 {
            return None;
        }

        // MP3: starts with ID3 tag or frame sync
        if data.starts_with(b"ID3") || (data[0] == 0xFF && (data[1] & 0xE0) == 0xE0) {
            return Some("audio/mp3".to_string());
        }

        // WAV: RIFF....WAVE
        if data.starts_with(b"RIFF") && &data[8..12] == b"WAVE" {
            return Some("audio/wav".to_string());
        }

        // FLAC: fLaC
        if data.starts_with(b"fLaC") {
            return Some("audio/flac".to_string());
        }

        // OGG: OggS
        if data.starts_with(b"OggS") {
            return Some("audio/ogg".to_string());
        }

        // AIFF: FORM....AIFF
        if data.starts_with(b"FORM") && &data[8..12] == b"AIFF" {
            return Some("audio/aiff".to_string());
        }

        // M4A/AAC: ftyp (ISO Base Media)
        if &data[4..8] == b"ftyp" {
            return Some("audio/aac".to_string());
        }

        None
    }

    /// Detect MIME type from filename extension (fallback)
    pub fn detect_mime_type_from_extension(filename: &str) -> Result<String, String> {
        let ext = Path::new(filename)
            .extension()
            .and_then(|s| s.to_str())
            .ok_or("Failed to get file extension")?;

        match ext.to_lowercase().as_str() {
            "mp3" => Ok("audio/mp3".to_string()),
            "wav" => Ok("audio/wav".to_string()),
            "m4a" => Ok("audio/aac".to_string()),
            "ogg" => Ok("audio/ogg".to_string()),
            "flac" => Ok("audio/flac".to_string()),
            "aiff" | "aif" => Ok("audio/aiff".to_string()),
            _ => Err(format!("Unsupported audio format: {}", ext)),
        }
    }

    /// Detect MIME type: prioritize magic bytes, fallback to extension
    pub fn detect_mime_type(filename: &str, data: &[u8]) -> Result<String, String> {
        if let Some(mime) = Self::detect_mime_type_from_bytes(data) {
            return Ok(mime);
        }
        Self::detect_mime_type_from_extension(filename)
    }

    /// Get max file size limit in bytes
    pub const fn max_size_bytes() -> usize {
        MAX_SIZE
    }

    /// Encode audio data to Base64
    pub fn encode_to_base64(audio_data: &[u8]) -> String {
        general_purpose::STANDARD.encode(audio_data)
    }

    /// Check if file exceeds size limit
    pub fn exceeds_size_limit(size_bytes: usize) -> bool {
        size_bytes > Self::max_size_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_mime_type_from_extension() {
        assert_eq!(
            AudioProcessor::detect_mime_type_from_extension("audio.mp3").unwrap(),
            "audio/mp3"
        );
        assert_eq!(
            AudioProcessor::detect_mime_type_from_extension("audio.wav").unwrap(),
            "audio/wav"
        );
        let _ = AudioProcessor::detect_mime_type_from_extension("audio.txt").unwrap_err();
    }

    #[test]
    fn test_detect_mime_type_from_bytes() {
        // MP3 with ID3 tag
        let mp3_id3 = b"ID3\x04\x00\x00\x00\x00\x00\x00\x00\x00";
        assert_eq!(
            AudioProcessor::detect_mime_type_from_bytes(mp3_id3),
            Some("audio/mp3".to_string())
        );

        // WAV file
        let wav = b"RIFF\x00\x00\x00\x00WAVEfmt ";
        assert_eq!(AudioProcessor::detect_mime_type_from_bytes(wav), Some("audio/wav".to_string()));

        // FLAC file
        let flac = b"fLaC\x00\x00\x00\x00\x00\x00\x00\x00";
        assert_eq!(
            AudioProcessor::detect_mime_type_from_bytes(flac),
            Some("audio/flac".to_string())
        );

        // OGG file
        let ogg = b"OggS\x00\x00\x00\x00\x00\x00\x00\x00";
        assert_eq!(AudioProcessor::detect_mime_type_from_bytes(ogg), Some("audio/ogg".to_string()));

        // Unknown format
        let unknown = b"UNKNOWN_FORMAT__";
        assert_eq!(AudioProcessor::detect_mime_type_from_bytes(unknown), None);
    }

    #[test]
    fn test_detect_mime_type_combined() {
        // Magic bytes take priority over extension
        let wav_data = b"RIFF\x00\x00\x00\x00WAVEfmt ";
        assert_eq!(AudioProcessor::detect_mime_type("fake.mp3", wav_data).unwrap(), "audio/wav");

        // Fallback to extension when magic bytes unknown
        let unknown = b"UNKNOWN_FORMAT__";
        assert_eq!(AudioProcessor::detect_mime_type("audio.flac", unknown).unwrap(), "audio/flac");
    }

    #[test]
    fn test_exceeds_size_limit() {
        assert!(!AudioProcessor::exceeds_size_limit(10 * 1024 * 1024));
        assert!(AudioProcessor::exceeds_size_limit(20 * 1024 * 1024));
        assert!(AudioProcessor::exceeds_size_limit(15 * 1024 * 1024 + 1));
        assert!(!AudioProcessor::exceeds_size_limit(15 * 1024 * 1024));
    }

    #[test]
    fn test_base64_encoding() {
        let data = b"test audio data";
        let encoded = AudioProcessor::encode_to_base64(data);
        assert!(!encoded.is_empty());
    }
}
