#!/bin/sh
# Checks the HTTP contract of the frontend nginx (frontend/nginx.conf, AGENTS.md §14.2) against a
# running instance: security headers, CSP nonce, caching, private pages, methods, dotfiles,
# security.txt. POSIX sh + curl: runs in the nginx:alpine based CI stage (devops/Dockerfile.test)
# and from Git Bash against the e2e container (devops/dev/e2e.sh).
#
# Usage: check_frontend_headers.sh BASE_URL [CONTACT_EMAIL]
#   CONTACT_EMAIL given: /.well-known/security.txt must exist and name it;
#   absent or empty:     /.well-known/security.txt must answer 404.
set -eu

base="${1:?usage: check_frontend_headers.sh BASE_URL [CONTACT_EMAIL]}"
email="${2:-}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
failures=0

fail() {
    echo "FAIL  $*" >&2
    failures=$((failures + 1))
}

# fetch METHOD PATH: headers in $work/h (lower-cased names), body in $work/b, status in $status.
fetch() {
    if [ "$1" = HEAD ]; then
        status="$(curl -sS --path-as-is -o "$work/b" -D "$work/raw" -w '%{http_code}' -I "$base$2")"
    else
        status="$(curl -sS --path-as-is -o "$work/b" -D "$work/raw" -w '%{http_code}' -X "$1" "$base$2")"
    fi
    tr -d '\r' <"$work/raw" | awk -F': ' 'NR > 1 && NF > 1 { name = tolower($1); sub(/^[^:]*: /, ""); print name ": " $0 }' >"$work/h"
}

# header NAME: value of the response header (empty if absent).
header() {
    awk -v name="$1" 'index($0, name ": ") == 1 { print substr($0, length(name) + 3); exit }' "$work/h"
}

# expect_header CONTEXT NAME VALUE ("" = must be absent)
expect_header() {
    actual="$(header "$2")"
    if [ "$actual" != "$3" ]; then
        fail "$1: $2 is '$actual', expected '${3:-<absent>}'"
    fi
}

expect_status() {
    if [ "$status" != "$2" ]; then
        fail "$1: status $status, expected $2"
    fi
}

csp_template="default-src 'none'; script-src 'self' https://stats.methodinfo.fr; style-src 'self' 'nonce-NONCE' 'sha256-StEaX+se6YS7pqjzrzMIA0KaX9zF/8zAhvQXZAe5epY=' 'sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=' 'sha256-441zG27rExd4/il+NvIqyL8zFx5XmyNQtE381kSkUJk='; style-src-attr 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self' https://stats.methodinfo.fr; worker-src 'self'; manifest-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'; object-src 'none'; upgrade-insecure-requests"

# Headers every response carries, whatever its status.
common_headers() {
    nonce="$(header content-security-policy | sed -n "s/.*'nonce-\([0-9a-f]\{32\}\)'.*/\1/p")"
    if [ -z "$nonce" ]; then
        fail "$1: no 128-bit nonce in the CSP"
    fi
    expect_header "$1" content-security-policy "$(echo "$csp_template" | sed "s/NONCE/$nonce/")"
    expect_header "$1" strict-transport-security "max-age=63072000; includeSubDomains"
    expect_header "$1" x-content-type-options nosniff
    expect_header "$1" x-frame-options DENY
    expect_header "$1" referrer-policy no-referrer
    expect_header "$1" cross-origin-opener-policy same-origin
    expect_header "$1" cross-origin-resource-policy same-origin
    expect_header "$1" cross-origin-embedder-policy require-corp
    expect_header "$1" x-powered-by ""
    for feature in camera microphone geolocation payment usb browsing-topics; do
        case "$(header permissions-policy)" in
            *"$feature=()"*) ;;
            *) fail "$1: Permissions-Policy does not disable $feature" ;;
        esac
    done
    case "$(header server)" in
        "" | nginx) ;;
        *) fail "$1: Server header discloses '$(header server)'" ;;
    esac
}

# --- The SPA shell -------------------------------------------------------------------------
fetch GET /
expect_status "GET /" 200
common_headers "GET /"
expect_header "GET /" cache-control no-cache
expect_header "GET /" x-robots-tag ""
expect_header "GET /" allow ""
expect_header "GET /" etag ""
if ! grep -q "<script nonce=\"$nonce\" src=\"/theme-init.js\"></script>" "$work/b"; then
    fail "GET /: theme-init.js does not carry this response's CSP nonce"
