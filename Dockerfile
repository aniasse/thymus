FROM rust:1.93-slim AS builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/thymus-core /usr/local/bin/
COPY --from=builder /app/target/release/thymus-sensor /usr/local/bin/
COPY crates/core/static/ /opt/thymus/static/
WORKDIR /opt/thymus
EXPOSE 9443
