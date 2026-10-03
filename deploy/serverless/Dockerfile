FROM public.ecr.aws/docker/library/node:24-alpine AS prepare

WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --omit=dev
COPY prepare-release.mjs ./
# The normal CLI image only includes SQLite. This release binary includes
# PostgreSQL and the console, with checksums verified by prepare-release.mjs.
ARG GPROXY_RELEASE_VERSION=v4.0.3
RUN GPROXY_RELEASE_VERSION="$GPROXY_RELEASE_VERSION" node prepare-release.mjs

FROM public.ecr.aws/docker/library/node:24-alpine
WORKDIR /app
COPY --from=prepare /app/.gproxy ./.gproxy
COPY lib/ ./lib/
COPY container.mjs ./

USER node
ENV PORT=8787
EXPOSE 8787
CMD ["node", "container.mjs"]