fi
first_nonce="$nonce"
asset="$(sed -n 's/.*src="\(\/assets\/[^"]*\.js\)".*/\1/p' "$work/b" | head -n 1)"

fetch GET /
common_headers "GET / (again)"
if [ "$nonce" = "$first_nonce" ]; then
    fail "GET /: the CSP nonce is reused across responses"
fi

for path in /idees/une-idee /proposer /methodologie /page-inconnue; do
    fetch GET "$path"
    expect_status "GET $path" 200
    common_headers "GET $path"
    expect_header "GET $path" cache-control no-cache
    expect_header "GET $path" x-robots-tag ""
done

# --- Private pages: never stored, never indexed --------------------------------------------
for path in /suivi /suivi/ /suivi?x=1 /SUIVI /suivi/ancien-jeton /admin /admin/idees /Admin /%73uivi; do
    fetch GET "$path"
    expect_status "GET $path" 200
    common_headers "GET $path"
    expect_header "GET $path" cache-control no-store
    expect_header "GET $path" x-robots-tag "noindex, nofollow"
done
for path in /suivie /administration /idees/suivi-des-depenses; do
    fetch GET "$path"
    expect_header "GET $path" x-robots-tag ""
done

# --- Static files ------------------------------------------------------------------------------
if [ -z "$asset" ]; then
    fail "GET /: no /assets/*.js script found in index.html"
else
    fetch GET "$asset"
    expect_status "GET $asset" 200
    common_headers "GET $asset"
    expect_header "GET $asset" cache-control "public, max-age=31536000, immutable"
    fetch HEAD "$asset"
    expect_status "HEAD $asset" 200
fi
fetch GET /assets/absent.js
expect_status "GET /assets/absent.js" 404
expect_header "GET /assets/absent.js" cache-control ""
fetch GET /theme-init.js
expect_status "GET /theme-init.js" 200
expect_header "GET /theme-init.js" cache-control no-cache

fetch GET /health
expect_status "GET /health" 200

fetch GET /api/ideas
expect_status "GET /api/ideas" 404
common_headers "GET /api/ideas"
case "$(header content-type)" in
    application/json*) ;;
    *) fail "GET /api/ideas: not a JSON 404 ($(header content-type))" ;;
esac

# --- Methods: GET and HEAD only ----------------------------------------------------------------
fetch HEAD /
expect_status "HEAD /" 200
for method in POST PUT PATCH DELETE OPTIONS; do
    for path in / /suivi /assets/absent.js /api/ideas; do
        fetch "$method" "$path"
        expect_status "$method $path" 405
        expect_header "$method $path" allow "GET, HEAD"
        common_headers "$method $path"
    done
done

# --- Dotfiles ----------------------------------------------------------------------------------
for path in /.env /.git/config /.htaccess /assets/.env /%2eenv /.well-known/ /.well-known/change-password /.well-known/security.txt.bak; do
    fetch GET "$path"
    expect_status "GET $path" 404
done

# --- security.txt (RFC 9116) -------------------------------------------------------------------
fetch GET /.well-known/security.txt
if [ -n "$email" ]; then
    expect_status "GET /.well-known/security.txt" 200
    common_headers "GET /.well-known/security.txt"
    expect_header "GET /.well-known/security.txt" content-type "text/plain; charset=utf-8"
    grep -qx "Contact: mailto:$email" "$work/b" || fail "security.txt: no 'Contact: mailto:$email'"
    grep -qx "Preferred-Languages: fr" "$work/b" || fail "security.txt: no Preferred-Languages"
    grep -qx 'Canonical: https://[A-Za-z0-9.-]*/.well-known/security.txt' "$work/b" ||
        fail "security.txt: no Canonical URL"
    expires="$(sed -n 's/^Expires: \([0-9]\{4\}-[0-9]\{2\}-[0-9]\{2\}T[0-9:]\{8\}Z\)$/\1/p' "$work/b")"
    if [ -z "$expires" ]; then
        fail "security.txt: no Expires date"
    elif [ "$expires" \< "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" ]; then
        fail "security.txt: expired ($expires)"
    fi
    if grep -q '\${' "$work/b"; then
        fail "security.txt: unsubstituted variable"
    fi
else
    expect_status "GET /.well-known/security.txt (no contact configured)" 404
fi

if [ "$failures" -gt 0 ]; then
    echo "$failures frontend header check(s) failed" >&2
    exit 1
fi
echo "ok    frontend headers, caching, methods, dotfiles and security.txt ($base)"
