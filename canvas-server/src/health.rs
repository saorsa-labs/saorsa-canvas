//! Health check endpoints for Kubernetes probes.
//!
//! Provides liveness and readiness probes for container orchestration:
//! - `/health/live` - Liveness probe (restart if fails)
//! - `/health/ready` - Readiness probe (remove from LB if fails)
//! - `/health` - Combined check for backward compatibility

use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;

use crate::AppState;

/// Health status response.
#[derive(Debug, Serialize)]
pub struct HealthStatus {
    /// Overall status: "healthy" or "unhealthy"
    pub status: &'static str,
    /// Server version
    pub version: &'static str,
    /// Individual component checks
    pub checks: HealthChecks,
}

/// Individual health checks.
#[derive(Debug, Serialize)]
pub struct HealthChecks {
    /// Scene store accessible
    pub scene_store: bool,
    /// WebSocket handler ready
    pub websocket: bool,
}

/// Liveness probe - is the server running?
///
/// Returns 200 OK if the process is alive.
/// Kubernetes will restart the pod if this fails.
#[tracing::instrument(name = "liveness_probe")]
pub async fn liveness() -> StatusCode {
    StatusCode::OK
}

/// Readiness probe - is the server ready to accept traffic?
///
/// Checks that all dependencies are available.
/// Kubernetes will remove the pod from the load balancer if this fails.
#[tracing::instrument(name = "readiness_probe", skip(state))]
pub async fn readiness(State(state): State<AppState>) -> (StatusCode, Json<HealthStatus>) {
    // Check scene store is accessible by retrieving the default scene
    // This exercises the RwLock and verifies the store is functional
    let scene_ok = state.sync.store().get("default").is_some();

    // WebSocket is always ready if server is up
    let ws_ok = true;

    let all_ok = scene_ok && ws_ok;

    let status = HealthStatus {
        status: if all_ok { "healthy" } else { "unhealthy" },
        version: env!("CARGO_PKG_VERSION"),
        checks: HealthChecks {
            scene_store: scene_ok,
            websocket: ws_ok,
        },
    };

    let code = if all_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (code, Json(status))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_serialization() {
        let status = HealthStatus {
            status: "healthy",
            version: "0.1.0",
            checks: HealthChecks {
                scene_store: true,
                websocket: true,
            },
        };

        let json = serde_json::to_string(&status).expect("should serialize");
        assert!(json.contains("healthy"));
        assert!(json.contains("0.1.0"));
        assert!(json.contains("scene_store"));
        assert!(json.contains("websocket"));
    }

    #[test]
    fn test_health_status_unhealthy() {
        let status = HealthStatus {
            status: "unhealthy",
            version: "0.1.0",
            checks: HealthChecks {
                scene_store: false,
                websocket: true,
            },
        };

        let json = serde_json::to_string(&status).expect("should serialize");
        assert!(json.contains("unhealthy"));
        assert!(json.contains("false")); // scene_store: false
    }
}
