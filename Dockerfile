# GPROXY v2 runtime image.
#
# Built by .github/workflows/release.yml (the `docker-build` job) from a
# *prebuilt* Linux binary — NOT from source. The binary is compiled and
# UPX-compressed in `build-native` and handed to this Dockerfile as
# `dist/gproxy`, so the image is just a thin runtime wrapper.
#
#   RUNTIME_BASE=gcr.io/distroless/cc-debian13      # glibc (gnu) builds
#   RUNTIME_BASE=gcr.io/distroless/static-debian13  # static (musl) builds
ARG RUNTIME_BASE=gcr.io/distroless/cc-debian13
FROM ${RUNTIME_BASE}

WORKDIR /app

COPY dist/gproxy /usr/local/bin/gproxy
# Ships an empty data dir so SQLite has a writable parent on first boot
# (distroless has no shell to mkdir it).
COPY dist/data /app/data

# Bind on all interfaces inside the container; SQLite data under /app/data.
ENV GPROXY_HOST=0.0.0.0 \
    GPROXY_PORT=8787 \
    GPROXY_PERSISTENCE=db \
    GPROXY_DATA_DIR=/app/data \
    GPROXY_AUTOSTART=off

EXPOSE 8787

ENTRYPOINT ["/usr/local/bin/gproxy"]
