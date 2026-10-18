#!/bin/bash
# Entrypoint of the CI test image (devops/Dockerfile.test): waits for the throwaway PostgreSQL
# (service test-db) and runs the backend test suite, already compiled at image build time.
# Extra arguments go to `cargo test` (e.g. `docker compose run --rm test -- --nocapture`).
set -euo pipefail

: "${DATABASE_URL:?DATABASE_URL must point to the test database}"
host="${TEST_DB_HOST:-test-db}"
port="${TEST_DB_PORT:-5432}"
timeout_secs="${TEST_DB_WAIT_SECS:-90}"

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

cd /src/backend
exec cargo test --locked --offline "$@"
