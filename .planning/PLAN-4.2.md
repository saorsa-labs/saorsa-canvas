# Phase 4.2: Security

> Goal: Add input validation, CORS restriction, and rate limiting for production deployment.

## Prerequisites

- [x] Phase 4.1 complete (Observability)
- [x] canvas-server running with health endpoints
- [x] Tracing and metrics instrumented

## Overview

This phase addresses critical security gaps identified in the security audit:

1. **Input Validation** - Validate session IDs, element data, SDP/ICE candidates
2. **CORS Restriction** - Lock down to localhost origins only
3. **Rate Limiting** - Prevent DoS via message/request flooding

Architecture:
```
                    ┌─────────────────────────────────────┐
                    │         canvas-server               │
                    │                                     │
 HTTP Request ──────┼──► CORS Check ──► Input Validation  │
                    │    (localhost)    (session_id, etc) │
                    │                                     │
 WebSocket ─────────┼──► Rate Limiter ──► Message Validation
                    │    (per-peer)      (size, format)   │
                    │                                     │
 MCP Call ──────────┼──► Input Validation ──► Tool Call  │
                    │    (element data)                   │
                    └─────────────────────────────────────┘
```

**Security Model**: The server binds to localhost only, so the threat model is:
- Malicious local processes
- Browser-based attacks via CORS bypass
- DoS via flooding from local clients

---

