# Build stage - Leptos 0.8 requires Rust nightly (1.88+)
FROM rustlang/rust:nightly-bookworm-slim AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js for Tailwind
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build CSS with Tailwind
WORKDIR /app/crates/bp-web
RUN npm install
RUN mkdir -p /app/target/site/pkg
RUN npx @tailwindcss/cli --input style/tailwind.css --output /app/target/site/pkg/bp-web.css --minify

# Copy static assets from public folder
RUN cp -r public/* /app/target/site/

# Build bp-web (no validator-accounting at build time — report generated on the volume at runtime)
WORKDIR /app
RUN cargo build -p bp-web --features ssr --release

# Also build validator-accounting so it's available for on-demand report generation
RUN cargo build -p validator-accounting --release || true

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    sqlite3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binaries and static assets
COPY --from=builder /app/target/release/bp-web /app/bp-web
COPY --from=builder /app/target/site /app/target/site
# Copy validator-accounting if it built successfully
COPY --from=builder /app/target/release/validator-accounting /app/validator-accounting

# Create data directory (will be replaced by Fly volume mount)
RUN mkdir -p /data

# Fly uses 8080 by default; also support $PORT for compatibility
ENV PORT=8080
ENV LEPTOS_SITE_ADDR=0.0.0.0:8080
ENV LEPTOS_SITE_ROOT=target/site
ENV DATA_DIR=/data

EXPOSE 8080

CMD ["./bp-web", "--data-dir", "/data"]
