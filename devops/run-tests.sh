#!/bin/bash
# Entrypoint of the CI test image (devops/Dockerfile.test): waits for the throwaway PostgreSQL
# (service test-db) and runs the backend test executables compiled at image build time
# (/src/backend/test-bin/; the image holds neither target/ nor the cargo registry).
# Extra arguments go to every test executable (libtest options and name filters), e.g.
# `docker compose run --rm test -- --nocapture` or `docker compose run --rm test scoring`; the
# first standalone `--` is dropped, as `cargo test` did. Cargo options (`--lib`, `-p`…) do not
# apply any more.
# Every executable runs even after a failure (like `cargo test --no-fail-fast`), so that one CI
# log shows every failing test; the exit status is non-zero if any of them failed.
set -euo pipefail

: "${DATABASE_URL:?DATABASE_URL must point to the test database}"
host="${TEST_DB_HOST:-test-db}"
port="${TEST_DB_PORT:-5432}"
timeout_secs="${TEST_DB_WAIT_SECS:-90}"
bin_dir="${TEST_BIN_DIR:-/src/backend/test-bin}"

# The postgres image only listens on TCP once its initialisation is over, so an open port means
# the server accepts connections.
echo "Waiting for PostgreSQL on ${host}:${port} (up to ${timeout_secs}s)..."
deadline=$((SECONDS + timeout_secs))
until timeout 2 bash -c "exec 3<>/dev/tcp/${host}/${port}" 2>/dev/null; do
    if ((SECONDS >= deadline)); then
        echo "PostgreSQL did not answer on ${host}:${port} within ${timeout_secs}s" >&2
        exit 1
    fi
    sleep 1
done
echo "PostgreSQL is up."

args=()
dropped=0
for arg in "$@"; do
    if ((dropped == 0)) && [[ $arg == "--" ]]; then
        dropped=1
        continue
    fi
    args+=("$arg")
done

# Same working directory and CARGO_MANIFEST_DIR as under `cargo test`: our tests take the
# manifest directory at compile time (env!), but code linked into lib-iapolitique also reads it
# at run time (std::env::var).
cd /src/backend
export CARGO_MANIFEST_DIR=/src/backend
mapfile -t bins < <(find "$bin_dir" -maxdepth 1 -type f -perm -u+x | LC_ALL=C sort)
if ((${#bins[@]} == 0)); then
    echo "No test executable in ${bin_dir}" >&2
    exit 1
fi

failed=()
for bin in "${bins[@]}"; do
    name="${bin##*/}"
    echo
    echo "==> Running ${name}"
    rc=0
    "$bin" "${args[@]}" || rc=$?
    if ((rc != 0)); then
        echo "==> ${name} FAILED (exit status ${rc})" >&2
        failed+=("$name")
    fi
done

echo
if ((${#failed[@]} > 0)); then
    echo "==> ${#failed[@]} of ${#bins[@]} test executables FAILED: ${failed[*]}" >&2
    exit 1
fi
echo "==> All ${#bins[@]} test executables passed."
