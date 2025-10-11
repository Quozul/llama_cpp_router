# syntax=docker/dockerfile:1.7-labs
FROM rust:1.90-slim-trixie AS builder

ARG BINARY_NAME=llama_cpp_router

WORKDIR /usr/src/app
COPY --parents ./Cargo.toml ./Cargo.lock ./src ./

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/src/app/target \
    cargo build --release --bin $BINARY_NAME && \
    cp target/release/$BINARY_NAME /usr/local/bin/app

FROM debian:trixie-slim

WORKDIR /app

RUN apt-get update && \
    apt-get install -y rocm-smi && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder --chown=nonroot:nonroot /usr/local/bin/app /usr/local/bin/app

CMD ["app"]
