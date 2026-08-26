FROM public.ecr.aws/docker/library/node:lts-trixie-slim AS console

WORKDIR /source/console
RUN corepack enable
COPY console/package.json console/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY console/ ./
RUN pnpm build

FROM public.ecr.aws/docker/library/rust:1-trixie AS builder

ARG CARGO_NET_OFFLINE=false
WORKDIR /source
RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake \
    && rm -rf /var/lib/apt/lists/*
COPY . .
COPY --from=console /source/console/dist/ crates/gproxy-host-axum/assets/web/
RUN CARGO_NET_OFFLINE="$CARGO_NET_OFFLINE" \
    cargo build --locked --release --package gproxy-host-axum --bin gproxy

FROM public.ecr.aws/docker/library/debian:trixie-slim

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
