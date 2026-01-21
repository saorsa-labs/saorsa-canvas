//! Video texture management for streaming video content.
//!
//! This module provides types and utilities for managing video frame textures
//! that can be uploaded to the GPU for rendering. It supports:
//!
//! - Per-frame RGBA data upload
//! - Caching of video textures by stream ID
//! - Graceful handling of missing video streams

use std::collections::HashMap;

use thiserror::Error;

/// Errors that can occur during video texture operations.
#[derive(Debug, Error)]
pub enum VideoTextureError {
    /// The video frame data has invalid dimensions or size.
    #[error("Invalid video frame data: expected {expected} bytes, got {actual}")]
    InvalidFrameData {
        /// Expected byte count.
        expected: usize,
        /// Actual byte count.
        actual: usize,
    },

    /// The requested stream was not found.
    #[error("Video stream not found: {0}")]
    StreamNotFound(String),

    /// GPU texture creation failed.
    #[error("Failed to create video texture: {0}")]
    TextureCreation(String),
}

/// Result type for video texture operations.
pub type VideoTextureResult<T> = Result<T, VideoTextureError>;

/// Raw video frame data in RGBA format.
///
/// This struct holds a single frame of video data that can be uploaded
/// to a GPU texture. The data is expected to be in RGBA format with
/// 4 bytes per pixel.
#[derive(Debug, Clone)]
pub struct VideoFrameData {
    /// Width of the frame in pixels.
    pub width: u32,
    /// Height of the frame in pixels.
    pub height: u32,
    /// RGBA pixel data (4 bytes per pixel, row-major order).
    pub data: Vec<u8>,
}

impl VideoFrameData {
    /// Create a new video frame from RGBA data.
    ///
    /// # Errors
    ///
    /// Returns an error if the data length doesn't match width * height * 4.
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> VideoTextureResult<Self> {
        let expected = (width as usize) * (height as usize) * 4;
        if data.len() != expected {
            return Err(VideoTextureError::InvalidFrameData {
                expected,
                actual: data.len(),
            });
        }

        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// Create a placeholder frame with a solid color.
    ///
    /// Used when a video stream is not yet available.
    #[must_use]
    pub fn placeholder(width: u32, height: u32) -> Self {
        let pixel_count = (width as usize) * (height as usize);
        let mut data = Vec::with_capacity(pixel_count * 4);

        // Create a dark gray placeholder (similar to video player background)
        for _ in 0..pixel_count {
            data.extend_from_slice(&[32, 32, 32, 255]); // Dark gray
        }

        Self {
            width,
            height,
            data,
        }
    }

    /// Check if the frame dimensions are valid (non-zero).
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.width > 0 && self.height > 0 && !self.data.is_empty()
    }
}

/// Cached video texture entry.
#[derive(Debug)]
pub struct VideoTextureEntry {
    /// Width of the cached texture.
    pub width: u32,
    /// Height of the cached texture.
    pub height: u32,
    /// Last update timestamp (frame number or time).
    pub last_updated: u64,
}

/// Manages video textures for multiple streams.
///
/// This manager tracks active video streams and their associated GPU textures.
/// It provides methods to update textures with new frame data and retrieve
/// cached textures for rendering.
///
/// # Example
///
/// ```ignore
/// use canvas_renderer::video::{VideoTextureManager, VideoFrameData};
///
/// let mut manager = VideoTextureManager::new();
///
/// // Update a video stream with new frame data
/// let frame = VideoFrameData::placeholder(640, 480);
/// manager.update_texture("stream-1", frame);
///
/// // Check if texture exists
/// if manager.has_texture("stream-1") {
///     // Render the video element
/// }
/// ```
#[derive(Debug, Default)]
pub struct VideoTextureManager {
    /// Texture metadata by stream ID.
    entries: HashMap<String, VideoTextureEntry>,
    /// Frame counter for tracking updates.
    frame_counter: u64,
}

