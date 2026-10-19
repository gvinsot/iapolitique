#!/bin/bash
# Real-browser check of the frontend (AGENTS.md §14.2): Chromium and Firefox (Playwright image)
# browse every public and admin route through an HTTPS Traefik that routes like production
# (/api to the api, /api/admin to the admin port on the admin host, the rest to nginx), and fail
# on any CSP violation, missing header or blocked style (devops/dev/browser_check.mjs).
#
# Needs the containers of devops/dev/e2e.sh on its network (e2e-api, e2e-front): run by e2e.sh
# after its main scenario, or by hand after `E2E_KEEP=1 devops/dev/e2e.sh`.
# Usage: devops/dev/browser_check.sh [ADMIN_TOKEN]   (default: read from the e2e-api container)
set -euo pipefail
export MSYS_NO_PATHCONV=1

native_dir() { (cd "$1" && (pwd -W 2>/dev/null || pwd)); }
here="$(native_dir "$(dirname "$0")")"
net="${E2E_NETWORK:-iapolitique-e2e}"
traefik_image="${E2E_TRAEFIK_IMAGE:-traefik:v3.7.6}"
playwright_version="${E2E_PLAYWRIGHT_VERSION:-1.63.0}"
token="${1:-$(docker inspect -f '{{range .Config.Env}}{{println .}}{{end}}' e2e-api |
    sed -n 's/^ADMIN_API_TOKEN=//p')}"
[[ -n "$token" ]] || { echo "no admin token (is e2e-api running?)" >&2; exit 1; }

cleanup() {
    docker rm -f e2e-proxy >/dev/null 2>&1 || true
}
trap cleanup EXIT

# Same routing as the Traefik labels of devops/docker-compose.swarm.yml (traefik-dynamic.yml).
echo "== HTTPS proxy"
docker rm -f e2e-proxy >/dev/null 2>&1 || true
docker create --name e2e-proxy --network "$net" "$traefik_image" \
    --entrypoints.websecure.address=:443 \
    --providers.file.filename=/dynamic.yml \
    --log.level=ERROR >/dev/null
docker cp "$here/traefik-dynamic.yml" e2e-proxy:/dynamic.yml >/dev/null
docker start e2e-proxy >/dev/null

echo "== browsers"
# The Playwright container shares the proxy's network namespace: https://localhost is Traefik,
# and both browsers resolve admin.localhost to the loopback address by themselves.
docker run --rm --network "container:e2e-proxy" --ipc=host \
    -e "ADMIN_TOKEN=$token" -e "BROWSERS=${E2E_BROWSERS:-chromium,firefox}" \
    -v "$here/browser_check.mjs:/check/browser_check.mjs:ro" \
    "mcr.microsoft.com/playwright:v$playwright_version-noble" \
    bash -c "mkdir -p /tmp/check && cd /tmp/check \
        && npm install --silent --no-audit --no-fund playwright@$playwright_version >/dev/null \
        && cp /check/browser_check.mjs . && node browser_check.mjs"
