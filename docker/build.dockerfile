FROM rust:bookworm AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./

ENV SQLX_OFFLINE=true
RUN mkdir -p src && echo "fn main() {}" > src/main.rs \
    && cargo build --release --bin services_manager_server \
    && rm -rf src

COPY . .

RUN cargo build --release --bin services_manager_server


FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y openssl ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

RUN update-ca-certificates

RUN groupadd --system appuser && useradd --system --gid appuser --no-create-home appuser

COPY ./scripts/start.sh /
RUN chmod +x /start.sh && chown appuser:appuser /start.sh

WORKDIR /app
RUN chown appuser:appuser /app

COPY --from=builder /app/target/release/services_manager_server /usr/local/bin
RUN chown appuser:appuser /usr/local/bin/services_manager_server \
    && chmod 755 /usr/local/bin/services_manager_server

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 CMD curl -f http://localhost:${PORT:-8080}/health || exit 1

USER appuser

ENTRYPOINT ["/start.sh"]
