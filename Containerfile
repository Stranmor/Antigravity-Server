# Antigravity Server - Multi-stage Podman Build
# Build: podman build -t antigravity-manager:latest .
# Run: podman run -d -p 8045:8045 -v ~/.antigravity_tools:/data antigravity-manager:latest

# =============================================================================
# Stage 1: Build Frontend (Leptos WASM)
# =============================================================================
FROM docker.io/rust:1.88-bookworm AS frontend-builder

# Install trunk and wasm target
RUN cargo install trunk --locked \
    && rustup target add wasm32-unknown-unknown

WORKDIR /build

# Copy frontend source and workspace files
COPY src-leptos/ src-leptos/
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY antigravity-server/ antigravity-server/
COPY antigravity-vps-cli/ antigravity-vps-cli/

# Build frontend
WORKDIR /build/src-leptos
RUN trunk build --release

# =============================================================================
# Stage 2: Build Backend (Rust Binary)
# =============================================================================
FROM docker.io/rust:1.88-bookworm AS backend-builder

WORKDIR /build

# Copy everything needed for build
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY antigravity-server/ antigravity-server/
COPY antigravity-vps-cli/ antigravity-vps-cli/
COPY src-leptos/ src-leptos/
COPY vendor/ vendor/

# Copy pre-built frontend from stage 1
COPY --from=frontend-builder /build/src-leptos/dist/ src-leptos/dist/

# Build release binary (skip trunk in build.rs since we have dist/)
ENV SKIP_TRUNK_BUILD=1
RUN cargo build --release -p antigravity-server

# =============================================================================
# Stage 3: Runtime (Minimal Debian)
# =============================================================================
FROM docker.io/debian:bookworm-slim AS runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 antigravity

WORKDIR /app

# Copy binary
COPY --from=backend-builder /build/target/release/antigravity-server /usr/local/bin/

# Copy frontend assets
COPY --from=frontend-builder /build/src-leptos/dist/ /app/dist/

# Create data directory
RUN mkdir -p /data && chown antigravity:antigravity /data

USER antigravity

# Environment
ENV RUST_LOG=info
ENV ANTIGRAVITY_PORT=8045
ENV ANTIGRAVITY_STATIC_DIR=/app/dist
ENV ANTIGRAVITY_DATA_DIR=/data

EXPOSE 8045

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -sf http://localhost:8045/api/health || exit 1

CMD ["antigravity-server"]