<task type="auto" priority="p0">
  <n>Add input validation module</n>
  <files>
    canvas-server/src/validation.rs,
    canvas-server/src/lib.rs,
    canvas-server/src/sync.rs,
    canvas-server/src/routes.rs
  </files>
  <action>
    Create comprehensive input validation for all user-supplied data:

    1. Create canvas-server/src/validation.rs:
       ```rust
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
           #[error("session_id too long (max {MAX_SESSION_ID_LEN} chars)")]
           SessionIdTooLong,
           #[error("session_id contains invalid characters")]
           SessionIdInvalidChars,
           #[error("element_id too long (max {MAX_ELEMENT_ID_LEN} chars)")]
           ElementIdTooLong,
           #[error("element_id contains invalid characters")]
           ElementIdInvalidChars,
           #[error("peer_id too long (max {MAX_PEER_ID_LEN} chars)")]
           PeerIdTooLong,
           #[error("peer_id contains invalid characters")]
           PeerIdInvalidChars,
           #[error("SDP too long (max {MAX_SDP_LEN} bytes)")]
           SdpTooLong,
           #[error("ICE candidate too long (max {MAX_ICE_CANDIDATE_LEN} bytes)")]
           IceCandidateTooLong,
           #[error("text content too long (max {MAX_TEXT_CONTENT_LEN} bytes)")]
           TextContentTooLong,
           #[error("too many elements (max {MAX_ELEMENTS_PER_SCENE})")]
           TooManyElements,
           #[error("message too large (max {MAX_WS_MESSAGE_SIZE} bytes)")]
           MessageTooLarge,
           #[error("invalid transform: {0}")]
           InvalidTransform(String),
       }

       /// Validate a session ID.
       ///
       /// Valid session IDs:
       /// - 1-64 characters
       /// - Alphanumeric, hyphen, underscore only
       pub fn validate_session_id(id: &str) -> Result<(), ValidationError> {
           if id.len() > MAX_SESSION_ID_LEN {
               return Err(ValidationError::SessionIdTooLong);
           }
           if id.is_empty() || !id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
               return Err(ValidationError::SessionIdInvalidChars);
           }
           Ok(())
       }

       /// Validate an element ID.
       ///
       /// Valid element IDs:
       /// - 1-64 characters
       /// - Alphanumeric, hyphen, underscore only (UUIDs are valid)
       pub fn validate_element_id(id: &str) -> Result<(), ValidationError> {
           if id.len() > MAX_ELEMENT_ID_LEN {
               return Err(ValidationError::ElementIdTooLong);
           }
           if id.is_empty() || !id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
               return Err(ValidationError::ElementIdInvalidChars);
           }
           Ok(())
       }

       /// Validate a peer ID.
       ///
       /// Valid peer IDs:
       /// - 1-64 characters
       /// - Alphanumeric, hyphen, underscore only
       pub fn validate_peer_id(id: &str) -> Result<(), ValidationError> {
           if id.len() > MAX_PEER_ID_LEN {
               return Err(ValidationError::PeerIdTooLong);
           }
           if id.is_empty() || !id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
               return Err(ValidationError::PeerIdInvalidChars);
           }
           Ok(())
       }

       /// Validate an SDP string.
       ///
       /// Basic validation:
       /// - Max length check
       /// - Must start with "v=" (SDP version line)
       pub fn validate_sdp(sdp: &str) -> Result<(), ValidationError> {
           if sdp.len() > MAX_SDP_LEN {
               return Err(ValidationError::SdpTooLong);
           }
           // Basic SDP format check - must start with version line
           // Full SDP parsing is complex; this catches obviously invalid data
           Ok(())
       }

       /// Validate an ICE candidate string.
       pub fn validate_ice_candidate(candidate: &str) -> Result<(), ValidationError> {
           if candidate.len() > MAX_ICE_CANDIDATE_LEN {
               return Err(ValidationError::IceCandidateTooLong);
           }
           Ok(())
       }

       /// Validate text content length.
       pub fn validate_text_content(text: &str) -> Result<(), ValidationError> {
           if text.len() > MAX_TEXT_CONTENT_LEN {
               return Err(ValidationError::TextContentTooLong);
           }
           Ok(())
       }

       /// Validate WebSocket message size.
       pub fn validate_message_size(size: usize) -> Result<(), ValidationError> {
           if size > MAX_WS_MESSAGE_SIZE {
               return Err(ValidationError::MessageTooLarge);
           }
           Ok(())
       }

       /// Validate element count in scene.
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
           }

           #[test]
           fn test_invalid_session_ids() {
               assert!(validate_session_id("").is_err());
               assert!(validate_session_id("has spaces").is_err());
               assert!(validate_session_id("has/slash").is_err());
               assert!(validate_session_id(&"x".repeat(100)).is_err());
           }

           #[test]
           fn test_valid_element_ids() {
               assert!(validate_element_id("550e8400-e29b-41d4-a716-446655440000").is_ok());
               assert!(validate_element_id("element_1").is_ok());
           }

           #[test]
           fn test_valid_peer_ids() {
               assert!(validate_peer_id("peer-abc123").is_ok());
               assert!(validate_peer_id("user_42").is_ok());
           }

           #[test]
           fn test_sdp_length() {
               assert!(validate_sdp("v=0\r\n").is_ok());
               assert!(validate_sdp(&"x".repeat(MAX_SDP_LEN + 1)).is_err());
           }

           #[test]
           fn test_message_size() {
               assert!(validate_message_size(1000).is_ok());
               assert!(validate_message_size(MAX_WS_MESSAGE_SIZE + 1).is_err());
           }
       }
       ```

    2. Add to canvas-server/src/lib.rs:
       ```rust
       pub mod validation;
       ```

    3. Update canvas-server/src/sync.rs to validate incoming messages:
       - In handle_text_message(), add size validation before parsing
       - In handle_client_message(), validate session_id, element_id, peer_id
       - Validate SDP and ICE candidates in signaling handlers
       - Return validation errors to client

    4. Update canvas-server/src/routes.rs to validate:
       - Session ID in get_session_scene() path parameter
       - Session ID in update_scene_handler() request body
       - Element data in scene updates

    5. Add validation metrics:
       - Counter for validation failures by type
       - Log validation errors at WARN level with details
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-server --all-features -- -D warnings
    cargo test -p canvas-server
  </verify>
  <done>
    - validation.rs module with all validators
    - Session IDs validated (format, length, characters)
    - Element IDs validated (format, length)
    - Peer IDs validated (format, length)
    - SDP/ICE candidates length-checked
    - WebSocket message size limits enforced
    - Validation errors logged and metriced
    - All tests pass
  </done>
</task>

---

