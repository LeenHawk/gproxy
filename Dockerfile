# syntax=docker/dockerfile:1
FROM rust:1.92-slim-trixie AS builder

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        git \
        pkg-config \
        libssl-dev \
        ca-certificates \
        cmake \
        ninja-build \
        perl \
        upx-ucl \
        libclang-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY front ./front
COPY README.md README_zh.md LICENSE ./

RUN cargo build --release \
    && upx --best --lzma target/release/gproxy

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/gproxy /usr/local/bin/gproxy

EXPOSE 8787

CMD ["/usr/local/bin/gproxy"]
