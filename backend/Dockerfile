# Build stage - using latest stable Rust
FROM rust:latest AS builder

WORKDIR /app

# Copy manifests
COPY Cargo.toml ./

# Create dummy main.rs to cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Build dependencies (this layer will be cached)
RUN cargo build --release && rm -rf src

# Copy actual source code
COPY src ./src

# Touch main.rs to ensure rebuild
RUN touch src/main.rs

# Build the actual application
RUN cargo build --release

# Runtime stage - using debian slim for cross-platform support
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from builder
COPY --from=builder /app/target/release/mysql-executor /app/mysql-executor

# Create directories for SQL files and config
RUN mkdir -p /app/sql /app/config

# Set environment variables
ENV RUST_LOG=info
ENV MYSQL_EXECUTOR_CONFIG=/app/config/config.toml

# Run the application
CMD ["/app/mysql-executor"]
