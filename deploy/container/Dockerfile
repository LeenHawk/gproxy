FROM public.ecr.aws/docker/library/node:lts-trixie-slim AS console

ARG GPROXY_BUILD_HASH=unknown
WORKDIR /source
COPY Cargo.toml ./
WORKDIR /source/console
RUN corepack enable
COPY console/package.json console/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY console/ ./
RUN GPROXY_BUILD_HASH="$GPROXY_BUILD_HASH" pnpm build

FROM scratch AS console-dist
COPY --from=console /source/console/dist/ /

FROM public.ecr.aws/docker/library/rust:1-trixie AS builder

ARG CARGO_NET_OFFLINE=false
ARG GPROXY_UPDATE_PUBKEY
ARG GPROXY_BUILD_VERSION
ARG GPROXY_BUILD_CHANNEL=releases
ARG GPROXY_BUILD_HASH
ARG GPROXY_INSTALLATION_KIND=container
WORKDIR /source
RUN apt-get update \
    && apt-get install -y --no-install-recommends clang cmake libclang-dev \
    && rm -rf /var/lib/apt/lists/*
COPY . .
COPY --from=console /source/console/dist/ crates/gproxy-host-axum/assets/web/
RUN test -n "$GPROXY_UPDATE_PUBKEY" \
    && test "$(printf '%s' "$GPROXY_UPDATE_PUBKEY" | base64 -d | wc -c)" -eq 32 \
    && CARGO_NET_OFFLINE="$CARGO_NET_OFFLINE" GPROXY_UPDATE_PUBKEY="$GPROXY_UPDATE_PUBKEY" \
    GPROXY_BUILD_VERSION="$GPROXY_BUILD_VERSION" GPROXY_BUILD_CHANNEL="$GPROXY_BUILD_CHANNEL" \
    GPROXY_BUILD_HASH="$GPROXY_BUILD_HASH" GPROXY_INSTALLATION_KIND="$GPROXY_INSTALLATION_KIND" \
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
    && install -d -o gproxy -g gproxy /var/lib/gproxy

COPY --from=builder /source/target/release/gproxy /usr/local/bin/gproxy

USER gproxy
EXPOSE 8787
ENV GPROXY_HOST=0.0.0.0 \
    GPROXY_PORT=8787 \
    GPROXY_DATA_DIR=/var/lib/gproxy \
    GPROXY_PERSISTENCE=sqlite
STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/local/bin/gproxy"]
