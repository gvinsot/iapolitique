#!/bin/bash
# Local end-to-end run of the runtime images (never on the swarm, never against a real Mistral):
#   - api and worker containers from registry.methodinfo.fr/iapolitique-api:latest,
#   - the frontend container (nginx) from registry.methodinfo.fr/iapolitique-frontend:latest,
#   - a fresh database on a PostgreSQL + pgvector server (by default the local test server of
#     AGENTS.md §13.5, reached from the containers through host.docker.internal),
#   - the real decision service (E2E_DECISION_SERVICE_URL),
#   - devops/dev/mock_mistral.py in a python container, with a dummy MISTRAL_API_KEY.
# The containers run with the hardening of devops/docker-compose.swarm.yml (read-only root file
# system, tmpfs, capabilities, init). devops/check_frontend_headers.sh checks the nginx headers,
# then devops/dev/e2e_scenario.py drives the pipeline, devops/dev/browser_check.sh browses every
# route in Chromium and Firefox (CSP violations, ALTCHA, admin; E2E_BROWSER=0 skips it), a second
# phase restarts the api and the worker with a tiny daily budget, and the container logs are
# checked for client IPs, tracking tokens and visitor texts (AGENTS.md §7.1).
#
# Build the images first (from devops/, with a copy of .env.example):
#   docker compose -f docker-compose.swarm.yml --env-file <copy> build api frontend
# Usage: devops/dev/e2e.sh          (containers are removed at the end; E2E_KEEP=1 keeps them)
set -euo pipefail
# Git Bash: keep container paths (/app/...) as they are; host paths are given in native form.
export MSYS_NO_PATHCONV=1

native_dir() { (cd "$1" && (pwd -W 2>/dev/null || pwd)); }
here="$(native_dir "$(dirname "$0")")"
pg_url="${E2E_PG_URL:-postgres://postgres:postgres@host.docker.internal:55432}"
db="${E2E_DB:-iapolitique_e2e}"
decision_url="${E2E_DECISION_SERVICE_URL:-http://192.168.1.27:8001/v1/systemone}"
api_image="${E2E_API_IMAGE:-registry.methodinfo.fr/iapolitique-api:latest}"
front_image="${E2E_FRONT_IMAGE:-registry.methodinfo.fr/iapolitique-frontend:latest}"
public_port="${E2E_PUBLIC_PORT:-18080}"
admin_port="${E2E_ADMIN_PORT:-18081}"
front_port="${E2E_FRONT_PORT:-18000}"
net=iapolitique-e2e
# The first interpreter that really runs (on Windows, python3 may be a Store alias stub).
python=""
for candidate in python3 python; do
    if "$candidate" -c "" >/dev/null 2>&1; then python="$candidate"; break; fi
done
[[ -n "$python" ]] || { echo "Python 3 is required" >&2; exit 1; }
work="$(mktemp -d)"
secrets_file="$(native_dir "$work")/secrets.txt"

case "$db" in
    *prod* | *qa*) echo "refusing to use database $db" >&2; exit 1 ;;
esac

token="$("$python" -c 'import secrets; print(secrets.token_hex(32))')"
altcha_key="$("$python" -c 'import secrets; print(secrets.token_hex(32))')"

cleanup() {
    if [[ "${E2E_KEEP:-0}" != 1 ]]; then
        docker rm -f e2e-api e2e-worker e2e-front e2e-mock-mistral >/dev/null 2>&1 || true
        docker network rm "$net" >/dev/null 2>&1 || true
    fi
    rm -rf "$work"
}
trap cleanup EXIT

psql() {
    docker run --rm --add-host=host.docker.internal:host-gateway pgvector/pgvector:pg18 \
        psql -v ON_ERROR_STOP=1 -qAt "$@"
}

echo "== fresh database $db"
docker rm -f e2e-api e2e-worker e2e-front e2e-mock-mistral >/dev/null 2>&1 || true
psql "$pg_url/postgres" -c "DROP DATABASE IF EXISTS $db WITH (FORCE)" -c "CREATE DATABASE $db"
docker network create "$net" >/dev/null 2>&1 || true

echo "== mock Mistral"
docker create --name e2e-mock-mistral --network "$net" python:3.13-alpine \
    python /mock_mistral.py --port 8090 >/dev/null
docker cp "$here/mock_mistral.py" e2e-mock-mistral:/mock_mistral.py >/dev/null
docker start e2e-mock-mistral >/dev/null

# Same hardening as devops/docker-compose.swarm.yml (AGENTS.md §14.7).
# Resource limits as well (AGENTS.md §14.5): the stack must stay healthy under them.
backend_hardening=(--read-only --tmpfs /tmp:size=16m --cap-drop ALL --init
    --cpus 2 --memory 512m --pids-limit 1024)
front_hardening=(--read-only --tmpfs /run:size=1m --tmpfs /var/cache/nginx:size=16m --cap-drop ALL
    --cap-add CHOWN --cap-add SETGID --cap-add SETUID --cap-add NET_BIND_SERVICE --init
    --cpus 0.5 --memory 128m --pids-limit 256)
contact_email=securite@example.org