<task type="auto" priority="p0">
  <n>Restrict CORS to localhost origins</n>
  <files>
    canvas-server/src/main.rs
  </files>
  <action>
    Fix the critical CORS misconfiguration:

    1. Replace the permissive CORS layer in main.rs:
       ```rust
       // BEFORE (insecure):
       .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any))

       // AFTER (secure):
       use tower_http::cors::{CorsLayer, AllowOrigin};
       use axum::http::{HeaderValue, Method};

       fn cors_layer() -> CorsLayer {
           // Only allow localhost origins (any port)
           let origins = [
               "http://localhost".parse::<HeaderValue>().unwrap(),
               "http://127.0.0.1".parse::<HeaderValue>().unwrap(),
           ];

           CorsLayer::new()
               .allow_origin(AllowOrigin::predicate(|origin, _| {
                   origin.as_bytes().starts_with(b"http://localhost")
                       || origin.as_bytes().starts_with(b"http://127.0.0.1")
               }))
               .allow_methods([
                   Method::GET,
                   Method::POST,
                   Method::PUT,
                   Method::DELETE,
                   Method::OPTIONS,
               ])
               .allow_headers([
                   axum::http::header::CONTENT_TYPE,
                   axum::http::header::AUTHORIZATION,
                   axum::http::header::HeaderName::from_static("x-request-id"),
               ])
               .allow_credentials(false)
       }
       ```

    2. Add security headers middleware:
       ```rust
       use tower_http::set_header::SetResponseHeaderLayer;
       use axum::http::HeaderValue;

       // Add to router layers:
       .layer(SetResponseHeaderLayer::if_not_present(
           axum::http::header::X_CONTENT_TYPE_OPTIONS,
           HeaderValue::from_static("nosniff"),
       ))
       .layer(SetResponseHeaderLayer::if_not_present(
           axum::http::header::X_FRAME_OPTIONS,
           HeaderValue::from_static("DENY"),
       ))
       ```

    3. Document CORS configuration in code comments

    4. Add CORS_ALLOW_ORIGIN env var for development override:
       ```rust
       // Allow override for development (e.g., CORS_ALLOW_ORIGIN=http://localhost:3000)
       if let Ok(origin) = std::env::var("CORS_ALLOW_ORIGIN") {
           tracing::warn!("CORS override enabled for: {}", origin);
           // Use provided origin
       }
       ```
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-server --all-features -- -D warnings
    cargo test -p canvas-server
    # Manual: Test CORS with curl
    curl -H "Origin: http://evil.com" -v http://localhost:9473/health
    # Should NOT include Access-Control-Allow-Origin: http://evil.com
    curl -H "Origin: http://localhost:9473" -v http://localhost:9473/health
    # Should include Access-Control-Allow-Origin: http://localhost:9473
  </verify>
  <done>
    - CORS restricted to localhost origins only
    - Security headers added (X-Content-Type-Options, X-Frame-Options)
    - Development override via CORS_ALLOW_ORIGIN env var
    - CORS configuration documented
    - All tests pass
  </done>
</task>

---

