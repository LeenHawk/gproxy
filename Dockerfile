FROM public.ecr.aws/docker/library/node:22.23.2-bookworm-slim@sha256:83f487e0a63425e5b4d146fb5e5be574bcbe1b7b843d3ebafdd95eaf7767a7e5 AS console

WORKDIR /source/console
RUN corepack enable
COPY console/package.json console/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY console/ ./
RUN pnpm build

FROM public.ecr.aws/docker/library/rust:1.96.1-bookworm@sha256:d99f7b31f49909348dc59b51f3c95d1efded1701ffb222f095aaab7de3c4abd8 AS builder

ARG CARGO_NET_OFFLINE=false
WORKDIR /source
RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake \
    && rm -rf /var/lib/apt/lists/*
COPY . .
COPY --from=console /source/console/dist/ crates/gproxy-host-axum/assets/web/
RUN CARGO_NET_OFFLINE="$CARGO_NET_OFFLINE" \
    cargo build --locked --release --package gproxy-host-axum --bin gproxy

FROM public.ecr.aws/docker/library/debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171

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
