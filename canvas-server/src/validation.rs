//! Input validation for untrusted data.
//!
//! All user-supplied input MUST be validated before use.
//! This module provides validators for common data types.

use thiserror::Error;

/// Maximum length for session IDs.
pub const MAX_SESSION_ID_LEN: usize = 64;
/// Maximum length for element IDs (UUIDs are 36 chars).
pub const MAX_ELEMENT_ID_LEN: usize = 64;
/// Maximum length for peer IDs.
pub const MAX_PEER_ID_LEN: usize = 64;
/// Maximum length for SDP offers/answers.
pub const MAX_SDP_LEN: usize = 65536; // 64KB should be plenty
/// Maximum length for ICE candidates.
pub const MAX_ICE_CANDIDATE_LEN: usize = 2048;
/// Maximum text content length in elements.
pub const MAX_TEXT_CONTENT_LEN: usize = 1_048_576; // 1MB
/// Maximum elements per scene.
pub const MAX_ELEMENTS_PER_SCENE: usize = 10_000;
/// Maximum WebSocket message size.
pub const MAX_WS_MESSAGE_SIZE: usize = 1_048_576; // 1MB

/// Validation error types.
#[derive(Debug, Error)]
pub enum ValidationError {
    /// Session ID exceeds maximum length.
    #[error("session_id too long (max {MAX_SESSION_ID_LEN} chars)")]
    SessionIdTooLong,
    /// Session ID contains invalid characters.
    #[error("session_id contains invalid characters")]
    SessionIdInvalidChars,
    /// Element ID exceeds maximum length.
    #[error("element_id too long (max {MAX_ELEMENT_ID_LEN} chars)")]
    ElementIdTooLong,
    /// Element ID contains invalid characters.
    #[error("element_id contains invalid characters")]
    ElementIdInvalidChars,
    /// Peer ID exceeds maximum length.
    #[error("peer_id too long (max {MAX_PEER_ID_LEN} chars)")]
    PeerIdTooLong,
    /// Peer ID contains invalid characters.
    #[error("peer_id contains invalid characters")]
    PeerIdInvalidChars,
    /// SDP exceeds maximum length.
    #[error("SDP too long (max {MAX_SDP_LEN} bytes)")]
    SdpTooLong,
    /// ICE candidate exceeds maximum length.
    #[error("ICE candidate too long (max {MAX_ICE_CANDIDATE_LEN} bytes)")]
    IceCandidateTooLong,
    /// Text content exceeds maximum length.
    #[error("text content too long (max {MAX_TEXT_CONTENT_LEN} bytes)")]
    TextContentTooLong,
    /// Too many elements in scene.
    #[error("too many elements (max {MAX_ELEMENTS_PER_SCENE})")]
    TooManyElements,
    /// WebSocket message exceeds maximum size.
    #[error("message too large (max {MAX_WS_MESSAGE_SIZE} bytes)")]
    MessageTooLarge,
    /// Invalid transform values.
    #[error("invalid transform: {0}")]
    InvalidTransform(String),
}

/// Check if a character is valid for IDs (alphanumeric, hyphen, or underscore).
fn is_valid_id_char(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_'
}

/// Validate a session ID.
///
/// Valid session IDs:
/// - 1-64 characters
/// - Alphanumeric, hyphen, underscore only
///
/// # Errors
///
/// Returns [`ValidationError::SessionIdTooLong`] if the ID exceeds 64 characters.
/// Returns [`ValidationError::SessionIdInvalidChars`] if the ID is empty or contains invalid characters.
pub fn validate_session_id(id: &str) -> Result<(), ValidationError> {
    if id.len() > MAX_SESSION_ID_LEN {
        return Err(ValidationError::SessionIdTooLong);
    }
    if id.is_empty() || !id.chars().all(is_valid_id_char) {
        return Err(ValidationError::SessionIdInvalidChars);
    }
    Ok(())
}

