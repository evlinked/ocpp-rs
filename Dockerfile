# Multi-stage build for ocpp-rs
FROM rust:1.88-slim as builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN addgroup --gid 1001 --system ocpp && \
    adduser --no-create-home --shell /bin/false --disabled-password --uid 1001 --system --group ocpp

# Set working directory
WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

# Build dependencies (this layer will be cached)
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -f target/release/deps/ocpp*

# Build the actual application
COPY . .
RUN cargo build --release --bin ocpp-cli

# Runtime stage
FROM debian:bookworm-slim as runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libpq5 \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN addgroup --gid 1001 --system ocpp && \
    adduser --no-create-home --shell /bin/false --disabled-password --uid 1001 --system --group ocpp

# Create app directory
WORKDIR /app

# Copy the binary from builder stage
COPY --from=builder /app/target/release/ocpp-cli /usr/local/bin/ocpp-cli

# Copy configuration template
COPY --from=builder /app/config/ /app/config/

# Create necessary directories
RUN mkdir -p /app/logs /app/data && \
    chown -R ocpp:ocpp /app

# Switch to non-root user
USER ocpp

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ocpp-cli health-check || exit 1

# Expose default ports
EXPOSE 8080 9090

# Default environment
ENV RUST_LOG=info
ENV RUST_BACKTRACE=1
ENV OCPP_CONFIG_PATH=/app/config/default.toml

# Default command
CMD ["ocpp-cli", "csms", "start"]

# Multi-target builds
FROM runtime as csms
CMD ["ocpp-cli", "csms", "start"]

FROM runtime as cp-simulator
CMD ["ocpp-cli", "simulator", "start"]

FROM runtime as conformance
CMD ["ocpp-cli", "conformance", "run"]