impl VideoTextureManager {
    /// Create a new video texture manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            frame_counter: 0,
        }
    }

    /// Update or create a video texture for a stream.
    ///
    /// This method records the texture metadata. The actual GPU texture
    /// upload is handled by the wgpu backend using the frame data.
    pub fn update_texture(&mut self, stream_id: &str, frame: &VideoFrameData) {
        self.frame_counter += 1;

        self.entries.insert(
            stream_id.to_string(),
            VideoTextureEntry {
                width: frame.width,
                height: frame.height,
                last_updated: self.frame_counter,
            },
        );
    }

    /// Get texture metadata for a stream.
    #[must_use]
    pub fn get_texture(&self, stream_id: &str) -> Option<&VideoTextureEntry> {
        self.entries.get(stream_id)
    }

    /// Check if a texture exists for a stream.
    #[must_use]
    pub fn has_texture(&self, stream_id: &str) -> bool {
        self.entries.contains_key(stream_id)
    }

    /// Remove a video texture from the cache.
    ///
    /// Call this when a video stream ends or the element is removed.
    pub fn remove_texture(&mut self, stream_id: &str) -> bool {
        self.entries.remove(stream_id).is_some()
    }

    /// Clear all video textures.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Get the number of cached video textures.
    #[must_use]
    pub fn texture_count(&self) -> usize {
        self.entries.len()
    }

    /// Get an iterator over all stream IDs.
    pub fn stream_ids(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// Get the current frame counter.
    #[must_use]
    pub fn frame_counter(&self) -> u64 {
        self.frame_counter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_video_frame_data_new() {
        // Valid frame
        let data = vec![0u8; 4 * 4 * 4]; // 4x4 RGBA
        let frame = VideoFrameData::new(4, 4, data);
        assert!(frame.is_ok());

        let frame = frame.expect("Frame should be valid");
        assert_eq!(frame.width, 4);
        assert_eq!(frame.height, 4);
        assert!(frame.is_valid());
    }

    #[test]
    fn test_video_frame_data_invalid() {
        // Wrong size
        let data = vec![0u8; 10]; // Not 4x4x4
        let frame = VideoFrameData::new(4, 4, data);
        assert!(frame.is_err());

        match frame {
            Err(VideoTextureError::InvalidFrameData { expected, actual }) => {
                assert_eq!(expected, 64);
                assert_eq!(actual, 10);
            }
            _ => panic!("Expected InvalidFrameData error"),
        }
    }

    #[test]
    fn test_video_frame_placeholder() {
        let frame = VideoFrameData::placeholder(640, 480);
        assert_eq!(frame.width, 640);
        assert_eq!(frame.height, 480);
        assert_eq!(frame.data.len(), 640 * 480 * 4);
        assert!(frame.is_valid());

        // Check that it's dark gray
        assert_eq!(&frame.data[0..4], &[32, 32, 32, 255]);
    }

    #[test]
    fn test_video_texture_manager() {
        let mut manager = VideoTextureManager::new();

        // Initially empty
        assert_eq!(manager.texture_count(), 0);
        assert!(!manager.has_texture("stream-1"));

        // Add a texture
        let frame = VideoFrameData::placeholder(320, 240);
        manager.update_texture("stream-1", &frame);

        assert_eq!(manager.texture_count(), 1);
        assert!(manager.has_texture("stream-1"));

        // Get texture metadata
        let entry = manager.get_texture("stream-1");
        assert!(entry.is_some());
        let entry = entry.expect("Entry should exist");
        assert_eq!(entry.width, 320);
        assert_eq!(entry.height, 240);
        assert_eq!(entry.last_updated, 1);

        // Update the same stream
        let frame2 = VideoFrameData::placeholder(640, 480);
        manager.update_texture("stream-1", &frame2);

        let entry = manager.get_texture("stream-1").expect("Entry should exist");
        assert_eq!(entry.width, 640);
        assert_eq!(entry.height, 480);
        assert_eq!(entry.last_updated, 2);

        // Add another stream
        manager.update_texture("stream-2", &frame);
        assert_eq!(manager.texture_count(), 2);

        // Remove a texture
        assert!(manager.remove_texture("stream-1"));
        assert_eq!(manager.texture_count(), 1);
        assert!(!manager.has_texture("stream-1"));
        assert!(manager.has_texture("stream-2"));

        // Remove non-existent
        assert!(!manager.remove_texture("stream-1"));

        // Clear all
        manager.clear();
        assert_eq!(manager.texture_count(), 0);
    }

    #[test]
    fn test_video_texture_manager_stream_ids() {
        let mut manager = VideoTextureManager::new();

        let frame = VideoFrameData::placeholder(100, 100);
        manager.update_texture("a", &frame);
        manager.update_texture("b", &frame);
        manager.update_texture("c", &frame);

        let mut ids: Vec<_> = manager.stream_ids().collect();
        ids.sort();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }
}