/// Validate an element ID.
///
/// Valid element IDs:
/// - 1-64 characters
/// - Alphanumeric, hyphen, underscore only (UUIDs are valid)
///
/// # Errors
///
/// Returns [`ValidationError::ElementIdTooLong`] if the ID exceeds 64 characters.
/// Returns [`ValidationError::ElementIdInvalidChars`] if the ID is empty or contains invalid characters.
pub fn validate_element_id(id: &str) -> Result<(), ValidationError> {
    if id.len() > MAX_ELEMENT_ID_LEN {
        return Err(ValidationError::ElementIdTooLong);
    }
    if id.is_empty() || !id.chars().all(is_valid_id_char) {
        return Err(ValidationError::ElementIdInvalidChars);
    }
    Ok(())
}

/// Validate a peer ID.
///
/// Valid peer IDs:
/// - 1-64 characters
/// - Alphanumeric, hyphen, underscore only
///
/// # Errors
///
/// Returns [`ValidationError::PeerIdTooLong`] if the ID exceeds 64 characters.
/// Returns [`ValidationError::PeerIdInvalidChars`] if the ID is empty or contains invalid characters.
pub fn validate_peer_id(id: &str) -> Result<(), ValidationError> {
    if id.len() > MAX_PEER_ID_LEN {
        return Err(ValidationError::PeerIdTooLong);
    }
    if id.is_empty() || !id.chars().all(is_valid_id_char) {
        return Err(ValidationError::PeerIdInvalidChars);
    }
    Ok(())
}

/// Validate an SDP string.
///
/// Basic validation:
/// - Max length check (64KB)
///
/// Note: Full SDP parsing is complex; this catches obviously oversized data.
///
/// # Errors
///
/// Returns [`ValidationError::SdpTooLong`] if the SDP exceeds 64KB.
pub fn validate_sdp(sdp: &str) -> Result<(), ValidationError> {
    if sdp.len() > MAX_SDP_LEN {
        return Err(ValidationError::SdpTooLong);
    }
    Ok(())
}

/// Validate an ICE candidate string.
///
/// # Errors
///
/// Returns [`ValidationError::IceCandidateTooLong`] if the candidate exceeds 2KB.
pub fn validate_ice_candidate(candidate: &str) -> Result<(), ValidationError> {
    if candidate.len() > MAX_ICE_CANDIDATE_LEN {
        return Err(ValidationError::IceCandidateTooLong);
    }
    Ok(())
}

/// Validate text content length.
///
/// # Errors
///
/// Returns [`ValidationError::TextContentTooLong`] if the text exceeds 1MB.
pub fn validate_text_content(text: &str) -> Result<(), ValidationError> {
    if text.len() > MAX_TEXT_CONTENT_LEN {
        return Err(ValidationError::TextContentTooLong);
    }
    Ok(())
}

/// Validate WebSocket message size.
///
/// # Errors
///
/// Returns [`ValidationError::MessageTooLarge`] if the message exceeds 1MB.
pub fn validate_message_size(size: usize) -> Result<(), ValidationError> {
    if size > MAX_WS_MESSAGE_SIZE {
        return Err(ValidationError::MessageTooLarge);
    }
    Ok(())
}

