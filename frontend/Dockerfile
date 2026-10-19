# syntax=docker/dockerfile:1.7
#
# iapolitique frontend: Vite build, then static files served by nginx.
# Build context: the repository root (devops/docker-compose.swarm.yml, `context: ..`), e.g.
#   docker build -f frontend/Dockerfile --target runtime -t iapolitique-frontend .
# frontend/Dockerfile.dockerignore keeps the context down to the frontend sources.
#
# No ARG/ENV carries a secret: the frontend has none (AGENTS.md §10.3).

# Base images are pinned by digest (AGENTS.md §14.7): a tag can be moved, a digest cannot. The
# digest is that of the multi-architecture index. To upgrade, change the version and take the new
# digest from `docker buildx imagetools inspect <image>:<tag>` (first "Digest:" line), in every
# file that uses the image (frontend/Dockerfile, devops/Dockerfile.test).
ARG NODE_VERSION=22
ARG NODE_DIGEST=sha256:0a7108bf6c7bf5de370ffb1a3ed6be93d405b43ff159f681a8d18c0e2bc2e402
ARG NGINX_VERSION=1.31
ARG NGINX_DIGEST=sha256:892bb2152464f0c5d8972593a5f5afaf8054552580b699d95f8ff4af08c2d33d
# Alpine release of that nginx image (/etc/alpine-release): selects its security feed below.
ARG NGINX_ALPINE_RELEASE=3.24

# Alpine security feed (finding SUP-2). BuildKit fetches an ADD URL at every build and re-runs
# the `apk upgrade` that binds it whenever Alpine publishes a fix, so the shipped image takes the
# fixed packages without waiting for a new nginx digest. Leaves no file in the image.
FROM scratch AS alpine-secdb
ARG NGINX_ALPINE_RELEASE
ADD https://secdb.alpinelinux.org/v${NGINX_ALPINE_RELEASE}/main.json /main.json

# -----------------------------------------------------------------------------------------------
# build: install exact dependencies (package-lock.json) and produce dist/.
# -----------------------------------------------------------------------------------------------
FROM node:${NODE_VERSION}-alpine@${NODE_DIGEST} AS build
WORKDIR /app
ENV CI=true
COPY frontend/package.json frontend/package-lock.json frontend/.npmrc ./
RUN --mount=type=cache,id=iapolitique-npm,target=/root/.npm,sharing=locked \
    npm ci --no-audit --no-fund
COPY frontend/ ./
# The shipped build names no third-party origin (scripts/check-dist-urls.mjs, AGENTS.md §14.2).
RUN npm run build && npm run check:dist

# -----------------------------------------------------------------------------------------------
# runtime: nginx on port 80 (AGENTS.md §13.1). The master process binds the port, workers run
# as the unprivileged "nginx" user.
# -----------------------------------------------------------------------------------------------
FROM nginx:${NGINX_VERSION}-alpine@${NGINX_DIGEST} AS runtime
ARG NGINX_ALPINE_RELEASE
# Fixed Alpine packages (the pinned digest freezes them otherwise), then our own complete
# configuration instead of the default site. Same steps as devops/Dockerfile.test.
RUN --mount=type=bind,from=alpine-secdb,target=/tmp/secdb \
    grep -q "^${NGINX_ALPINE_RELEASE}\." /etc/alpine-release \
    && apk upgrade --no-cache \
    && rm -f /etc/nginx/conf.d/default.conf
COPY frontend/nginx.conf /etc/nginx/nginx.conf
# /.well-known/security.txt (RFC 9116), rendered at container start by the image's envsubst
# templates mechanism from LEGAL_CONTACT_EMAIL and TRAEFIK_HOST (16-security-txt.envsh). The
# output goes to /run, the only writable place besides the nginx cache when the root file
# system is read-only (devops/docker-compose.swarm.yml).
ENV NGINX_ENVSUBST_OUTPUT_DIR=/run/nginx-generated
COPY frontend/nginx/.well-known/security.txt.template /etc/nginx/templates/.well-known/
COPY --chmod=0755 frontend/nginx/16-security-txt.envsh /docker-entrypoint.d/
COPY --from=build /app/dist /usr/share/nginx/html
RUN nginx -t
EXPOSE 80
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD wget -q -O /dev/null http://127.0.0.1/health || exit 1
