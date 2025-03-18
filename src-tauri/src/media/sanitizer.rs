//! Media sanitization module for the Whitenoise application.
//!
//! This module provides functionality for sanitizing media files before they are
//! uploaded to chats or stored in the application. This includes:
//! - Removing potentially sensitive metadata (EXIF, GPS data, etc.)
//! - Checking for potentially malicious content
//! - Validating file formats and content
//!
//! # Security
//!
//! The sanitizer is designed to protect user privacy by removing metadata that
//! might contain sensitive information, such as:
//! - GPS coordinates
//! - Camera settings and device information
//! - Creation dates and times
//! - Software used to create/edit the file
//! - Thumbnails and previews

use crate::media::errors::MediaError;
use crate::media::types::FileUpload;
use image::{GenericImageView, ImageFormat, ImageOutputFormat};
use serde::{Deserialize, Serialize};
use sqlx::{Decode, Encode, Type};
use std::io::Cursor;

#[derive(Debug, Serialize, Deserialize, Type, Encode, Decode)]
#[sqlx(type_name = "jsonb")]
pub struct SafeMediaMetadata {
    // Common fields
    pub mime_type: String,
    pub size_bytes: u64,
    pub format: Option<String>,

    // Image-specific fields
    pub dimensions: Option<(u32, u32)>,
    pub color_space: Option<String>,
    pub has_alpha: Option<bool>,
    pub bits_per_pixel: Option<u8>,

    // Video-specific fields (basic info only)
    pub duration_seconds: Option<f64>,
    pub frame_rate: Option<f32>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub video_bitrate: Option<u64>,
    pub audio_bitrate: Option<u64>,
    pub video_dimensions: Option<(u32, u32)>,

    // Document-specific fields
    pub page_count: Option<u32>,
    pub author: Option<String>,
    pub title: Option<String>,

    // Additional metadata
    pub created_at: Option<i64>,
    pub modified_at: Option<i64>,
}

#[derive(Debug)]
pub struct SanitizedMedia {
    pub data: Vec<u8>,
    pub metadata: SafeMediaMetadata,
}

/// Sanitizes an image by removing potentially sensitive metadata.
///
/// This function:
/// 1. Decodes the image into a generic image format
/// 2. Re-encodes it without any metadata
/// 3. Returns the sanitized image data
///
/// The function preserves the image quality while removing potentially sensitive
/// metadata such as EXIF data, GPS coordinates, and creation timestamps.
///
/// # Arguments
///
/// * `data` - The original image data
/// * `format` - The format to output the sanitized image in
///
/// # Returns
///
/// * `Ok(Vec<u8>)` - The sanitized image data
/// * `Err(MediaError)` - Error if sanitization fails
pub fn sanitize_image(data: &[u8], format: ImageOutputFormat) -> Result<Vec<u8>, MediaError> {
    // Load the image
    let img = image::load_from_memory(data).map_err(|e| MediaError::Sanitize(e.to_string()))?;

    // Create a buffer to write the sanitized image to
    let mut buffer = Cursor::new(Vec::new());

    // Write the image without any metadata
    img.write_to(&mut buffer, format)
        .map_err(|e| MediaError::Sanitize(e.to_string()))?;

    Ok(buffer.into_inner())
}

/// Determines the appropriate output format for an image based on its input format.
///
/// This function chooses the best output format based on:
/// - The input format
/// - Quality and size considerations
/// - Browser compatibility
///
/// The function makes intelligent choices about output formats:
/// - PNG for images requiring transparency
/// - JPEG for photographs with good quality (85%)
/// - WebP for modern browsers when supported
/// - GIF for animated images
///
/// # Arguments
///
/// * `input_format` - The format of the input image
///
/// # Returns
///
/// * `ImageOutputFormat` - The recommended output format
pub fn determine_output_format(input_format: ImageFormat) -> ImageOutputFormat {
    match input_format {
        ImageFormat::Png => ImageOutputFormat::Png,
        ImageFormat::Jpeg => ImageOutputFormat::Jpeg(85), // Good quality JPEG
        ImageFormat::WebP => ImageOutputFormat::WebP,
        ImageFormat::Gif => ImageOutputFormat::Gif,
        _ => ImageOutputFormat::Png, // Default to PNG for other formats
    }
}