/// Validate element count in scene.
///
/// # Errors
///
/// Returns [`ValidationError::TooManyElements`] if the count reaches the limit.
pub fn validate_element_count(count: usize) -> Result<(), ValidationError> {
    if count >= MAX_ELEMENTS_PER_SCENE {
        return Err(ValidationError::TooManyElements);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_session_ids() {
        assert!(validate_session_id("default").is_ok());
        assert!(validate_session_id("my-session").is_ok());
        assert!(validate_session_id("session_123").is_ok());
        assert!(validate_session_id("a").is_ok());
        assert!(validate_session_id("ABC123").is_ok());
        assert!(validate_session_id("test-session_v2").is_ok());
    }

    #[test]
    fn test_invalid_session_ids() {
        assert!(validate_session_id("").is_err());
        assert!(validate_session_id("has spaces").is_err());
        assert!(validate_session_id("has/slash").is_err());
        assert!(validate_session_id("../../../etc/passwd").is_err());
        assert!(validate_session_id("path\\traversal").is_err());
        assert!(validate_session_id(&"x".repeat(100)).is_err());
        assert!(validate_session_id("contains<script>").is_err());
    }

    #[test]
    fn test_session_id_boundary() {
        // Exactly at limit should pass
        let at_limit = "x".repeat(MAX_SESSION_ID_LEN);
        assert!(validate_session_id(&at_limit).is_ok());

        // One over should fail
        let over_limit = "x".repeat(MAX_SESSION_ID_LEN + 1);
        assert!(validate_session_id(&over_limit).is_err());
    }

    #[test]
    fn test_valid_element_ids() {
        assert!(validate_element_id("550e8400-e29b-41d4-a716-446655440000").is_ok());
        assert!(validate_element_id("element_1").is_ok());
        assert!(validate_element_id("elem-123").is_ok());
    }

    #[test]
    fn test_invalid_element_ids() {
        assert!(validate_element_id("").is_err());
        assert!(validate_element_id("has spaces").is_err());
        assert!(validate_element_id(&"x".repeat(100)).is_err());
    }

    #[test]
    fn test_valid_peer_ids() {
        assert!(validate_peer_id("peer-abc123").is_ok());
        assert!(validate_peer_id("user_42").is_ok());
        assert!(validate_peer_id("550e8400-e29b-41d4-a716-446655440000").is_ok());
    }

    #[test]
    fn test_invalid_peer_ids() {
        assert!(validate_peer_id("").is_err());
        assert!(validate_peer_id("peer with space").is_err());
        assert!(validate_peer_id(&"x".repeat(100)).is_err());
    }

    #[test]
    fn test_sdp_length() {
        assert!(validate_sdp("v=0\r\n").is_ok());
        assert!(validate_sdp("").is_ok()); // Empty is technically valid
        assert!(validate_sdp(&"x".repeat(MAX_SDP_LEN)).is_ok());
        assert!(validate_sdp(&"x".repeat(MAX_SDP_LEN + 1)).is_err());
    }

    #[test]
    fn test_ice_candidate_length() {
        assert!(validate_ice_candidate("candidate:1 1 UDP 2130706431").is_ok());
        assert!(validate_ice_candidate(&"x".repeat(MAX_ICE_CANDIDATE_LEN)).is_ok());
        assert!(validate_ice_candidate(&"x".repeat(MAX_ICE_CANDIDATE_LEN + 1)).is_err());
    }

    #[test]
    fn test_text_content_length() {
        assert!(validate_text_content("Hello, world!").is_ok());
        assert!(validate_text_content(&"x".repeat(MAX_TEXT_CONTENT_LEN)).is_ok());
        assert!(validate_text_content(&"x".repeat(MAX_TEXT_CONTENT_LEN + 1)).is_err());
    }

    #[test]
    fn test_message_size() {
        assert!(validate_message_size(1000).is_ok());
        assert!(validate_message_size(MAX_WS_MESSAGE_SIZE).is_ok());
        assert!(validate_message_size(MAX_WS_MESSAGE_SIZE + 1).is_err());
    }

    #[test]
    fn test_element_count() {
        assert!(validate_element_count(0).is_ok());
        assert!(validate_element_count(MAX_ELEMENTS_PER_SCENE - 1).is_ok());
        assert!(validate_element_count(MAX_ELEMENTS_PER_SCENE).is_err());
    }

    #[test]
    fn test_error_messages() {
        let err = ValidationError::SessionIdTooLong;
        assert!(err.to_string().contains("64"));

        let err = ValidationError::SdpTooLong;
        assert!(err.to_string().contains("65536"));

        let err = ValidationError::MessageTooLarge;
        assert!(err.to_string().contains("1048576"));
    }
}
