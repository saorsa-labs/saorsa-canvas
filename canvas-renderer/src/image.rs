//! Image loading utilities.
//!
//! Supports loading images from files, URLs, and base64-encoded data URIs.

use crate::error::{RenderError, RenderResult};

/// Loaded texture data ready for GPU upload.
#[derive(Debug, Clone)]
pub struct TextureData {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// RGBA pixel data (4 bytes per pixel).
    pub data: Vec<u8>,
    /// Original format of the image.
    pub format: ImageFormat,
}

/// Supported image formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    /// PNG with alpha support.
    Png,
    /// JPEG (no alpha).
    Jpeg,
    /// WebP (alpha support).
    WebP,
    /// Unknown/other format.
    Unknown,
}

impl ImageFormat {
    /// Detect format from file extension.
    #[must_use]
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "png" => Self::Png,
            "jpg" | "jpeg" => Self::Jpeg,
            "webp" => Self::WebP,
            _ => Self::Unknown,
        }
    }

    /// Detect format from MIME type.
    #[must_use]
    pub fn from_mime(mime: &str) -> Self {
        match mime.to_lowercase().as_str() {
            "image/png" => Self::Png,
            "image/jpeg" | "image/jpg" => Self::Jpeg,
            "image/webp" => Self::WebP,
            _ => Self::Unknown,
        }
    }

    /// Detect format from magic bytes.
    #[must_use]
    pub fn from_magic_bytes(data: &[u8]) -> Self {
        if data.len() < 4 {
            return Self::Unknown;
        }

        // PNG: 89 50 4E 47
        if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
            return Self::Png;
        }

        // JPEG: FF D8 FF
        if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return Self::Jpeg;
        }

        // WebP: RIFF....WEBP
        if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
            return Self::WebP;
        }

        Self::Unknown
    }
}

/// Load an image from raw bytes.
///
/// # Errors
///
/// Returns an error if the image cannot be decoded.
pub fn load_image_from_bytes(data: &[u8]) -> RenderResult<TextureData> {
    let format = ImageFormat::from_magic_bytes(data);

    let img = image::load_from_memory(data)
        .map_err(|e| RenderError::Resource(format!("Failed to decode image: {e}")))?;

    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();

    Ok(TextureData {
        width,
        height,
        data: rgba.into_raw(),
        format,
    })
}

/// Load an image from a data URI (base64 encoded).
///
/// Supports formats like: `data:image/png;base64,iVBORw0KGgo...`
///
/// # Errors
///
/// Returns an error if the data URI is malformed or the image cannot be decoded.
pub fn load_image_from_data_uri(uri: &str) -> RenderResult<TextureData> {
    // Parse data URI
    if !uri.starts_with("data:") {
        return Err(RenderError::Resource("Not a data URI".to_string()));
    }

    let uri_data = &uri[5..]; // Skip "data:"

    // Find the comma separating metadata from data
    let comma_pos = uri_data
        .find(',')
        .ok_or_else(|| RenderError::Resource("Invalid data URI: missing comma".to_string()))?;

    let metadata = &uri_data[..comma_pos];
    let encoded_data = &uri_data[comma_pos + 1..];

    // Check for base64 encoding
    let is_base64 = metadata.contains(";base64");

    let bytes = if is_base64 {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(encoded_data)
            .map_err(|e| RenderError::Resource(format!("Failed to decode base64: {e}")))?
    } else {
        // URL-encoded
        urlencoding_decode(encoded_data)?
    };

    load_image_from_bytes(&bytes)
}

/// Simple URL decoding (percent-encoding).
fn urlencoding_decode(input: &str) -> RenderResult<Vec<u8>> {
    let mut result = Vec::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() == 2 {
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte);
                    continue;
                }
            }
            return Err(RenderError::Resource(
                "Invalid URL encoding".to_string(),
            ));
        }
        result.push(c as u8);
    }

    Ok(result)
}

/// Resize an image to fit within max dimensions while preserving aspect ratio.
///
/// Returns `None` if the image is already smaller than the max dimensions.
#[must_use]
pub fn resize_to_fit(texture: &TextureData, max_width: u32, max_height: u32) -> Option<TextureData> {
    if texture.width <= max_width && texture.height <= max_height {
        return None;
    }

    let scale_x = f64::from(max_width) / f64::from(texture.width);
    let scale_y = f64::from(max_height) / f64::from(texture.height);
    let scale = scale_x.min(scale_y);

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let new_width = (f64::from(texture.width) * scale) as u32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let new_height = (f64::from(texture.height) * scale) as u32;

    // Use image crate for resizing
    let img = image::RgbaImage::from_raw(texture.width, texture.height, texture.data.clone())?;

    let resized = image::imageops::resize(
        &img,
        new_width,
        new_height,
        image::imageops::FilterType::Lanczos3,
    );

    Some(TextureData {
        width: new_width,
        height: new_height,
        data: resized.into_raw(),
        format: texture.format,
    })
}