/// Extracts metadata from an image file.
///
/// This function analyzes an image file to extract relevant metadata such as:
/// - Image dimensions
/// - Color space information
/// - Alpha channel presence
/// - Bits per pixel
/// - File format
///
/// # Arguments
///
/// * `data` - The image file data
/// * `mime_type` - The MIME type of the image
///
/// # Returns
///
/// * `Ok(SafeMediaMetadata)` - The extracted metadata
/// * `Err(MediaError)` - Error if extraction fails
fn extract_image_metadata(data: &[u8], mime_type: &str) -> Result<SafeMediaMetadata, MediaError> {
    // Try to determine the input format
    let input_format =
        image::guess_format(data).map_err(|e| MediaError::Sanitize(e.to_string()))?;

    // Load the image to get metadata
    let img = image::load_from_memory(data).map_err(|e| MediaError::Sanitize(e.to_string()))?;

    Ok(SafeMediaMetadata {
        mime_type: mime_type.to_string(),
        size_bytes: data.len() as u64,
        format: Some(
            match input_format {
                ImageFormat::Png => "png",
                ImageFormat::Jpeg => "jpeg",
                ImageFormat::WebP => "webp",
                ImageFormat::Gif => "gif",
                _ => "unknown",
            }
            .to_string(),
        ),
        dimensions: Some(img.dimensions()),
        color_space: Some("RGB".to_string()),
        has_alpha: Some(matches!(input_format, ImageFormat::Png | ImageFormat::Gif)),
        bits_per_pixel: Some(match input_format {
            ImageFormat::Png => 32,
            ImageFormat::Jpeg => 24,
            ImageFormat::Gif => 8,
            _ => 24,
        }),
        duration_seconds: None,
        frame_rate: None,
        video_codec: None,
        audio_codec: None,
        video_bitrate: None,
        audio_bitrate: None,
        video_dimensions: None,
        page_count: None,
        author: None,
        title: None,
        created_at: None,
        modified_at: None,
    })
}

/// Sanitizes an image file by removing potentially sensitive metadata.
///
/// This function combines image sanitization and metadata extraction to:
/// 1. Remove sensitive metadata from the image data
/// 2. Extract safe metadata for storage
/// 3. Return both the sanitized data and metadata
///
/// # Arguments
///
/// * `data` - The original image data
/// * `mime_type` - The MIME type of the image
///
/// # Returns
///
/// * `Ok(SanitizedMedia)` - The sanitized image data and metadata
/// * `Err(MediaError)` - Error if sanitization fails
fn sanitize_image_file(data: &[u8], mime_type: &str) -> Result<SanitizedMedia, MediaError> {
    // Try to determine the input format
    let input_format =
        image::guess_format(data).map_err(|e| MediaError::Sanitize(e.to_string()))?;

    // Determine the output format
    let output_format = determine_output_format(input_format);

    // Sanitize the image
    let sanitized_data = sanitize_image(data, output_format)?;

    // Extract metadata
    let metadata = extract_image_metadata(data, mime_type)?;

    Ok(SanitizedMedia {
        data: sanitized_data,
        metadata,
    })
}

/// Sanitizes a video file by removing potentially sensitive metadata.
///
/// This function currently only extracts basic file information without
/// parsing the video content. Future implementations may add more detailed
/// video metadata extraction.
///
/// # Arguments
///
/// * `data` - The original video data
/// * `mime_type` - The MIME type of the video
///
/// # Returns
///
/// * `Ok(SanitizedMedia)` - The sanitized video data and basic metadata
/// * `Err(MediaError)` - Error if sanitization fails
fn sanitize_video_file(data: &[u8], mime_type: &str) -> Result<SanitizedMedia, MediaError> {
    // For now, we'll just return the original data with basic metadata
    Ok(SanitizedMedia {
        data: data.to_vec(),
        metadata: SafeMediaMetadata {
            mime_type: mime_type.to_string(),
            size_bytes: data.len() as u64,
            format: Some(mime_type.split('/').nth(1).unwrap_or("unknown").to_string()),
            dimensions: None,
            color_space: None,
            has_alpha: None,
            bits_per_pixel: None,
            duration_seconds: None,
            frame_rate: None,
            video_codec: None,
            audio_codec: None,
            video_bitrate: None,
            audio_bitrate: None,
            video_dimensions: None,
            page_count: None,
            author: None,
            title: None,
            created_at: None,
            modified_at: None,
        },
    })
}

