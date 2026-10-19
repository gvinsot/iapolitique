#!/bin/bash
# OWASP ZAP scans of the local stack (AGENTS.md §14.8), LOCAL ONLY: never against QA or production
# (the active scans send thousands of attack requests).
#
# The ZAP container shares the network namespace of the local Traefik proxy (e2e-sec-proxy,
# started by devops/dev/security/run.sh), so https://localhost is the public host exactly as a
# browser sees it. Three scans:
#   1. zap-baseline.py on the frontend (spider + AJAX spider, passive rules);
#   2. zap-api-scan.py with the committed spec (frontend/src/api/openapi.json, admin routes
#      included: on the public host they must all answer 404), passive + active API policy;
#   3. targeted active scan: zap-api-scan.py on the public routes only, every parameter seeded
#      with real values (zap_spec.py: the hostile idea's slug, valid filters), alpha rules on.
# Alerts are triaged by zap-rules.tsv (each IGNORE/INFO carries its justification); any alert of
# a rule not listed there fails the run. zap_summary.py writes zap-summary.md.
#
# Usage: devops/dev/security/zap.sh WORK_DIR   (WORK_DIR holds seed.json; reports land there)
set -euo pipefail
export MSYS_NO_PATHCONV=1

native_dir() { (cd "$1" && (pwd -W 2>/dev/null || pwd)); }
here="$(native_dir "$(dirname "$0")")"
repo="$(native_dir "$here/../../..")"
work="$(native_dir "${1:?usage: zap.sh WORK_DIR}")"
zap_image="${SEC_ZAP_IMAGE:-ghcr.io/zaproxy/zaproxy:stable}"
python_image="${SEC_PYTHON_IMAGE:-python:3.13-alpine}"
proxy="${SEC_PROXY:-e2e-sec-proxy}"

mkdir -p "$work/zap"
cp "$repo/frontend/src/api/openapi.json" "$work/zap/openapi.json"
cp "$here/zap-rules.tsv" "$work/zap/zap-rules.tsv"
docker run --rm -v "$work:/work" -v "$here:/security:ro" "$python_image" \
    python -I /security/zap_spec.py /work/zap/openapi.json /work/seed.json \
    /work/zap/openapi-public-targeted.json

zap() {
    # -I: warnings do not change the exit code; zap_summary.py applies the triage instead.
    docker run --rm --network "container:$proxy" -v "$work/zap:/zap/wrk:rw" "$zap_image" "$@" -I \
        || { echo "ZAP exited with $? ($1)" >&2; return 1; }
}

echo "== ZAP baseline (frontend)"
zap zap-baseline.py -t https://localhost/ -j -m 3 -c zap-rules.tsv \
    -J baseline.json -r baseline.html -w baseline.md >"$work/zap/baseline.log" 2>&1
tail -3 "$work/zap/baseline.log"

echo "== ZAP API scan (committed spec, public host)"
zap zap-api-scan.py -t /zap/wrk/openapi.json -f openapi -O https://localhost -c zap-rules.tsv \
    -J api.json -r api.html -w api.md >"$work/zap/api.log" 2>&1
tail -3 "$work/zap/api.log"

echo "== ZAP targeted active scan (public routes, seeded parameters, alpha rules)"
zap zap-api-scan.py -t /zap/wrk/openapi-public-targeted.json -f openapi -O https://localhost -a \
    -c zap-rules.tsv -J targeted.json -r targeted.html -w targeted.md \
    >"$work/zap/targeted.log" 2>&1
tail -3 "$work/zap/targeted.log"

docker run --rm -v "$work:/work" -v "$here:/security:ro" "$python_image" \
    python -I /security/zap_summary.py /security/zap-rules.tsv /work/zap/zap-summary.md \
    /work/zap/baseline.json /work/zap/api.json /work/zap/targeted.json
