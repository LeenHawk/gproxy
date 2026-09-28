FROM node:24-trixie AS node
FROM golang:1.27.1-trixie AS go
FROM rust:1-trixie
COPY --from=node /usr/local/bin/node /usr/local/bin/node
COPY --from=node /usr/local/lib/node_modules /usr/local/lib/node_modules
COPY --from=go /usr/local/go /usr/local/go
ENV PATH="/usr/local/go/bin:${PATH}"
RUN ln -s /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm \
    && npm install -g pnpm@9.15.9 @cnbcool/cnb-cli@1.16.18 \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
       clang cmake libclang-dev pkg-config jq zip unzip python3 openssl \
       libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libdbus-1-dev \
       libayatana-appindicator3-dev librsvg2-dev patchelf \
    && rm -rf /var/lib/apt/lists/* \
    && rustup component add clippy rustfmt \
    && rustup target add wasm32-unknown-unknown
