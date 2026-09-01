# Multi-stage Dockerfile for DooPack production
FROM rust:1.80-slim as builder
WORKDIR /usr/src/doopack

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libsqlite3-dev \
    curl \
    xz-utils \
    && rm -rf /var/lib/apt/lists/*

# Install WebAssembly target and Trunk for frontend compilation
RUN rustup target add wasm32-unknown-unknown
RUN arch=$(dpkg --print-architecture) && \
    if [ "$arch" = "arm64" ]; then \
        TRUNK_ARCH="aarch64"; \
    else \
        TRUNK_ARCH="x86_64"; \
    fi && \
    curl -L "https://github.com/trunk-rs/trunk/releases/download/v0.21.14/trunk-${TRUNK_ARCH}-unknown-linux-gnu.tar.gz" | tar -xzf- -C /usr/local/bin

# Copy workspace configurations and sources
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY sdks ./sdks

# Build frontend static assets (WebAssembly)
WORKDIR /usr/src/doopack/crates/frontend
RUN trunk build --release

# Build backend server binary
WORKDIR /usr/src/doopack
RUN cargo build --release -p server

# --- Backend Production Stage ---
FROM debian:bookworm-slim as backend
WORKDIR /app

RUN apt-get update && apt-get install -y \
    openssl \
    ca-certificates \
    libsqlite3-0 \
    curl \
    gnupg \
    && rm -rf /var/lib/apt/lists/*

# Install Docker CLI so the orchestrator can build & run container runners on host
RUN install -m 0755 -d /etc/apt/keyrings \
    && curl -fsSL https://download.docker.com/linux/debian/gpg | gpg --dearmor -o /etc/apt/keyrings/docker.gpg \
    && chmod a+r /etc/apt/keyrings/docker.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian bookworm stable" | \
    tee /etc/apt/sources.list.d/docker.list > /dev/null \
    && apt-get update && apt-get install -y docker-ce-cli && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /usr/src/doopack/target/release/server /usr/local/bin/doopack-server

# Create data and runtime directories
RUN mkdir -p /app/data /app/services_runtime

ENV DATABASE_URL="sqlite:///app/data/orchestrator.db"
ENV REDIS_URL="redis://:redisroot@redis:6379"
ENV RUST_LOG="info"

EXPOSE 4500
CMD ["/usr/local/bin/doopack-server"]

# --- Frontend Production Stage ---
FROM nginx:alpine as frontend
COPY --from=builder /usr/src/doopack/crates/frontend/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf
EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]

