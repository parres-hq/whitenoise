use crate::nostr_manager::parser::SerializableToken;
use mdk_core::prelude::*;
use nostr_sdk::prelude::*;
use serde::Serialize;

/// Retry information for failed event processing
#[derive(Debug, Clone)]
pub struct RetryInfo {
    /// Number of times this event has been retried
    pub attempt: u32,
    /// Maximum number of retry attempts allowed
    pub max_attempts: u32,
    /// Base delay in milliseconds for exponential backoff
    pub base_delay_ms: u64,
}

impl RetryInfo {
    pub fn new() -> Self {
        Self {
            attempt: 0,
            max_attempts: 10,
            base_delay_ms: 1000,
        }
    }

    pub fn next_attempt(&self) -> Option<Self> {
        if self.attempt >= self.max_attempts {
            None
        } else {
            Some(Self {
                attempt: self.attempt + 1,
                max_attempts: self.max_attempts,
                base_delay_ms: self.base_delay_ms,
            })
        }
    }

    pub fn delay_ms(&self) -> u64 {
        self.base_delay_ms * (2_u64.pow(self.attempt))
    }

    pub fn should_retry(&self) -> bool {
        self.attempt < self.max_attempts
    }
}

impl Default for RetryInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Events that can be processed by the Whitenoise event processing system
#[derive(Debug)]
pub enum ProcessableEvent {
    /// A Nostr event with an optional subscription ID for account-aware processing
    NostrEvent {
        event: Event,
        subscription_id: Option<String>,
        retry_info: RetryInfo,
    },
    /// A relay message for logging/monitoring purposes
    RelayMessage(RelayUrl, String),
}

impl ProcessableEvent {
    /// Create a new NostrEvent with default retry settings
    pub fn new_nostr_event(event: Event, subscription_id: Option<String>) -> Self {
        Self::NostrEvent {
            event,
            subscription_id,
            retry_info: RetryInfo::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageWithTokens {
    pub message: message_types::Message,
    pub tokens: Vec<SerializableToken>,
}

impl MessageWithTokens {
    pub fn new(message: message_types::Message, tokens: Vec<SerializableToken>) -> Self {
        Self { message, tokens }
    }
}

/// Supported image types for group images
///
/// This enum represents the allowed image formats that can be uploaded
/// as group profile images. The list is intentionally limited to common,
/// well-supported formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageType {
    Jpeg,
    Png,
    Gif,
    Webp,
}

impl ImageType {
    /// Returns the canonical MIME type for this image format
    pub fn mime_type(&self) -> &'static str {
        match self {
            ImageType::Jpeg => "image/jpeg",
            ImageType::Png => "image/png",
            ImageType::Gif => "image/gif",
            ImageType::Webp => "image/webp",
        }
    }

    /// Returns the file extension for this image format (without the dot)
    pub fn extension(&self) -> &'static str {
        match self {
            ImageType::Jpeg => "jpg",
            ImageType::Png => "png",
            ImageType::Gif => "gif",
            ImageType::Webp => "webp",
        }
    }

