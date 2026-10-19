#!/bin/bash
# Security campaign on the LOCAL stack (AGENTS.md §14.8, docs/securite.md §7). Never against QA
# or production: it sends attack payloads, exhausts rate limits and runs active scans.
#
#   1. devops/dev/e2e.sh (E2E_STACK_ONLY=1): runtime images of api, worker and frontend, fresh
#      database, real decision model, mock Mistral (devops/dev/mock_mistral.py, hostile mode on
#      the marker `sondexss`);
#   2. a Traefik container routing like production (devops/dev/traefik-dynamic.yml);
#   3. seed.py: published ideas, payloads through the whole pipeline, CSV formula round trip;
#   4. xss_check.mjs: stored XSS rendered in Chromium and Firefox with CSP monitoring;
#   5. probes.py: CORS, CSRF, Origin and Host tricks, oversized bodies, admin isolation,
#      headers of every response family, connection reuse, X-Forwarded-For spoofing;
#   6. zap.sh: ZAP baseline, API scan and targeted active scan, triaged by zap-rules.tsv.
#
# Usage: devops/dev/security/run.sh
# Variables: SEC_WORK (reports directory, default: a new temporary directory, kept),
# SEC_REUSE_STACK=1 (use the running e2e containers and SEC_WORK/seed.json as they are),
# SEC_KEEP=1 (leave the stack running at the end), SEC_SKIP_ZAP=1, plus those of e2e.sh.
# Prerequisites: those of devops/dev/e2e.sh (images built, test PostgreSQL, decision model).
set -euo pipefail
export MSYS_NO_PATHCONV=1

native_dir() { (cd "$1" && (pwd -W 2>/dev/null || pwd)); }
here="$(native_dir "$(dirname "$0")")"
dev="$(native_dir "$here/..")"
mkdir -p "${SEC_WORK:=$(mktemp -d)}"
work="$(native_dir "$SEC_WORK")"
net="${E2E_NETWORK:-iapolitique-e2e}"
proxy=e2e-sec-proxy
traefik_image="${E2E_TRAEFIK_IMAGE:-traefik:v3.7.6}"
playwright_version="${E2E_PLAYWRIGHT_VERSION:-1.63.0}"
python_image="${SEC_PYTHON_IMAGE:-python:3.13-alpine}"
python=""
for candidate in python3 python; do
    if "$candidate" -c "" >/dev/null 2>&1; then python="$candidate"; break; fi
done
[[ -n "$python" ]] || { echo "Python 3 is required" >&2; exit 1; }

cleanup() {
    docker rm -f "$proxy" >/dev/null 2>&1 || true
    if [[ "${SEC_KEEP:-0}" != 1 ]]; then
        docker rm -f e2e-api e2e-worker e2e-front e2e-mock-mistral >/dev/null 2>&1 || true
        docker network rm "$net" >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT

if [[ "${SEC_REUSE_STACK:-0}" != 1 ]]; then
    echo "== local stack"
    E2E_STACK_ONLY=1 "$dev/e2e.sh"
fi
token="$(docker inspect -f '{{range .Config.Env}}{{println .}}{{end}}' e2e-api |
    sed -n 's/^ADMIN_API_TOKEN=//p')"
[[ -n "$token" ]] || { echo "no admin token (is e2e-api running?)" >&2; exit 1; }

echo "== HTTPS proxy (Traefik, production routing)"
docker rm -f "$proxy" >/dev/null 2>&1 || true
docker create --name "$proxy" --network "$net" "$traefik_image" \
    --entrypoints.websecure.address=:443 --providers.file.filename=/dynamic.yml \
    --log.level=ERROR >/dev/null
docker cp "$dev/traefik-dynamic.yml" "$proxy:/dynamic.yml" >/dev/null
docker start "$proxy" >/dev/null

if [[ "${SEC_REUSE_STACK:-0}" != 1 || ! -f "$work/seed.json" ]]; then
    echo "== seed (published ideas, payloads through the pipeline, CSV round trip)"
    "$python" "$here/seed.py" --public "http://127.0.0.1:${E2E_PUBLIC_PORT:-18080}" \
        --admin "http://127.0.0.1:${E2E_ADMIN_PORT:-18081}" --admin-token "$token" \
        --out "$work/seed.json"
fi

status=0
# Browsers first: the probes exhaust the tracking rate limit of the proxy address.
echo "== stored XSS in real browsers"
docker run --rm --network "container:$proxy" --ipc=host -e "ADMIN_TOKEN=$token" \
    -e SEED=/work/seed.json -e "BROWSERS=${E2E_BROWSERS:-chromium,firefox}" \
    -v "$work:/work:ro" -v "$here/xss_check.mjs:/check/xss_check.mjs:ro" \
    "mcr.microsoft.com/playwright:v$playwright_version-noble" \
    bash -c "mkdir -p /tmp/check && cd /tmp/check \
        && npm install --silent --no-audit --no-fund playwright@$playwright_version >/dev/null \
        && cp /check/xss_check.mjs . && node xss_check.mjs" \
    >"$work/xss.log" 2>&1 || status=1
tail -1 "$work/xss.log"
grep -E '^\[|FAIL' "$work/xss.log" || true

echo "== manual probes"
docker run --rm --network "container:$proxy" -e "ADMIN_TOKEN=$token" -e SEED=/work/seed.json \
    -e RESULTS=/work/probes.json -v "$work:/work" -v "$here:/security:ro" "$python_image" \
    python -I /security/probes.py | tee "$work/probes.log" | grep -v '^ok ' || status=1

if [[ "${SEC_SKIP_ZAP:-0}" != 1 ]]; then
    echo "== OWASP ZAP"
    SEC_PROXY="$proxy" "$here/zap.sh" "$work" >"$work/zap.log" 2>&1 || status=1
    grep -E '^(==|ok|FAIL|  )|NON TRI' "$work/zap.log" || true
fi

echo "Reports: $work (probes.json, xss.log, zap/*.html, zap/zap-summary.md)"
if [[ "$status" != 0 ]]; then
    echo "FAIL  security campaign" >&2
    exit 1
fi
echo "Security campaign passed"