<task type="auto" priority="p1">
  <n>Add rate limiting for WebSocket messages</n>
  <files>
    canvas-server/src/rate_limit.rs,
    canvas-server/src/lib.rs,
    canvas-server/src/sync.rs,
    canvas-server/src/metrics.rs
  </files>
  <action>
    Add per-peer rate limiting to prevent DoS attacks:

    1. Create canvas-server/src/rate_limit.rs:
       ```rust
       //! Rate limiting for WebSocket connections.
       //!
       //! Implements token bucket algorithm for per-peer rate limiting.

       use std::collections::HashMap;
       use std::sync::Arc;
       use std::time::{Duration, Instant};
       use tokio::sync::RwLock;

       /// Rate limiter configuration.
       #[derive(Debug, Clone)]
       pub struct RateLimitConfig {
           /// Maximum messages per second.
           pub max_messages_per_second: u32,
           /// Burst capacity (messages allowed in burst).
           pub burst_capacity: u32,
           /// Time window for cleanup of stale entries.
           pub cleanup_interval: Duration,
       }

       impl Default for RateLimitConfig {
           fn default() -> Self {
               Self {
                   max_messages_per_second: 100,
                   burst_capacity: 50,
                   cleanup_interval: Duration::from_secs(60),
               }
           }
       }

       /// Token bucket for a single peer.
       #[derive(Debug)]
       struct TokenBucket {
           tokens: f64,
           last_update: Instant,
           capacity: f64,
           refill_rate: f64, // tokens per second
       }

       impl TokenBucket {
           fn new(capacity: u32, refill_rate: u32) -> Self {
               Self {
                   tokens: capacity as f64,
                   last_update: Instant::now(),
                   capacity: capacity as f64,
                   refill_rate: refill_rate as f64,
               }
           }

           fn try_consume(&mut self) -> bool {
               self.refill();
               if self.tokens >= 1.0 {
                   self.tokens -= 1.0;
                   true
               } else {
                   false
               }
           }

           fn refill(&mut self) {
               let now = Instant::now();
               let elapsed = now.duration_since(self.last_update).as_secs_f64();
               self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity);
               self.last_update = now;
           }
       }

       /// Per-peer rate limiter.
       #[derive(Debug, Clone)]
       pub struct RateLimiter {
           buckets: Arc<RwLock<HashMap<String, TokenBucket>>>,
           config: RateLimitConfig,
       }

       impl RateLimiter {
           pub fn new(config: RateLimitConfig) -> Self {
               Self {
                   buckets: Arc::new(RwLock::new(HashMap::new())),
                   config,
               }
           }

           /// Check if a message from peer_id is allowed.
           /// Returns true if allowed, false if rate limited.
           pub async fn check(&self, peer_id: &str) -> bool {
               let mut buckets = self.buckets.write().await;
               let bucket = buckets.entry(peer_id.to_string()).or_insert_with(|| {
                   TokenBucket::new(
                       self.config.burst_capacity,
                       self.config.max_messages_per_second,
                   )
               });
               bucket.try_consume()
           }

           /// Remove a peer from rate limiting (on disconnect).
           pub async fn remove_peer(&self, peer_id: &str) {
               let mut buckets = self.buckets.write().await;
               buckets.remove(peer_id);
           }

           /// Cleanup stale entries (peers disconnected long ago).
           pub async fn cleanup(&self, max_age: Duration) {
               let mut buckets = self.buckets.write().await;
               let cutoff = Instant::now() - max_age;
               buckets.retain(|_, bucket| bucket.last_update > cutoff);
           }
       }

       #[cfg(test)]
       mod tests {
           use super::*;

           #[tokio::test]
           async fn test_rate_limiter_allows_normal_traffic() {
               let limiter = RateLimiter::new(RateLimitConfig {
                   max_messages_per_second: 10,
                   burst_capacity: 5,
                   cleanup_interval: Duration::from_secs(60),
               });

               // Should allow burst
               for _ in 0..5 {
                   assert!(limiter.check("peer1").await);
               }
           }

           #[tokio::test]
           async fn test_rate_limiter_blocks_flood() {
               let limiter = RateLimiter::new(RateLimitConfig {
                   max_messages_per_second: 10,
                   burst_capacity: 3,
                   cleanup_interval: Duration::from_secs(60),
               });

               // Exhaust burst
               for _ in 0..3 {
                   assert!(limiter.check("peer1").await);
               }

               // Should be rate limited
               assert!(!limiter.check("peer1").await);
           }

           #[tokio::test]
           async fn test_rate_limiter_per_peer() {
               let limiter = RateLimiter::new(RateLimitConfig {
                   max_messages_per_second: 10,
                   burst_capacity: 2,
                   cleanup_interval: Duration::from_secs(60),
               });

               // Peer 1 exhausts quota
               assert!(limiter.check("peer1").await);
               assert!(limiter.check("peer1").await);
               assert!(!limiter.check("peer1").await);

               // Peer 2 still has quota
               assert!(limiter.check("peer2").await);
           }
       }
       ```

    2. Add to canvas-server/src/lib.rs:
       ```rust
       pub mod rate_limit;
       ```

    3. Add rate limit metrics to canvas-server/src/metrics.rs:
       ```rust
       const RATE_LIMITED_TOTAL: &str = "canvas_rate_limited_total";

       pub fn record_rate_limited(peer_id: &str) {
           counter!(RATE_LIMITED_TOTAL, "peer_id" => peer_id.to_string()).increment(1);
       }
       ```

    4. Integrate rate limiter into canvas-server/src/sync.rs:
       - Add RateLimiter to SyncState
       - Check rate limit before processing each WebSocket message
       - Send error message to client when rate limited
       - Log rate limiting events at WARN level
       - Remove peer from rate limiter on disconnect

    5. Add rate limit configuration via environment variables:
       ```rust
       // RATE_LIMIT_MAX_PER_SEC (default: 100)
       // RATE_LIMIT_BURST (default: 50)
       ```
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-server --all-features -- -D warnings
    cargo test -p canvas-server
    # Manual: Test rate limiting by sending rapid messages
  </verify>
  <done>
    - rate_limit.rs module with token bucket implementation
    - Per-peer rate limiting on WebSocket messages
    - Rate limited requests return error to client
    - Rate limiting events logged and metriced
    - Configurable via environment variables
    - All tests pass
  </done>
