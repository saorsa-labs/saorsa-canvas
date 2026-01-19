# Saorsa Canvas Deployment Guide

Complete guide for deploying Saorsa Canvas in various environments.

## Table of Contents

- [Quick Start](#quick-start)
- [Docker Deployment](#docker-deployment)
- [Kubernetes Deployment](#kubernetes-deployment)
- [Security Considerations](#security-considerations)
- [Monitoring](#monitoring)
- [Troubleshooting](#troubleshooting)

---

## Quick Start

### Prerequisites

- Rust 1.75+ (for building)
- Modern web browser (for client)

### Build from Source

```bash
# Clone the repository
git clone https://github.com/saorsa-labs/saorsa-canvas.git
cd saorsa-canvas

# Build release binary
cargo build --release

# Binary location
ls -la target/release/canvas-server
```

### Run the Server

```bash
# Run with defaults
./target/release/canvas-server

# Output:
# 2024-01-19T10:00:00.000Z  INFO canvas_server: Saorsa Canvas server starting on http://127.0.0.1:9473
# 2024-01-19T10:00:00.001Z  INFO canvas_server: Open http://localhost:9473 in your browser
```

### Verify Installation

```bash
# Health check
curl http://localhost:9473/health/ready

# Expected response:
# {"status":"healthy","version":"0.1.0","checks":{"scene_store":true,"websocket":true}}
```

### Access the Web UI

Open http://localhost:9473 in your browser.

---

## Docker Deployment

### Dockerfile

```dockerfile
# Build stage
FROM rust:1.75-slim-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy source
COPY . .

# Build release binary
RUN cargo build --release -p canvas-server

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary
COPY --from=builder /app/target/release/canvas-server /app/canvas-server

# Copy web assets
COPY --from=builder /app/web /app/web

# Create non-root user
RUN useradd -r -u 1000 canvas
USER canvas

# Expose port
EXPOSE 9473

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:9473/health/live || exit 1

# Run server
CMD ["./canvas-server"]
```

### Build Docker Image

```bash
docker build -t saorsa-canvas:latest .
```

### Run Docker Container

```bash
# Basic run
docker run -p 9473:9473 saorsa-canvas:latest

# With environment variables
docker run -p 9473:9473 \
    -e RUST_LOG=info \
    -e RUST_LOG_FORMAT=json \
    saorsa-canvas:latest

# With Communitas integration
docker run -p 9473:9473 \
    -e COMMUNITAS_MCP_URL=http://communitas:8080/mcp \
    -e COMMUNITAS_MCP_TOKEN=secret \
    saorsa-canvas:latest
```

### Docker Compose

```yaml
# docker-compose.yml
version: '3.8'

services:
  canvas:
    build: .
    image: saorsa-canvas:latest
    ports:
      - "9473:9473"
    environment:
      RUST_LOG: info,canvas_server=info
      RUST_LOG_FORMAT: json
      WS_RATE_LIMIT_BURST: "100"
      WS_RATE_LIMIT_SUSTAINED: "10"
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:9473/health/live"]
      interval: 30s
      timeout: 3s
      retries: 3
      start_period: 5s
    restart: unless-stopped

  # Optional: Prometheus for metrics
  prometheus:
    image: prom/prometheus:latest
    ports:
      - "9090:9090"
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml
    depends_on:
      - canvas
```

### Prometheus Configuration

```yaml
# prometheus.yml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: 'canvas'
    static_configs:
      - targets: ['canvas:9473']
    metrics_path: /metrics
```

---

## Kubernetes Deployment

### Deployment

```yaml
# deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: canvas-server
  labels:
    app: canvas
spec:
  replicas: 1
  selector:
    matchLabels:
      app: canvas
  template:
    metadata:
      labels:
        app: canvas
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/port: "9473"
        prometheus.io/path: "/metrics"
    spec:
      securityContext:
        runAsNonRoot: true
        runAsUser: 1000
        fsGroup: 1000
      containers:
        - name: canvas
          image: saorsa-canvas:latest
          ports:
            - containerPort: 9473
              name: http
          envFrom:
            - configMapRef:
                name: canvas-config
            - secretRef:
                name: canvas-secrets
                optional: true
          resources:
            requests:
              memory: "64Mi"
              cpu: "100m"
            limits:
              memory: "256Mi"
              cpu: "500m"
          livenessProbe:
            httpGet:
              path: /health/live
              port: http
            initialDelaySeconds: 5
            periodSeconds: 10
            timeoutSeconds: 3
            failureThreshold: 3
          readinessProbe:
            httpGet:
              path: /health/ready
              port: http
            initialDelaySeconds: 5
            periodSeconds: 10
            timeoutSeconds: 3
            failureThreshold: 3
          securityContext:
            allowPrivilegeEscalation: false
            readOnlyRootFilesystem: true
            capabilities:
              drop:
                - ALL
```

### Service

```yaml
# service.yaml
apiVersion: v1
kind: Service
metadata:
  name: canvas-server
  labels:
    app: canvas
spec:
  type: ClusterIP
  ports:
    - port: 9473
      targetPort: http
      protocol: TCP
      name: http
  selector:
    app: canvas
```

### ConfigMap

```yaml
# configmap.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: canvas-config
data:
  RUST_LOG: "info,canvas_server=info"
  RUST_LOG_FORMAT: "json"
  WS_RATE_LIMIT_BURST: "100"
  WS_RATE_LIMIT_SUSTAINED: "10"
```

### Secret (Optional)

```yaml
# secret.yaml
apiVersion: v1
kind: Secret
metadata:
  name: canvas-secrets
type: Opaque
stringData:
  COMMUNITAS_MCP_URL: "http://communitas.default.svc:8080/mcp"
  COMMUNITAS_MCP_TOKEN: "your-secret-token"
```

### Apply Resources

```bash
kubectl apply -f configmap.yaml
kubectl apply -f secret.yaml  # Optional
kubectl apply -f deployment.yaml
kubectl apply -f service.yaml
```

### Port Forward for Testing

```bash
kubectl port-forward svc/canvas-server 9473:9473
```

### Ingress (Optional)

**Important**: The canvas server binds to localhost only and has localhost-only CORS. For external access, you would need a reverse proxy that handles the security boundary.

```yaml
# ingress.yaml (for internal/trusted networks only)
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: canvas-ingress
  annotations:
    nginx.ingress.kubernetes.io/proxy-read-timeout: "3600"
    nginx.ingress.kubernetes.io/proxy-send-timeout: "3600"
    nginx.ingress.kubernetes.io/websocket-services: "canvas-server"
spec:
  rules:
    - host: canvas.internal
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: canvas-server
                port:
                  number: 9473
```

---

## Security Considerations

### Localhost-Only Binding

The server binds to `127.0.0.1` only, not `0.0.0.0`. This is intentional:

- Prevents external network access
- Reduces attack surface
- Designed for local-first use

**In Docker/Kubernetes**: The container's localhost is isolated. Use port mapping or services to expose the endpoint.

### CORS Restrictions

CORS is restricted to localhost origins:
- `http://localhost:9473`
- `http://127.0.0.1:9473`
- Common dev ports (3000, 5173, 8080)

This prevents cross-origin requests from external sites.

### Rate Limiting

WebSocket connections are rate-limited to prevent abuse:
- **Burst**: 100 messages (prevents burst attacks)
- **Sustained**: 10 messages/second (prevents sustained abuse)

Configure via environment variables for your use case.

### Input Validation

All inputs are validated:
- **Session IDs**: Alphanumeric + `-_`, max 64 chars
- **Element IDs**: Same pattern
- **SDP/ICE**: Format and size validation (64KB limit)

Validation failures are:
- Logged for monitoring
- Counted in Prometheus metrics
- Returned as 400 Bad Request

### Sensitive Data

**DO**:
- Store tokens in secrets management (K8s Secrets, Vault, etc.)
- Use environment variables for configuration
- Enable JSON logging for structured log aggregation

**DON'T**:
- Log sensitive data (tokens, credentials)
- Hardcode secrets in code or config files
- Commit secrets to version control

### Container Security

The Docker/Kubernetes configurations include:
- Non-root user (UID 1000)
- Read-only root filesystem
- Dropped capabilities
- No privilege escalation

---

## Monitoring

### Prometheus Metrics

Scrape metrics from `/metrics`:

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'canvas'
    static_configs:
      - targets: ['canvas-server:9473']
```

### Key Metrics to Watch

| Metric | Alert Threshold | Description |
|--------|-----------------|-------------|
| `canvas_http_requests_total{status="5xx"}` | > 0 | Server errors |
| `canvas_http_request_duration_seconds` | p99 > 1s | Slow requests |
| `canvas_ws_connections_active` | > 1000 | Connection exhaustion |
| `canvas_rate_limited_total` | spike | Potential abuse |
| `canvas_validation_failures_total` | spike | Invalid input attempts |

### Grafana Dashboard

Example dashboard JSON available in `docs/grafana/canvas-dashboard.json` (if created).

### Log Aggregation

Enable JSON logging for log aggregation:

```bash
export RUST_LOG_FORMAT=json
```

Example structured log:
```json
{
  "timestamp": "2024-01-19T10:30:00.123Z",
  "level": "INFO",
  "target": "canvas_server::main",
  "message": "WebSocket connection upgrade requested",
  "span": {
    "name": "websocket_connect"
  }
}
```

Compatible with:
- Elasticsearch/Kibana
- Loki/Grafana
- CloudWatch Logs
- Datadog

### Health Checks

| Endpoint | Purpose | Kubernetes Probe |
|----------|---------|------------------|
| `/health/live` | Process alive | Liveness |
| `/health/ready` | Ready for traffic | Readiness |
| `/health` | Both (backward compat) | Either |

---

## Troubleshooting

### Server Won't Start

**Port in use**:
```bash
# Find process using port
lsof -i :9473

# Kill process or use different port
export CANVAS_PORT=9474
```

**Missing web directory**:
```
Error: No such file or directory (os error 2)
```
Ensure `web/` directory is present relative to the binary.

### Container Exits Immediately

Check logs:
```bash
docker logs <container-id>
kubectl logs deployment/canvas-server
```

Common issues:
- Missing environment variables
- Port conflicts
- Permission denied (non-root user)

### Health Check Failures

**Liveness fails** (container restarts):
- Server crashed - check logs for panics
- Resource exhaustion - increase limits

**Readiness fails** (removed from LB):
- Scene store inaccessible - check memory
- Dependency unavailable - check Communitas connection

### WebSocket Connection Issues

**Connection refused**:
- Verify server is running
- Check port mapping
- Verify localhost binding

**Connection drops**:
- Rate limiting - check metrics
- Timeout - check proxy configuration

For Nginx/Ingress, ensure WebSocket support:
```nginx
proxy_http_version 1.1;
proxy_set_header Upgrade $http_upgrade;
proxy_set_header Connection "upgrade";
proxy_read_timeout 3600s;
```

### Communitas Connection Fails

1. **Verify URL is reachable**:
   ```bash
   curl -v $COMMUNITAS_MCP_URL
   ```

2. **Check authentication**:
   ```bash
   curl -H "Authorization: Bearer $COMMUNITAS_MCP_TOKEN" $COMMUNITAS_MCP_URL
   ```

3. **Enable debug logging**:
   ```bash
   export RUST_LOG=debug,canvas_server=trace
   ```

### Performance Issues

**High latency**:
- Check resource limits (CPU, memory)
- Review rate limiting settings
- Profile with `RUST_LOG=trace`

**Memory growth**:
- Check element count per session
- Review WebSocket connection count
- Look for memory leaks in metrics

### Debug Mode

Enable maximum verbosity:
```bash
export RUST_LOG=trace
export RUST_LOG_FORMAT=text  # Easier to read
./canvas-server 2>&1 | less
```

---

## Appendix: Resource Requirements

### Minimum Requirements

| Resource | Value |
|----------|-------|
| CPU | 100m |
| Memory | 64Mi |
| Disk | 50Mi (binary + assets) |

### Recommended Production

| Resource | Value |
|----------|-------|
| CPU | 500m |
| Memory | 256Mi |
| Disk | 100Mi |

### Per-Connection Overhead

| Resource | Per Connection |
|----------|----------------|
| Memory | ~10KB |
| File descriptors | 1 |

For 1000 concurrent connections: ~10MB additional memory.
