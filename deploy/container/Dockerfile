FROM docker.io/library/node:24.22.1-trixie-slim@sha256:43b9ba1142b8dcf33fa8f922ca1be85714adc444e0586c7cf7726daaeabc1bea AS console

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

FROM docker.io/library/rust:1.99.0-trixie@sha256:81015672d6939b0265f09adf96d5de125482bb1330106076956b0518e74edc90 AS builder

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

FROM docker.io/library/debian:trixie-20261005-slim@sha256:a29215f6a35e51e22adffa17f89e9d2ef06214e64a2bad10d765c46aea49f11f

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
