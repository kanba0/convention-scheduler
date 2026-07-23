# syntax=docker/dockerfile:1

# ---- builder: compile the release binary ----
FROM rust:1-slim-bookworm AS builder
WORKDIR /app

# `ring` (pulled in by sqlx's rustls TLS) has C/asm that needs a compiler.
RUN apt-get update \
    && apt-get install -y --no-install-recommends build-essential \
    && rm -rf /var/lib/apt/lists/*

# Query verification uses the committed .sqlx cache, so the build needs no DB.
ENV SQLX_OFFLINE=true

COPY . .
RUN cargo build --release --locked

# ---- runtime: just the binary ----
FROM debian:bookworm-slim AS runtime

# CA roots for the rustls TLS client (Postgres over TLS in production).
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# The migrations are embedded in the binary at compile time (sqlx::migrate!),
# so the runtime image carries only the executable.
COPY --from=builder /app/target/release/convention_scheduler /usr/local/bin/convention_scheduler

EXPOSE 8080
CMD ["convention_scheduler"]
