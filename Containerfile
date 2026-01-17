# Stage 1: Builder
# Use a specific Rust version for reproducibility (verified 2026-01-17, Rust 1.92 is latest stable)
FROM rust:1.92-slim-bullseye AS builder

WORKDIR /app

# Install system dependencies required for compilation (e.g., OpenSSL for some crates)
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

# Copy only the Cargo.toml files first to leverage Docker layer caching
COPY Cargo.toml ./
COPY crates/antigravity-core/Cargo.toml ./crates/antigravity-core/
COPY crates/antigravity-shared/Cargo.toml ./crates/antigravity-shared/
COPY antigravity-server/Cargo.toml ./antigravity-server/

# This step will cache dependencies
# Use a dummy main.rs to build dependencies and cache them
RUN mkdir -p antigravity-server/src && \
    echo "fn main() {println!(\"dummy\");}" > antigravity-server/src/main.rs && \
    cargo build --release -p antigravity-server || true

# Copy the rest of the source code
COPY . .

# Build the main application
# --locked ensures reproducible builds using Cargo.lock
# --offline can be used if all dependencies are vendored
RUN cargo build --release -p antigravity-server --locked

# Copy frontend static files
# Frontend is built separately via Nix in src-leptos/dist
# This assumes src-leptos/dist is present in the build context
# If not, it needs to be built as part of the Nix build process before image creation
COPY src-leptos/dist ./src-leptos/dist

# Stage 2: Runtime
FROM debian:bullseye-slim

WORKDIR /app

# Install runtime dependencies if any (e.g., CA certificates)
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy the built binary from the builder stage
COPY --from=builder /app/target/release/antigravity-server /usr/local/bin/antigravity-server

# Copy static files (Leptos UI)
COPY --from=builder /app/src-leptos/dist /app/src-leptos/dist

# Expose the port (default 8045)
EXPOSE 8045

# Set environment variables
ENV RUST_LOG=info
ENV ANTIGRAVITY_PORT=8045
ENV ANTIGRAVITY_STATIC_DIR=/app/src-leptos/dist

# Run the application
CMD ["antigravity-server"]