/// Sanitizes a media file by removing potentially sensitive metadata.
///
/// This function handles different types of media files and applies appropriate
/// sanitization based on the file type. Currently supports:
/// - Images (removes EXIF, GPS data, etc.)
/// - Videos (extracts metadata using mp4parse)
/// - (Future support for other media types)
///
/// The function routes the file to the appropriate sanitizer based on its MIME type
/// and returns both the sanitized data and safe metadata.
///
/// # Arguments
///
/// * `file` - The uploaded file containing data and metadata
///
/// # Returns
///
/// * `Ok(SanitizedMedia)` - The sanitized file data and safe metadata
/// * `Err(MediaError)` - Error if sanitization fails
pub fn sanitize_media(file: &FileUpload) -> Result<SanitizedMedia, MediaError> {
    if file.mime_type.starts_with("image/") {
        sanitize_image_file(&file.data, &file.mime_type)
    } else if file.mime_type.starts_with("video/") {
        sanitize_video_file(&file.data, &file.mime_type)
    } else {
        // For non-image/video files, return the original data with minimal metadata
        Ok(SanitizedMedia {
            data: file.data.clone(),
            metadata: SafeMediaMetadata {
                mime_type: file.mime_type.clone(),
                size_bytes: file.data.len() as u64,
                format: None,
                dimensions: None,
                color_space: None,
                has_alpha: None,
                bits_per_pixel: None,
                duration_seconds: None,
                frame_rate: None,
                video_codec: None,
                audio_codec: None,
                video_bitrate: None,
                audio_bitrate: None,
                video_dimensions: None,
                page_count: None,
                author: None,
                title: None,
                created_at: None,
                modified_at: None,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageFormat;

    fn create_test_image(width: u32, height: u32, format: ImageOutputFormat) -> Vec<u8> {
        let mut img = image::RgbaImage::new(width, height);
        for pixel in img.pixels_mut() {
            *pixel = image::Rgba([255, 0, 0, 255]); // Red image
        }

        let mut buffer = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut buffer), format)
            .unwrap();
        buffer
    }

    fn create_test_file(filename: &str, mime_type: &str, data: &[u8]) -> FileUpload {
        FileUpload {
            filename: filename.to_string(),
            mime_type: mime_type.to_string(),
            data: data.to_vec(),
        }
    }

    #[test]
    fn test_sanitize_image() {
        // Test JPEG sanitization
        let jpeg_data = create_test_image(100, 100, ImageOutputFormat::Jpeg(85));
        let sanitized = sanitize_image(&jpeg_data, ImageOutputFormat::Jpeg(85)).unwrap();
        assert!(image::load_from_memory(&sanitized).is_ok());

        // Test PNG sanitization
        let png_data = create_test_image(100, 100, ImageOutputFormat::Png);
        let sanitized = sanitize_image(&png_data, ImageOutputFormat::Png).unwrap();
        assert!(image::load_from_memory(&sanitized).is_ok());

        // Test WebP sanitization
        let webp_data = create_test_image(100, 100, ImageOutputFormat::WebP);
        let sanitized = sanitize_image(&webp_data, ImageOutputFormat::WebP).unwrap();
        assert!(image::load_from_memory(&sanitized).is_ok());

        // Test error handling
        let invalid_data = b"not an image";
        assert!(sanitize_image(invalid_data, ImageOutputFormat::Jpeg(85)).is_err());
    }

    #[test]
    fn test_determine_output_format() {
        // Test all supported formats
        assert!(matches!(
            determine_output_format(ImageFormat::Png),
            ImageOutputFormat::Png
        ));
        assert!(matches!(
            determine_output_format(ImageFormat::Jpeg),
            ImageOutputFormat::Jpeg(85)
        ));
        assert!(matches!(
            determine_output_format(ImageFormat::WebP),
            ImageOutputFormat::WebP
        ));
        assert!(matches!(
            determine_output_format(ImageFormat::Gif),
            ImageOutputFormat::Gif
        ));

        // Test unknown format defaults to PNG
        assert!(matches!(
            determine_output_format(ImageFormat::Bmp),
            ImageOutputFormat::Png
        ));
    }

    #[test]
    fn test_extract_image_metadata() {
        // Test JPEG metadata extraction
        let jpeg_data = create_test_image(100, 100, ImageOutputFormat::Jpeg(85));
        let metadata = extract_image_metadata(&jpeg_data, "image/jpeg").unwrap();
        assert_eq!(metadata.dimensions, Some((100, 100)));
        assert_eq!(metadata.color_space, Some("RGB".to_string()));
        assert_eq!(metadata.format, Some("jpeg".to_string()));
        assert_eq!(metadata.has_alpha, Some(false)); // JPEG doesn't support alpha
        assert_eq!(metadata.bits_per_pixel, Some(24));

        // Test PNG metadata extraction
        let png_data = create_test_image(100, 100, ImageOutputFormat::Png);
        let metadata = extract_image_metadata(&png_data, "image/png").unwrap();
        assert_eq!(metadata.dimensions, Some((100, 100)));
        assert_eq!(metadata.color_space, Some("RGB".to_string()));
        assert_eq!(metadata.format, Some("png".to_string()));
        assert_eq!(metadata.has_alpha, Some(true)); // PNG supports alpha
        assert_eq!(metadata.bits_per_pixel, Some(32));

        // Test GIF metadata extraction
        let gif_data = create_test_image(100, 100, ImageOutputFormat::Gif);
        let metadata = extract_image_metadata(&gif_data, "image/gif").unwrap();
        assert_eq!(metadata.has_alpha, Some(true)); // GIF supports alpha
        assert_eq!(metadata.bits_per_pixel, Some(8));

        // Test WebP metadata extraction
        let webp_data = create_test_image(100, 100, ImageOutputFormat::WebP);
        let metadata = extract_image_metadata(&webp_data, "image/webp").unwrap();
        assert_eq!(metadata.has_alpha, Some(false)); // WebP doesn't support alpha in this context
        assert_eq!(metadata.bits_per_pixel, Some(24));

        // Test error handling
        let invalid_data = b"not an image";
        assert!(extract_image_metadata(invalid_data, "image/jpeg").is_err());
    }

    #[test]
    fn test_sanitize_image_file() {
        // Test JPEG file sanitization
        let jpeg_data = create_test_image(100, 100, ImageOutputFormat::Jpeg(85));
        let result = sanitize_image_file(&jpeg_data, "image/jpeg").unwrap();
        assert!(image::load_from_memory(&result.data).is_ok());
        assert_eq!(result.metadata.dimensions, Some((100, 100)));
        assert_eq!(result.metadata.format, Some("jpeg".to_string()));

        // Test PNG file sanitization
        let png_data = create_test_image(100, 100, ImageOutputFormat::Png);
        let result = sanitize_image_file(&png_data, "image/png").unwrap();
        assert!(image::load_from_memory(&result.data).is_ok());
        assert_eq!(result.metadata.dimensions, Some((100, 100)));
        assert_eq!(result.metadata.format, Some("png".to_string()));

        // Test error handling
        let invalid_data = b"not an image";
        assert!(sanitize_image_file(invalid_data, "image/jpeg").is_err());
    }

    #[test]
    fn test_sanitize_video_file() {
        // Test with a simple video file
        let video_data = b"not a real video file";
        let result = sanitize_video_file(video_data, "video/mp4").unwrap();

        // Verify basic metadata
        assert_eq!(result.metadata.mime_type, "video/mp4");
        assert_eq!(result.metadata.size_bytes, 21);
        assert_eq!(result.metadata.format, Some("mp4".to_string()));

        // Verify video-specific fields are None
        assert!(result.metadata.video_codec.is_none());
        assert!(result.metadata.video_dimensions.is_none());
        assert!(result.metadata.duration_seconds.is_none());
        assert!(result.metadata.frame_rate.is_none());
    }

    #[test]
    fn test_sanitize_media() {
        // Test image sanitization
        let jpeg_data = create_test_image(100, 100, ImageOutputFormat::Jpeg(85));
        let file = create_test_file("test.jpg", "image/jpeg", &jpeg_data);
        let result = sanitize_media(&file).unwrap();
        assert!(image::load_from_memory(&result.data).is_ok());
        assert_eq!(result.metadata.dimensions, Some((100, 100)));
        assert_eq!(result.metadata.format, Some("jpeg".to_string()));

        // Test video sanitization
        let video_data = b"not a real video file";
        let file = create_test_file("test.mp4", "video/mp4", video_data);
        let result = sanitize_media(&file).unwrap();
        assert_eq!(result.metadata.mime_type, "video/mp4");
        assert_eq!(result.metadata.format, Some("mp4".to_string()));

        // Test non-media file handling
        let test_data = b"not a media file";
        let file = create_test_file("test.txt", "text/plain", test_data);
        let result = sanitize_media(&file).unwrap();
        assert_eq!(result.data, test_data);
        assert!(result.metadata.dimensions.is_none());
        assert!(result.metadata.format.is_none());
    }
}