/// Create a solid color texture.
#[must_use]
pub fn create_solid_color(width: u32, height: u32, r: u8, g: u8, b: u8, a: u8) -> TextureData {
    let pixel_count = (width * height) as usize;
    let mut data = Vec::with_capacity(pixel_count * 4);

    for _ in 0..pixel_count {
        data.push(r);
        data.push(g);
        data.push(b);
        data.push(a);
    }

    TextureData {
        width,
        height,
        data,
        format: ImageFormat::Unknown,
    }
}

/// Create a placeholder texture with a checkerboard pattern.
#[must_use]
pub fn create_placeholder(width: u32, height: u32) -> TextureData {
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    let cell_size = 16u32;

    for y in 0..height {
        for x in 0..width {
            let cell_x = x / cell_size;
            let cell_y = y / cell_size;
            let is_light = (cell_x + cell_y).is_multiple_of(2);

            if is_light {
                data.extend_from_slice(&[200, 200, 200, 255]); // Light gray
            } else {
                data.extend_from_slice(&[150, 150, 150, 255]); // Dark gray
            }
        }
    }

    TextureData {
        width,
        height,
        data,
        format: ImageFormat::Unknown,
    }
}

/// Generate a thumbnail from texture data.
///
/// # Errors
///
/// Returns an error if the thumbnail cannot be generated.
pub fn generate_thumbnail(texture: &TextureData, max_size: u32) -> RenderResult<TextureData> {
    let img = image::RgbaImage::from_raw(texture.width, texture.height, texture.data.clone())
        .ok_or_else(|| RenderError::Resource("Invalid texture data".to_string()))?;

    let thumbnail = image::imageops::thumbnail(&img, max_size, max_size);
    let (w, h) = thumbnail.dimensions();

    Ok(TextureData {
        width: w,
        height: h,
        data: thumbnail.into_raw(),
        format: texture.format,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_detection_from_extension() {
        assert_eq!(ImageFormat::from_extension("png"), ImageFormat::Png);
        assert_eq!(ImageFormat::from_extension("PNG"), ImageFormat::Png);
        assert_eq!(ImageFormat::from_extension("jpg"), ImageFormat::Jpeg);
        assert_eq!(ImageFormat::from_extension("jpeg"), ImageFormat::Jpeg);
        assert_eq!(ImageFormat::from_extension("webp"), ImageFormat::WebP);
        assert_eq!(ImageFormat::from_extension("gif"), ImageFormat::Unknown);
    }

    #[test]
    fn test_format_detection_from_mime() {
        assert_eq!(ImageFormat::from_mime("image/png"), ImageFormat::Png);
        assert_eq!(ImageFormat::from_mime("image/jpeg"), ImageFormat::Jpeg);
        assert_eq!(ImageFormat::from_mime("image/webp"), ImageFormat::WebP);
    }

    #[test]
    fn test_format_detection_from_magic_bytes() {
        // PNG magic bytes
        assert_eq!(
            ImageFormat::from_magic_bytes(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]),
            ImageFormat::Png
        );

        // JPEG magic bytes
        assert_eq!(
            ImageFormat::from_magic_bytes(&[0xFF, 0xD8, 0xFF, 0xE0]),
            ImageFormat::Jpeg
        );

        // WebP magic bytes
        assert_eq!(
            ImageFormat::from_magic_bytes(b"RIFF\x00\x00\x00\x00WEBP"),
            ImageFormat::WebP
        );
    }

    #[test]
    fn test_create_solid_color() {
        let texture = create_solid_color(2, 2, 255, 0, 0, 255);
        assert_eq!(texture.width, 2);
        assert_eq!(texture.height, 2);
        assert_eq!(texture.data.len(), 16);
        // First pixel should be red
        assert_eq!(&texture.data[0..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn test_create_placeholder() {
        let texture = create_placeholder(32, 32);
        assert_eq!(texture.width, 32);
        assert_eq!(texture.height, 32);
        assert_eq!(texture.data.len(), 32 * 32 * 4);
    }

    #[test]
    fn test_data_uri_parsing() {
        // Create a minimal valid PNG (1x1 red pixel)
        let png_base64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==";
        let data_uri = format!("data:image/png;base64,{png_base64}");

        let result = load_image_from_data_uri(&data_uri);
        assert!(result.is_ok(), "Should parse valid data URI");

        let texture = result.unwrap();
        assert_eq!(texture.width, 1);
        assert_eq!(texture.height, 1);
        assert_eq!(texture.format, ImageFormat::Png);
    }

    #[test]
    fn test_invalid_data_uri() {
        let result = load_image_from_data_uri("not a data uri");
        assert!(result.is_err());

        let result = load_image_from_data_uri("data:image/png");
        assert!(result.is_err()); // Missing comma
    }
}
