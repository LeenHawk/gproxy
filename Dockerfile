FROM public.ecr.aws/docker/library/node:lts-trixie-slim AS console

WORKDIR /source/console
RUN corepack enable
COPY console/package.json console/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY console/ ./
RUN pnpm build

FROM scratch AS console-dist
COPY --from=console /source/console/dist/ /

FROM public.ecr.aws/docker/library/rust:1-trixie AS builder

ARG CARGO_NET_OFFLINE=false
ARG GPROXY_UPDATE_PUBKEY
WORKDIR /source
RUN apt-get update \
    && apt-get install -y --no-install-recommends clang cmake libclang-dev \
    && rm -rf /var/lib/apt/lists/*
COPY . .
COPY --from=console /source/console/dist/ crates/gproxy-host-axum/assets/web/
RUN test -n "$GPROXY_UPDATE_PUBKEY" \
    && test "$(printf '%s' "$GPROXY_UPDATE_PUBKEY" | base64 -d | wc -c)" -eq 32 \
    && CARGO_NET_OFFLINE="$CARGO_NET_OFFLINE" GPROXY_UPDATE_PUBKEY="$GPROXY_UPDATE_PUBKEY" \
    cargo build --locked --release --package gproxy-host-axum --bin gproxy

FROM public.ecr.aws/docker/library/debian:trixie-slim

ARG GPROXY_VERSION
ARG GPROXY_REVISION
LABEL org.opencontainers.image.version="$GPROXY_VERSION" \
      org.opencontainers.image.revision="$GPROXY_REVISION" \
      org.opencontainers.image.source="https://github.com/LeenHawk/gproxy"

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system gproxy \
    && useradd --system --gid gproxy --home-dir /var/lib/gproxy gproxy \
    && install -d -o gproxy -g gproxy /var/lib/gproxy /etc/gproxy

COPY --from=builder /source/target/release/gproxy /usr/local/bin/gproxy
COPY deploy/container/gproxy.toml /etc/gproxy/gproxy.toml

USER gproxy
EXPOSE 8787
ENTRYPOINT ["/usr/local/bin/gproxy", "/etc/gproxy/gproxy.toml"]
