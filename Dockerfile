# syntax=docker/dockerfile:1
FROM node:lts-alpine3.23 AS frontend

WORKDIR /app

COPY apps/gproxy/frontend/package.json apps/gproxy/frontend/pnpm-lock.yaml ./apps/gproxy/frontend/
RUN corepack enable \
    && cd apps/gproxy/frontend \
    && pnpm install --frozen-lockfile

COPY apps/gproxy/frontend ./apps/gproxy/frontend
RUN cd apps/gproxy/frontend && pnpm build

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
COPY crates ./crates
COPY apps ./apps
COPY src ./src
COPY route.md ./route.md

COPY --from=frontend /app/apps/gproxy/frontend/dist ./apps/gproxy/frontend/dist

RUN cargo build --release -p gproxy \
    && upx --best --lzma target/release/gproxy

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/gproxy /usr/local/bin/gproxy

ENV GPROXY_HOST=0.0.0.0
ENV GPROXY_PORT=8787
ENV GPROXY_DATA_DIR=/app/data

EXPOSE 8787

CMD ["/bin/sh", "-c", "/usr/local/bin/gproxy --host ${GPROXY_HOST} --port ${GPROXY_PORT} --admin-key ${GPROXY_ADMIN_KEY:-pwd} ${GPROXY_DSN:+--dsn ${GPROXY_DSN}} ${GPROXY_DATA_DIR:+--data-dir ${GPROXY_DATA_DIR}} ${GPROXY_PROXY:+--proxy ${GPROXY_PROXY}}"]
