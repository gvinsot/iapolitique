# syntax=docker/dockerfile:1.7
#
# Backend image of iapolitique (AGENTS.md §10.1): one binary for the api, the worker and the
# migrations (`command: ["api"]`, `["worker"]`, `["migrate"]`).
#
# Build context: the repository root (`context: ..` in devops/docker-compose.swarm.yml), so
# that prompts/ can be embedded; backend/Dockerfile.dockerignore keeps everything else out.
# The CI checks (fmt, clippy, tests) live in devops/Dockerfile.test.

# Keep RUST_VERSION aligned with devops/Dockerfile.test. The runtime uses the same Debian
# release as the rust image, so the binary finds the glibc it was linked against.
ARG RUST_VERSION=1.99
ARG DEBIAN_RELEASE=trixie

# ---------------------------------------------------------------------------------------------
# build: release binary at /out/iapolitique
# ---------------------------------------------------------------------------------------------
FROM rust:${RUST_VERSION}-${DEBIAN_RELEASE} AS build
ENV CARGO_TERM_COLOR=always \
    CARGO_INCREMENTAL=0
WORKDIR /src/backend

# 1. Dependencies only: this layer is reused as long as Cargo.toml and Cargo.lock are unchanged.
COPY backend/Cargo.toml backend/Cargo.lock backend/build.rs ./
RUN --mount=type=cache,id=iapolitique-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    mkdir -p src migrations \
    && echo 'fn main() {}' > src/main.rs \
    && touch src/lib.rs \
    && cargo build --release --locked \
    && rm -rf src

# 2. The crate itself (migrations are embedded by sqlx::migrate!).
COPY backend/ ./
RUN --mount=type=cache,id=iapolitique-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    touch src/main.rs src/lib.rs build.rs \
    && cargo build --release --locked \
    && mkdir -p /out \
    && cp target/release/iapolitique /out/iapolitique \
    && /out/iapolitique --version

# ---------------------------------------------------------------------------------------------
# runtime: non-root, no shell tools needed (the binary has its own `healthcheck` command)
# ---------------------------------------------------------------------------------------------
FROM debian:${DEBIAN_RELEASE}-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 iapolitique \
    && useradd --system --uid 10001 --gid iapolitique --home-dir /app --no-create-home \
        --shell /usr/sbin/nologin iapolitique
WORKDIR /app
COPY --from=build /out/iapolitique /app/iapolitique
# Versioned prompts, published on the methodology page (§13.4).
COPY prompts/ /app/prompts/
ENV PROMPTS_DIR=/app/prompts \
    LOG_FORMAT=json \
    RUST_LOG=info,sqlx=warn
USER 10001:10001
EXPOSE 8080 8081 8082
HEALTHCHECK --interval=15s --timeout=5s --start-period=30s --retries=3 \
    CMD ["/app/iapolitique", "healthcheck", "--url", "http://127.0.0.1:8080/api/health"]
ENTRYPOINT ["/app/iapolitique"]
CMD ["api"]