backend_env=(
    -e "DATABASE_CONNECTION_STRING=$pg_url/$db?application_name=iapolitique-e2e"
    -e "MISTRAL_API_KEY=e2e-dummy-key-not-a-real-one"
    -e "MISTRAL_API_BASE_URL=http://e2e-mock-mistral:8090"
    -e "DECISION_SERVICE_URL=$decision_url"
    -e "DECISION_SERVICE_TIMEOUT_SECS=${E2E_DECISION_TIMEOUT_SECS:-5}"
    -e "DECISION_SERVICE_JOB_TIMEOUT_SECS=${E2E_DECISION_JOB_TIMEOUT_SECS:-30}"
    -e "TRAEFIK_HOST=localhost"
    -e "ADMIN_HOST=admin.localhost"
    -e "WORKER_POLL_INTERVAL_MS=500"
)

start_backend() {
    local budget="$1"
    docker rm -f e2e-api e2e-worker >/dev/null 2>&1 || true
    docker run -d --name e2e-api --network "$net" --add-host=host.docker.internal:host-gateway \
        "${backend_hardening[@]}" -p "127.0.0.1:$public_port:8080" -p "127.0.0.1:$admin_port:8081" \
        "${backend_env[@]}" -e "MISTRAL_DAILY_BUDGET_EUR=$budget" \
        -e "ADMIN_API_TOKEN=$token" -e "ALTCHA_HMAC_KEY=$altcha_key" -e "TRUSTED_PROXY_HOPS=1" \
        -e "LEGAL_PUBLISHER_NAME=Éditeur de test" -e "LEGAL_CONTACT_EMAIL=contact@example.org" \
        --health-interval 2s --health-start-period 120s \
        "$api_image" api >/dev/null
    # Same probe as the compose file: the image's default HEALTHCHECK targets the api port.
    docker run -d --name e2e-worker --network "$net" --add-host=host.docker.internal:host-gateway \
        "${backend_hardening[@]}" "${backend_env[@]}" -e "MISTRAL_DAILY_BUDGET_EUR=$budget" \
        --health-cmd "/app/iapolitique healthcheck --url http://127.0.0.1:8082/health" \
        --health-interval 2s --health-start-period 120s \
        "$api_image" worker >/dev/null
    for _ in $(seq 1 90); do
        if [[ "$(docker inspect -f '{{.State.Health.Status}}' e2e-api)" == healthy &&
              "$(docker inspect -f '{{.State.Health.Status}}' e2e-worker)" == healthy ]]; then
            return 0
        fi
        sleep 2
    done
    docker logs e2e-api | tail -20 >&2
    echo "api or worker not healthy" >&2
    exit 1
}

echo "== api, worker and frontend"
start_backend 5
docker run -d --name e2e-front --network "$net" -p "127.0.0.1:$front_port:80" "${front_hardening[@]}" \
    -e "LEGAL_CONTACT_EMAIL=$contact_email" -e "TRAEFIK_HOST=localhost" "$front_image" >/dev/null

echo "== frontend"
for _ in $(seq 1 30); do
    curl -fsS "http://127.0.0.1:$front_port/health" >/dev/null 2>&1 && break
    sleep 1
done
# Its temporary files are host paths: Git Bash must convert them.
env -u MSYS_NO_PATHCONV "$here/../check_frontend_headers.sh" "http://127.0.0.1:$front_port" "$contact_email"
page="$(curl -fsS "http://127.0.0.1:$front_port/suivi")"
grep -q '<div id="root">' <<<"$page" || { echo "frontend: SPA fallback missing" >&2; exit 1; }
curl -fsS -H "X-Forwarded-For: 203.0.113.250" "http://127.0.0.1:$front_port/idees/x" >/dev/null
echo 203.0.113.250 >>"$secrets_file"
echo "ok    frontend serves the SPA"

echo "== scenario: main"
"$python" "$here/e2e_scenario.py" --public "http://127.0.0.1:$public_port" \
    --admin "http://127.0.0.1:$admin_port" --admin-token "$token" --secrets-out "$secrets_file"

if [[ "${E2E_BROWSER:-1}" != 0 ]]; then
    echo "== browser check"
    "$here/browser_check.sh" "$token"
    # The idea typed in the browsers must not reach any log either.
    echo "regrouper les achats de papier des écoles" >>"$secrets_file"
fi

echo "== scenario: budget exhausted"
docker logs e2e-api >"$work/api-1.log" 2>&1
docker logs e2e-worker >"$work/worker-1.log" 2>&1
start_backend 0.01
"$python" "$here/e2e_scenario.py" --public "http://127.0.0.1:$public_port" \
    --admin "http://127.0.0.1:$admin_port" --admin-token "$token" --phase budget \
    --secrets-out "$secrets_file"

echo "== logs"
docker logs e2e-api >"$work/api-2.log" 2>&1
docker logs e2e-worker >"$work/worker-2.log" 2>&1
docker logs e2e-front >"$work/front.log" 2>&1
echo "$token" >>"$secrets_file"
echo "$altcha_key" >>"$secrets_file"
sed -i '/^$/d' "$secrets_file"
if grep -F -f "$secrets_file" "$work"/*.log; then
    echo "FAIL  the logs above contain an IP, a token, a secret or a visitor text" >&2
    exit 1
fi
if grep -h '"level":"ERROR"' "$work"/api-*.log "$work"/worker-*.log; then
    echo "FAIL  errors logged" >&2
    exit 1
fi
echo "ok    no IP, token, secret or visitor text in $(cat "$work"/*.log | wc -l) log lines"
echo "E2E passed"