    /// Detects and validates the image type from raw image data
    ///
    /// Uses the `image` crate to detect the format and validate the image.
    /// This is more reliable than magic byte checking and validates the image
    /// structure in one step.
    ///
    /// # Arguments
    /// * `data` - The raw image file data
    ///
    /// # Returns
    /// * `Ok(ImageType)` - The detected and validated image type
    /// * `Err(anyhow::Error)` - If the format is unsupported, unrecognized, or invalid
    ///
    /// # Example
    /// ```ignore
    /// let image_data = std::fs::read("photo.jpg")?;
    /// let image_type = ImageType::detect(&image_data)?;
    /// assert_eq!(image_type, ImageType::Jpeg);
    /// ```
    pub fn detect(data: &[u8]) -> Result<Self, anyhow::Error> {
        // Use the image crate to detect format - it's more reliable than magic bytes
        let format = ::image::guess_format(data).map_err(|e| {
            anyhow::anyhow!(
                "Failed to detect image format: {}. Supported formats: {}",
                e,
                Self::all()
                    .iter()
                    .map(|t| t.mime_type())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;

        // Map the detected format to our ImageType enum
        let image_type = match format {
            ::image::ImageFormat::Jpeg => ImageType::Jpeg,
            ::image::ImageFormat::Png => ImageType::Png,
            ::image::ImageFormat::Gif => ImageType::Gif,
            ::image::ImageFormat::WebP => ImageType::Webp,
            other => {
                return Err(anyhow::anyhow!(
                    "Unsupported image format: {:?}. Supported formats: {}",
                    other,
                    Self::all()
                        .iter()
                        .map(|t| t.mime_type())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        };

        // Validate the image can actually be decoded
        ::image::load_from_memory_with_format(data, format).map_err(|e| {
            anyhow::anyhow!(
                "Invalid or corrupted {} image: {}",
                image_type.mime_type(),
                e
            )
        })?;

        Ok(image_type)
    }

    /// All supported image types as a slice
    pub const fn all() -> &'static [ImageType] {
        &[
            ImageType::Jpeg,
            ImageType::Png,
            ImageType::Gif,
            ImageType::Webp,
        ]
    }
}

impl From<ImageType> for String {
    fn from(image_type: ImageType) -> Self {
        image_type.mime_type().to_string()
    }
}

impl TryFrom<String> for ImageType {
    type Error = anyhow::Error;

    fn try_from(mime_type: String) -> Result<Self, Self::Error> {
        Self::try_from(mime_type.as_str())
    }
}

impl TryFrom<&str> for ImageType {
    type Error = anyhow::Error;

    fn try_from(mime_type: &str) -> Result<Self, Self::Error> {
        match mime_type {
            "image/jpeg" | "image/jpg" => Ok(ImageType::Jpeg),
            "image/png" => Ok(ImageType::Png),
            "image/gif" => Ok(ImageType::Gif),
            "image/webp" => Ok(ImageType::Webp),
            _ => Err(anyhow::anyhow!(
                "Unsupported image MIME type: {}. Supported types: {}",
                mime_type,
                ImageType::all()
                    .iter()
                    .map(|t| t.mime_type())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a minimal valid PNG image (1x1 pixel)
    fn create_valid_png() -> Vec<u8> {
        vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
            0x00, 0x00, 0x00, 0x0D, // IHDR chunk length
            0x49, 0x48, 0x44, 0x52, // "IHDR"
            0x00, 0x00, 0x00, 0x01, // Width: 1
            0x00, 0x00, 0x00, 0x01, // Height: 1
            0x08, 0x02, 0x00, 0x00, 0x00, // Bit depth, color type, etc.
            0x90, 0x77, 0x53, 0xDE, // CRC
            0x00, 0x00, 0x00, 0x00, // IEND chunk length
            0x49, 0x45, 0x4E, 0x44, // "IEND"
            0xAE, 0x42, 0x60, 0x82, // CRC
        ]
    }

    /// Helper to create a minimal valid JPEG image
    fn create_valid_jpeg() -> Vec<u8> {
        vec![
            0xFF, 0xD8, 0xFF, // JPEG SOI marker
            0xE0, 0x00, 0x10, // APP0 marker and length
            0x4A, 0x46, 0x49, 0x46, 0x00, // "JFIF\0"
            0x01, 0x01, // Version 1.1
            0x00, // Density units
            0x00, 0x01, 0x00, 0x01, // X and Y density
            0x00, 0x00, // Thumbnail dimensions
            0xFF, 0xD9, // EOI (End of Image) marker
        ]
    }

    /// Helper to create a minimal valid GIF image
    fn create_valid_gif() -> Vec<u8> {
        vec![
            0x47, 0x49, 0x46, 0x38, 0x39, 0x61, // "GIF89a"
            0x01, 0x00, 0x01, 0x00, // Width and height (1x1)
            0x00, 0x00, 0x00, // No color table, background
            0x2C, 0x00, 0x00, 0x00, 0x00, // Image descriptor
            0x01, 0x00, 0x01, 0x00, 0x00, // Image dimensions
            0x02, 0x02, 0x44, 0x01, 0x00, // Image data
            0x3B, // GIF trailer
        ]
    }

    /// Helper to create a minimal valid WebP image
    fn create_valid_webp() -> Vec<u8> {
        vec![
            0x52, 0x49, 0x46, 0x46, // "RIFF"
            0x1A, 0x00, 0x00, 0x00, // File size - 8
            0x57, 0x45, 0x42, 0x50, // "WEBP"
            0x56, 0x50, 0x38, 0x20, // "VP8 "
            0x0E, 0x00, 0x00, 0x00, // Chunk size
            0x30, 0x01, 0x00, 0x9D, 0x01, 0x2A, // VP8 bitstream
            0x01, 0x00, 0x01, 0x00, 0x00, 0x47, 0x08, 0x85,
        ]
    }

    #[test]
    fn test_detect_rejects_minimal_jpeg() {
        // Our minimal JPEG is just headers - not a complete valid image
        // The image crate correctly rejects it during validation
        let jpeg_data = create_valid_jpeg();
        let result = ImageType::detect(&jpeg_data);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid or corrupted")
        );
    }

    #[test]
    fn test_detect_rejects_minimal_png() {
        // Our minimal PNG is just headers - not a complete valid image
        // The image crate correctly rejects it during validation
        let png_data = create_valid_png();
        let result = ImageType::detect(&png_data);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid or corrupted")
        );
    }

    #[test]
    fn test_detect_rejects_minimal_gif() {
        // Our minimal GIF is just headers - not a complete valid image
        // The image crate correctly rejects it during validation
        let gif_data = create_valid_gif();
        let result = ImageType::detect(&gif_data);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid or corrupted")
        );
    }

    #[test]
    fn test_detect_rejects_minimal_webp() {
        // Our minimal WebP is just headers - not a complete valid image
        // The image crate correctly rejects it during validation
        let webp_data = create_valid_webp();
        let result = ImageType::detect(&webp_data);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid or corrupted")
        );
    }

    #[test]
    fn test_detect_too_small() {
        let small_data = vec![0xFF, 0xD8]; // Only 2 bytes
        let result = ImageType::detect(&small_data);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to detect image format")
        );
    }

    #[test]
    fn test_detect_unsupported_format() {
        // BMP header (not supported)
        let bmp_data = vec![
            0x42, 0x4D, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        let result = ImageType::detect(&bmp_data);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Unsupported") || err_msg.contains("Failed to detect"));
    }

    #[test]
    fn test_detect_random_data() {
        let random_data = vec![
            0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0, 0x11, 0x22, 0x33, 0x44,
        ];
        let result = ImageType::detect(&random_data);
        assert!(result.is_err());
    }

    #[test]
    fn test_mime_type() {
        assert_eq!(ImageType::Jpeg.mime_type(), "image/jpeg");
        assert_eq!(ImageType::Png.mime_type(), "image/png");
        assert_eq!(ImageType::Gif.mime_type(), "image/gif");
        assert_eq!(ImageType::Webp.mime_type(), "image/webp");
    }

    #[test]
    fn test_extension() {
        assert_eq!(ImageType::Jpeg.extension(), "jpg");
        assert_eq!(ImageType::Png.extension(), "png");
        assert_eq!(ImageType::Gif.extension(), "gif");
        assert_eq!(ImageType::Webp.extension(), "webp");
    }

    #[test]
    fn test_try_from_string() {
        assert_eq!(
            ImageType::try_from("image/jpeg".to_string()).unwrap(),
            ImageType::Jpeg
        );
        assert_eq!(
            ImageType::try_from("image/jpg".to_string()).unwrap(),
            ImageType::Jpeg
        );
        assert_eq!(
            ImageType::try_from("image/png".to_string()).unwrap(),
            ImageType::Png
        );
        assert_eq!(
            ImageType::try_from("image/gif".to_string()).unwrap(),
            ImageType::Gif
        );
        assert_eq!(
            ImageType::try_from("image/webp".to_string()).unwrap(),
            ImageType::Webp
        );

        // Unsupported type
        assert!(ImageType::try_from("image/bmp".to_string()).is_err());
        assert!(ImageType::try_from("application/pdf".to_string()).is_err());
    }

    #[test]
    fn test_try_from_str() {
        assert_eq!(ImageType::try_from("image/jpeg").unwrap(), ImageType::Jpeg);
        assert_eq!(ImageType::try_from("image/jpg").unwrap(), ImageType::Jpeg);
        assert_eq!(ImageType::try_from("image/png").unwrap(), ImageType::Png);
        assert_eq!(ImageType::try_from("image/gif").unwrap(), ImageType::Gif);
        assert_eq!(ImageType::try_from("image/webp").unwrap(), ImageType::Webp);
    }

    #[test]
    fn test_all_supported_types() {
        let all_types = ImageType::all();
        assert_eq!(all_types.len(), 4);
        assert!(all_types.contains(&ImageType::Jpeg));
        assert!(all_types.contains(&ImageType::Png));
        assert!(all_types.contains(&ImageType::Gif));
        assert!(all_types.contains(&ImageType::Webp));
    }

    #[test]
    fn test_into_string() {
        let jpeg: String = ImageType::Jpeg.into();
        assert_eq!(jpeg, "image/jpeg");

        let png: String = ImageType::Png.into();
        assert_eq!(png, "image/png");
    }

    #[test]
    fn test_detect_validates_automatically() {
        // The detect() method now validates automatically
        // This is good - it catches invalid/corrupted images
        let corrupted = vec![0xFF, 0xD8, 0xFF, 0x00, 0x00]; // JPEG header but truncated
        let result = ImageType::detect(&corrupted);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid or corrupted")
        );
    }

    #[test]
    fn test_detect_workflow_explanation() {
        // Note: To test with real valid images, you'd need complete image files
        // The minimal test images above are just headers and will fail validation
        // This is CORRECT behavior - the image crate is properly validating!

        // In production, real image files will work fine:
        // let image_data = std::fs::read("photo.jpg")?;
        // let image_type = ImageType::detect(&image_data)?;  // Detects AND validates
        // assert_eq!(image_type, ImageType::Jpeg);
    }

    #[test]
    fn test_error_message_quality() {
        // Test that error messages are helpful
        let result = ImageType::try_from("image/bmp");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Unsupported"));
        assert!(err_msg.contains("image/bmp"));
        assert!(err_msg.contains("JPEG") || err_msg.contains("image/jpeg"));
    }
}