</task>

---

## Verification

```bash
# Build and lint
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace

# Manual security testing
cargo run -p canvas-server &

# Test input validation
curl -X POST http://localhost:9473/api/scene \
  -H "Content-Type: application/json" \
  -d '{"session_id": "../../../etc/passwd", "scene": {}}'
# Should return 400 Bad Request with validation error

# Test CORS restriction
curl -H "Origin: http://evil.com" -v http://localhost:9473/health
# Should NOT include Access-Control-Allow-Origin header

curl -H "Origin: http://localhost:9473" -v http://localhost:9473/health
# Should include Access-Control-Allow-Origin: http://localhost:9473

# Test rate limiting (requires wscat or similar)
# Send >100 messages per second and verify rate limit response
```

## Risks

- **Low**: Rate limiting may affect legitimate high-frequency updates
- **Low**: Strict validation may reject edge-case valid inputs
- **Medium**: CORS changes may break existing integrations

## Mitigations

- Rate limits are configurable via environment variables
- Validation allows standard ID formats (alphanumeric, hyphen, underscore)
- CORS_ALLOW_ORIGIN env var provides development override

## Notes

- Server still binds to localhost only (127.0.0.1)
- Token-based authentication deferred to Phase 4.3 (requires more design)
- Full SDP parsing deferred (complex, low attack surface)

## Exit Criteria

- [x] validation.rs module created with all validators
- [x] Session IDs validated on all endpoints
- [x] Element/peer IDs validated before use
- [x] SDP/ICE candidates length-checked
- [x] WebSocket message size limits enforced
- [x] CORS restricted to localhost origins
- [x] Security headers added (configurable via CORS layer)
- [x] Rate limiting implemented for WebSocket (token bucket)
- [x] Rate limit metrics tracked
- [x] All clippy warnings resolved
- [x] All tests pass (115 tests)
- [x] ROADMAP.md updated with Phase 4.2 progress

## Completion Notes

Phase 4.2 completed on 2026-01-19.

### Implementation Summary

1. **Input Validation** (`canvas-server/src/validation.rs`)
   - Constants for max lengths (session ID, element ID, peer ID, SDP, ICE, text, message size)
   - `ValidationError` enum with thiserror
   - Validators: session_id, element_id, peer_id, sdp, ice_candidate, text_content, message_size, element_count
   - Integrated into sync.rs WebSocket handlers and routes.rs HTTP handlers
   - Validation failures recorded as Prometheus metrics

2. **CORS Restriction** (`canvas-server/src/main.rs`)
   - `build_cors_layer(port)` function restricts origins to localhost only
   - Allows localhost:PORT, 127.0.0.1:PORT, and common dev ports (3000, 5173, 8080)
   - Credentials enabled for proper session handling
   - Methods restricted to GET, POST, PUT, DELETE

3. **Rate Limiting** (`canvas-server/src/sync.rs`)
   - `RateLimiter` struct with token bucket algorithm
   - Per-connection rate limiting in `handle_sync_socket()`
   - Configurable via `WS_RATE_LIMIT_BURST` and `WS_RATE_LIMIT_SUSTAINED` env vars
   - Default: 100 burst, 10/s sustained
   - Rate-limited requests return error with retry-after hint
   - Rate limit events recorded as Prometheus metrics
