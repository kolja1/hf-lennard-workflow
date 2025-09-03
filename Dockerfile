# Multi-stage build for Rust workflow service
FROM rust:1.82-bookworm AS builder

# Install protobuf compiler
RUN apt-get update && apt-get install -y \
    protobuf-compiler \
    libprotobuf-dev \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /usr/src/app

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY proto/ ./proto/
COPY contracts/ ./contracts/

# Copy all crate sources
COPY crates/ ./crates/

# Build the release binary
RUN cargo build --release --bin workflow-server

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 -s /bin/bash workflow

# Create base directories only - let the app create its own structure
RUN mkdir -p /data /app/config \
    && chown -R workflow:workflow /data /app

# Copy the binary from builder
COPY --from=builder /usr/src/app/target/release/workflow-server /usr/local/bin/workflow-server

# Copy configuration files
COPY config/ /app/config/

# Switch to non-root user
USER workflow

# Set working directory
WORKDIR /app

# Expose gRPC port
EXPOSE 50051

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ["workflow-server", "--help"]

# Default command: start gRPC server with workflow monitoring on port 50051
CMD ["workflow-server", "--grpc-server", "--grpc-port", "50051", "--config", "/app/config/credentials.json"]