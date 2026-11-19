FROM docker.io/library/node:24-alpine AS prepare

WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --omit=dev
COPY prepare-release.mjs ./
# The serverless release binary keeps its cache in PostgreSQL; its checksums
# are verified by prepare-release.mjs. The Render and Heroku templates use the
# ordinary release image instead.
ARG GPROXY_RELEASE_VERSION=latest
RUN GPROXY_RELEASE_VERSION="$GPROXY_RELEASE_VERSION" node prepare-release.mjs

FROM docker.io/library/node:24-alpine
WORKDIR /app
COPY --from=prepare /app/.gproxy ./.gproxy
COPY lib/ ./lib/
COPY container.mjs ./

USER node
ENV PORT=8787
EXPOSE 8787
CMD ["node", "container.mjs"]
