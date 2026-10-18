# syntax=docker/dockerfile:1.7
#
# iapolitique frontend: Vite build, then static files served by nginx.
# Build context: the repository root (devops/docker-compose.swarm.yml, `context: ..`), e.g.
#   docker build -f frontend/Dockerfile --target runtime -t iapolitique-frontend .
# frontend/Dockerfile.dockerignore keeps the context down to the frontend sources.
#
# No ARG/ENV carries a secret: the frontend has none (AGENTS.md §10.3).

ARG NODE_VERSION=22
ARG NGINX_VERSION=1.31

# -----------------------------------------------------------------------------------------------
# build: install exact dependencies (package-lock.json) and produce dist/.
# -----------------------------------------------------------------------------------------------
FROM node:${NODE_VERSION}-alpine AS build
WORKDIR /app
ENV CI=true
COPY frontend/package.json frontend/package-lock.json frontend/.npmrc ./
RUN --mount=type=cache,id=iapolitique-npm,target=/root/.npm,sharing=locked \
    npm ci --no-audit --no-fund
COPY frontend/ ./
RUN npm run build

# -----------------------------------------------------------------------------------------------
# runtime: nginx on port 80 (AGENTS.md §13.1). The master process binds the port, workers run
# as the unprivileged "nginx" user.
# -----------------------------------------------------------------------------------------------
FROM nginx:${NGINX_VERSION}-alpine AS runtime
# The default site is replaced by our own complete configuration.
RUN rm -f /etc/nginx/conf.d/default.conf
COPY frontend/nginx.conf /etc/nginx/nginx.conf
COPY --from=build /app/dist /usr/share/nginx/html
RUN nginx -t
EXPOSE 80
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD wget -q -O /dev/null http://127.0.0.1/health || exit 1
