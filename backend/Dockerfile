# syntax=docker/dockerfile:1.7
#
# Backend image of iapolitique (AGENTS.md §10.1): one binary for the api, the worker and the
# migrations (`command: ["api"]`, `["worker"]`, `["migrate"]`).
#
# Build context: the repository root (`context: ..` in devops/docker-compose.swarm.yml), so
# that prompts/ can be embedded; backend/Dockerfile.dockerignore keeps everything else out.
# The CI checks (fmt, clippy, tests) live in devops/Dockerfile.test.

# Base images are pinned by version and by the digest of their multi-architecture index
# (AGENTS.md §14.7, finding IMG-1): a tag can be moved, a digest cannot. To upgrade, change the
# version and take the new digest from `docker buildx imagetools inspect <image>:<tag>` (first
# "Digest:" line), in every file that uses the image (RUST_*: devops/Dockerfile.test too);
# devops/check_image_pins.py fails the CI otherwise.
#
# Keep RUST_VERSION aligned with devops/Dockerfile.test. The runtime is distroless/cc on the same
# Debian release as the rust image (trixie = Debian 13), so the binary finds the glibc it was
# linked against.
ARG RUST_VERSION=1.99
ARG RUST_DIGEST=sha256:e259494b14672950a909b4c54527e19d29cae6ebf980672114c8bb85e5594e4a
ARG DEBIAN_RELEASE=trixie
# gcr.io/distroless/cc-debian13:latest: glibc, libgcc, CA certificates, tzdata; no shell, no
# package manager, no curl/wget (the binary has its own `healthcheck` command).
ARG DISTROLESS_CC_DIGEST=sha256:159783207c2cd44c2aa5715961d13c8612368ac9bd450f887e3f08fc8ea461e3

# ---------------------------------------------------------------------------------------------
# build: release binary at /out/iapolitique
# ---------------------------------------------------------------------------------------------
FROM rust:${RUST_VERSION}-${DEBIAN_RELEASE}@${RUST_DIGEST} AS build
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
# runtime: distroless (no shell, no package manager), non-root (uid 10001, as before: the
# distroless image has no entry for it in /etc/passwd, which the binary does not need)
# ---------------------------------------------------------------------------------------------
FROM gcr.io/distroless/cc-debian13@${DISTROLESS_CC_DIGEST} AS runtime
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
