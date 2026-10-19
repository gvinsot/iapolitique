#!/bin/bash
# Vulnerability scan of the runtime images with Trivy (AGENTS.md §14.7, finding SUP-2), LOCAL
# ONLY, before each production release (docs/securite.md §7.2).
#
# Scans the api image (also used by the worker) and the frontend image, as built from devops/:
#   docker compose -f docker-compose.swarm.yml --env-file <copy of .env.example> build api frontend
# Each image is exported with `docker save` and scanned offline from the archive (no Docker
# socket mounted in the scanner). Only HIGH and CRITICAL vulnerabilities that have a fix are
# reported (--ignore-unfixed): those are the ones a base image update or a dependency bump can
# remove. Any finding fails the run, unless it is listed in trivy-ignore.yaml with its reason and
# an expiry date.
#
# Usage: devops/dev/security/trivy.sh [WORK_DIR]   (reports land in WORK_DIR/trivy)
# Variables: SEC_API_IMAGE, SEC_FRONT_IMAGE (images to scan), SEC_TRIVY_IMAGE (scanner),
# SEC_TRIVY_CACHE (Docker volume keeping the vulnerability database between runs).
set -euo pipefail
export MSYS_NO_PATHCONV=1

native_dir() { (cd "$1" && (pwd -W 2>/dev/null || pwd)); }
here="$(native_dir "$(dirname "$0")")"
work_dir="${1:-${SEC_WORK:-$(mktemp -d)}}"
mkdir -p "$work_dir"
work="$(native_dir "$work_dir")"
# Pinned by version and digest of the multi-architecture index (AGENTS.md §14.7). Update: pick a
# release at least two weeks old, then `docker buildx imagetools inspect aquasec/trivy:<version>`.
trivy_image="${SEC_TRIVY_IMAGE:-aquasec/trivy:0.75.0@sha256:af6acf9a6b85dfe389a1941505c0ce9efef52a4719635e1a962f022a3d855daa}"
cache="${SEC_TRIVY_CACHE:-iapolitique_trivy_cache}"
api_image="${SEC_API_IMAGE:-registry.methodinfo.fr/iapolitique-api:latest}"
front_image="${SEC_FRONT_IMAGE:-registry.methodinfo.fr/iapolitique-frontend:latest}"

out="$work/trivy"
mkdir -p "$out"
trap 'rm -f "${out:?}"/*.tar' EXIT

trivy() {
    docker run --rm -v "$cache:/root/.cache/trivy" -v "$out:/work" -v "$here:/security:ro" \
        "$trivy_image" "$@"
}

echo "== Trivy $(trivy --version 2>/dev/null | sed -n 's/^Version: //p'), vulnerability database"
trivy image --download-db-only --no-progress >/dev/null

status=0
for entry in "api:$api_image" "frontend:$front_image"; do
    name="${entry%%:*}"
    image="${entry#*:}"
    echo "== $name ($image, $(docker image inspect -f '{{.Id}}' "$image" | cut -c8-19))"
    docker save -o "$out/$name.tar" "$image"
    # JSON first (kept as the report), then the same findings as a table for the log.
    if ! trivy image --input "/work/$name.tar" --scanners vuln --severity HIGH,CRITICAL \
        --ignore-unfixed --ignorefile /security/trivy-ignore.yaml --skip-db-update --no-progress \
        --format json --output "/work/$name.json" --exit-code 1 --quiet; then
        status=1
    fi
    trivy convert --format table --scanners vuln --output "/work/$name.txt" "/work/$name.json"
    cat "$out/$name.txt"
    rm -f "${out:?}/$name.tar"
done

echo "Reports: $out (api.json, api.txt, frontend.json, frontend.txt)"
if [[ "$status" != 0 ]]; then
    echo "FAIL  fixable HIGH or CRITICAL vulnerabilities in the runtime images" >&2
    exit 1
fi
echo "ok    no fixable HIGH or CRITICAL vulnerability in the runtime images"
