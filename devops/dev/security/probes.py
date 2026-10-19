#!/usr/bin/env python3
"""Manual security probes of the local stack (devops/dev/security/run.sh), LOCAL ONLY.

Python standard library only. Runs in a container that shares the network namespace of the
local Traefik proxy (devops/dev/traefik-dynamic.yml): https://localhost is the public host,
https://admin.localhost the admin host (Host header), and the containers of devops/dev/e2e.sh
are reachable directly (e2e-api:8080/8081, e2e-front:80) to compare with what Traefik lets
through. Never point it at QA or production: it sends hostile requests and exhausts rate limits.

Families (AGENTS.md §14): CORS preflight from a foreign origin, cross-site POST with text/plain,
form and multipart bodies, Origin spoofing and Sec-Fetch-Site, Host header and URL rewriting
tricks, oversized and malformed bodies and parameters, admin routes on the public host and port,
security headers on every response family (2xx, 3xx, 4xx, 5xx-like, exports, ALTCHA, admin,
nginx), and X-Forwarded-For spoofing against the tracking rate limit.

Each probe prints `ok`, `FAIL` or `info` (documented residual risk); exit code 1 on any FAIL.

Environment: ADMIN_TOKEN (required), SEED (path of the seed.json written by seed.py),
RESULTS (optional path: JSON list of the results).
"""

from __future__ import annotations

import http.client
import json
import os
import re
import socket
import ssl
import sys
import threading
from typing import Any

ADMIN_TOKEN = os.environ.get("ADMIN_TOKEN", "")
SEED = json.load(open(os.environ["SEED"], encoding="utf-8")) if os.environ.get("SEED") else {}
SLUG = (SEED.get("ordinary_slugs") or ["x"])[0]
PUBLIC = ("https", "localhost", 443, "localhost")
ADMIN = ("https", "localhost", 443, "admin.localhost")
API_DIRECT = ("http", "e2e-api", 8080, "localhost")
ADMIN_DIRECT = ("http", "e2e-api", 8081, "admin.localhost")
FRONT_DIRECT = ("http", "e2e-front", 80, "localhost")
TLS = ssl._create_unverified_context()  # Traefik's default self-signed certificate (local)
GOOD_ORIGIN = "https://localhost"
EVIL_ORIGIN = "https://evil.example"

API_CSP = "default-src 'none'; frame-ancestors 'none'; base-uri 'none'"
HSTS = "max-age=63072000; includeSubDomains"
# The exact policy of frontend/nginx.conf (also checked by devops/check_frontend_headers.sh),
# with a fresh 128-bit nonce on every response.
FRONT_CSP = re.compile(
    r"^default-src 'none'; script-src 'self' https://stats\.methodinfo\.fr; style-src 'self' 'nonce-[0-9a-f]{32}' "
    r"'sha256-[A-Za-z0-9+/]{43}='( 'sha256-[A-Za-z0-9+/]{43}='){2}; "
    r"style-src-attr 'unsafe-inline'; img-src 'self' data:; font-src 'self'; "
    r"connect-src 'self' https://stats\.methodinfo\.fr; worker-src 'self'; manifest-src 'self'; base-uri 'none'; "
    r"form-action 'self'; frame-ancestors 'none'; object-src 'none'; upgrade-insecure-requests$"
)
RESULTS: list[dict[str, str]] = []


def record(level: str, family: str, message: str) -> None:
    RESULTS.append({"level": level, "family": family, "message": message})
    print(f"{level:<5} {family}: {message}", flush=True)


def check(condition: bool, family: str, message: str) -> bool:
    record("ok" if condition else "FAIL", family, message)
    return condition


class Response:
    def __init__(self, status: int, headers: list[tuple[str, str]], body: bytes) -> None:
        self.status = status
        self.header_list = [(k.lower(), v) for k, v in headers]
        self.headers = dict(self.header_list)
        self.body = body

    def json(self) -> Any:
        try:
            return json.loads(self.body)
        except ValueError:
            return None

    def code(self) -> str | None:
        data = self.json()
        return data.get("error", {}).get("code") if isinstance(data, dict) else None


def request(target: tuple[str, str, int, str], method: str, path: str,
            headers: dict[str, str] | None = None, body: bytes | None = None,
            host: str | None = None, chunked: bool = False) -> Response:
    scheme, address, port, default_host = target
    if scheme == "https":
        conn: http.client.HTTPConnection = http.client.HTTPSConnection(
            address, port, context=TLS, timeout=60)
    else:
        conn = http.client.HTTPConnection(address, port, timeout=60)
    conn.putrequest(method, path, skip_host=True, skip_accept_encoding=True)
    conn.putheader("Host", host or default_host)
    for key, value in (headers or {}).items():
        conn.putheader(key, value)
    if body is not None and not chunked:
        conn.putheader("Content-Length", str(len(body)))
    if chunked:
        conn.putheader("Transfer-Encoding", "chunked")
    conn.endheaders()
    if body is not None:
        if chunked:
            for i in range(0, len(body), 8192):
                part = body[i:i + 8192]
                conn.send(f"{len(part):x}\r\n".encode() + part + b"\r\n")
            conn.send(b"0\r\n\r\n")
        else:
            conn.send(body)
    resp = conn.getresponse()
    data = resp.read()
    conn.close()
    return Response(resp.status, resp.getheaders(), data)


def json_post(target: tuple[str, str, int, str], path: str, payload: Any,
              headers: dict[str, str] | None = None, **kw: Any) -> Response:
    base = {"Content-Type": "application/json"}
    base.update(headers or {})
    return request(target, "POST", path, base, json.dumps(payload).encode(), **kw)


# --- Security headers -------------------------------------------------------------------------

def api_headers(family: str, resp: Response, *, no_store: bool) -> None:
    h = resp.headers
    problems = []
    expected = {
        "content-security-policy": API_CSP,
        "strict-transport-security": HSTS,
        "x-content-type-options": "nosniff",
        "x-frame-options": "DENY",
        "referrer-policy": "no-referrer",
        "cross-origin-opener-policy": "same-origin",
        "cross-origin-resource-policy": "same-origin",
        "x-robots-tag": "noindex, nofollow",
    }
    for name, value in expected.items():
        if h.get(name) != value:
            problems.append(f"{name}={h.get(name)!r}")
    pp = h.get("permissions-policy", "")
    if "camera=()" not in pp or "interest-cohort" in pp:
        problems.append(f"permissions-policy={pp[:40]!r}")
    for name in ("server", "x-powered-by", "set-cookie"):
        if name in h:
            problems.append(f"{name} present")
    if any(k.startswith("access-control-") for k, _ in resp.header_list):
        problems.append("Access-Control-* present")
    if "x-request-id" not in h:
        problems.append("no x-request-id")
    cache = h.get("cache-control", "")
    if not cache:
        problems.append("no Cache-Control")
    elif no_store and cache != "no-store":
        problems.append(f"cache-control={cache!r} (no-store expected)")
    if resp.status >= 400 and not (h.get("content-type", "").startswith("application/json")
                                   and resp.code()):
        problems.append(f"error body not JSON {{error:{{code}}}} ({h.get('content-type')})")
    check(not problems, "headers", f"{family} [{resp.status}] " + (", ".join(problems) or "complete"))


def front_headers(family: str, resp: Response) -> None:
    h = resp.headers
    problems = []
    if not FRONT_CSP.match(h.get("content-security-policy", "")):
        problems.append(f"csp={h.get('content-security-policy', '')[:60]!r}")
    for name, value in {
        "strict-transport-security": HSTS,
        "x-content-type-options": "nosniff",
        "x-frame-options": "DENY",
        "referrer-policy": "no-referrer",
        "cross-origin-opener-policy": "same-origin",
        "cross-origin-resource-policy": "same-origin",
        "cross-origin-embedder-policy": "require-corp",
    }.items():
        if h.get(name) != value:
            problems.append(f"{name}={h.get(name)!r}")
    if h.get("server", "nginx") != "nginx":
        problems.append(f"server={h.get('server')!r}")
    for name in ("x-powered-by", "set-cookie"):
        if name in h:
            problems.append(f"{name} present")
    if any(k.startswith("access-control-") for k, _ in resp.header_list):
        problems.append("Access-Control-* present")
    check(not problems, "headers", f"{family} [{resp.status}] " + (", ".join(problems) or "complete"))


def headers_on_every_family() -> None:
    # Frontend (nginx through Traefik).
    index = request(PUBLIC, "GET", "/")
    front_headers("nginx: / (SPA)", index)
    asset = re.search(rb'src="(/assets/[^"]+\.js)"', index.body)
    if check(asset is not None, "headers", "index.html references a bundled script"):
        front_headers("nginx: bundled script", request(PUBLIC, "GET", asset.group(1).decode()))
    for path, family in (("/suivi", "nginx: /suivi"), ("/admin", "nginx: /admin"),
                         ("/assets/absent.js", "nginx: missing asset 404"),
                         ("/.env", "nginx: dotfile 404"),
                         ("/.well-known/security.txt", "nginx: security.txt"),
                         ("/idees/inconnue", "nginx: deep route")):
        resp = request(PUBLIC, "GET", path)
        front_headers(family, resp)
    resp = request(PUBLIC, "POST", "/", {"Content-Type": "text/plain"}, b"x")
    check(resp.status == 405, "methods", f"nginx POST / answers {resp.status} (405 expected)")
    front_headers("nginx: 405", resp)
    resp = request(PUBLIC, "GET", "/suivi")
    check(resp.headers.get("cache-control") == "no-store"
          and resp.headers.get("x-robots-tag") == "noindex, nofollow",
          "headers", "nginx /suivi is no-store and noindex")

    # API (through Traefik), every response family.
    api_headers("api: health", request(PUBLIC, "GET", "/api/health"), no_store=False)
    api_headers("api: list", request(PUBLIC, "GET", "/api/ideas"), no_store=False)
    api_headers("api: idea", request(PUBLIC, "GET", f"/api/ideas/{SLUG}"), no_store=False)
    api_headers("api: 404", request(PUBLIC, "GET", "/api/ideas/inconnue"), no_store=True)
    api_headers("api: 404 route", request(PUBLIC, "GET", "/api/nope"), no_store=True)
    api_headers("api: 400 query", request(PUBLIC, "GET", "/api/ideas?per_page=99"),
                no_store=True)
    api_headers("api: 422 body", json_post(PUBLIC, "/api/ideas", {}), no_store=True)
    api_headers("api: 405 OPTIONS", request(PUBLIC, "OPTIONS", "/api/ideas"), no_store=True)
    api_headers("api: 415", request(PUBLIC, "POST", "/api/ideas", {"Content-Type": "text/plain"},
                                    b"x"), no_store=True)
    api_headers("api: 403 origin", json_post(PUBLIC, "/api/ideas", {},
                                             {"Origin": EVIL_ORIGIN}), no_store=True)
    api_headers("api: 413", json_post(PUBLIC, "/api/ideas", {"text": "a" * 70_000}),
                no_store=True)
    api_headers("api: altcha", request(PUBLIC, "GET", "/api/altcha/challenge"), no_store=True)
    api_headers("api: track 404", json_post(PUBLIC, "/api/track", {"token": "A" * 43}),
                no_store=True)
    api_headers("api: methodology", request(PUBLIC, "GET", "/api/methodology"), no_store=False)
    api_headers("api: openapi", request(PUBLIC, "GET", "/api/openapi.json"), no_store=False)
    csv = request(PUBLIC, "GET", "/api/exports/ideas.csv")
    api_headers("api: export csv", csv, no_store=False)
    check(re.fullmatch(r'attachment; filename="iapolitique-ideas-\d{4}-\d{2}-\d{2}\.csv"',
                       csv.headers.get("content-disposition", "")) is not None,
          "headers", f"export Content-Disposition fixed: {csv.headers.get('content-disposition')}")
    etag = csv.headers.get("etag", "")
    api_headers("api: export 304",
                request(PUBLIC, "GET", "/api/exports/ideas.csv", {"If-None-Match": etag}),
                no_store=False)
    # Admin host (Traefik routes /api/admin to the admin port).
    api_headers("admin: 401", request(ADMIN, "GET", "/api/admin/session"), no_store=True)
    api_headers("admin: 200", request(ADMIN, "GET", "/api/admin/dashboard",
                                      {"Authorization": f"Bearer {ADMIN_TOKEN}"}), no_store=True)
    api_headers("admin: 404 job", request(ADMIN, "POST", "/api/admin/jobs/999999/retry",
                                          {"Authorization": f"Bearer {ADMIN_TOKEN}"}),
                no_store=True)


# --- CORS and CSRF ----------------------------------------------------------------------------

def cors_and_csrf() -> None:
    family = "cors"
    for target, label in ((PUBLIC, "public"), (ADMIN, "admin")):
        resp = request(target, "OPTIONS", "/api/ideas" if target is PUBLIC else
                       "/api/admin/session", {
                           "Origin": EVIL_ORIGIN, "Access-Control-Request-Method": "POST",
                           "Access-Control-Request-Headers": "content-type, authorization"})
        check(resp.status == 405 and not any(k.startswith("access-control-")
                                             for k, _ in resp.header_list),
              family, f"{label} preflight from {EVIL_ORIGIN}: {resp.status}, no Access-Control-*")
    resp = request(PUBLIC, "GET", "/api/ideas", {"Origin": EVIL_ORIGIN})
    check("access-control-allow-origin" not in resp.headers, family,
          "simple GET from a foreign origin carries no Access-Control-Allow-Origin")
    resp = request(PUBLIC, "OPTIONS", "/", {"Origin": EVIL_ORIGIN,
                                            "Access-Control-Request-Method": "GET"})
    check(resp.status == 405 and "access-control-allow-origin" not in resp.headers, family,
          f"nginx preflight: {resp.status}, no Access-Control-*")

    family = "csrf"
    bodies = {
        "text/plain": b'{"token":"' + b"A" * 43 + b'"}',
        "application/x-www-form-urlencoded": b"token=" + b"A" * 43,
        "multipart/form-data; boundary=x": b"--x\r\nContent-Disposition: form-data; "
                                           b"name=\"token\"\r\n\r\nA\r\n--x--\r\n",
    }
    for ctype, body in bodies.items():
        for origin in (EVIL_ORIGIN, GOOD_ORIGIN):
            resp = request(PUBLIC, "POST", "/api/track/delete",
                           {"Content-Type": ctype, "Origin": origin,
                            "Sec-Fetch-Site": "cross-site" if origin == EVIL_ORIGIN
                            else "same-origin"}, body)
            check(resp.status in (403, 415), family,
                  f"POST {ctype.split(';')[0]} from {origin}: {resp.status} {resp.code()}")
    spoofs = [
        ({"Origin": "null"}, "Origin: null"),
        ({"Origin": "https://localhost.evil.example"}, "Origin suffix trick"),
        ({"Origin": "https://evil.example/https://localhost"}, "Origin with path"),
        ({"Origin": "http://localhost"}, "Origin with http scheme"),
        ({"Origin": "https://localhost:8443"}, "Origin with another port"),
        ({"Origin": "https://evillocalhost"}, "Origin prefix trick"),
        ({"Origin": f"{GOOD_ORIGIN}, {EVIL_ORIGIN}"}, "Origin list"),
        ({"Origin": GOOD_ORIGIN, "Sec-Fetch-Site": "cross-site"}, "allowed Origin + cross-site"),
        ({"Origin": GOOD_ORIGIN, "Sec-Fetch-Site": "same-site"}, "allowed Origin + same-site"),
        ({"Sec-Fetch-Site": "cross-site"}, "no Origin, Sec-Fetch-Site cross-site"),
        ({"Sec-Fetch-Site": "same-site"}, "no Origin, Sec-Fetch-Site same-site"),
        ({"Origin": EVIL_ORIGIN, "Referer": GOOD_ORIGIN + "/"}, "foreign Origin + local Referer"),
    ]
    for headers, label in spoofs:
        resp = json_post(PUBLIC, "/api/track/delete", {"token": "A" * 43}, headers)
        check(resp.status == 403 and resp.code() == "origin_not_allowed", family,
              f"{label}: {resp.status} {resp.code()}")
    resp = json_post(PUBLIC, "/api/track/delete", {"token": "A" * 43},
                     {"Origin": GOOD_ORIGIN, "Sec-Fetch-Site": "same-origin"})
    check(resp.status not in (403, 415), family,
          f"same-origin JSON POST passes the CSRF layer ({resp.status} {resp.code()})")
    resp = json_post(PUBLIC, "/api/track/delete", {"token": "A" * 43},
                     {"Origin": "https://admin.localhost"})
    check(resp.status not in (403, 415), family,
          f"admin-host Origin accepted on the public port ({resp.status}), as specified (§14.3)")
    # Admin port: only the admin host's origin.
    auth = {"Authorization": f"Bearer {ADMIN_TOKEN}"}
    for origin in (GOOD_ORIGIN, EVIL_ORIGIN, "null"):
        resp = json_post(ADMIN, "/api/admin/ideas/00000000-0000-0000-0000-000000000000/hide",
                         {"reason": "x"}, {**auth, "Origin": origin})
        check(resp.status == 403, family, f"admin mutation with Origin {origin}: {resp.status}")
    resp = request(ADMIN, "PUT", "/api/admin/settings/dedup-threshold",
                   {**auth, "Content-Type": "text/plain", "Origin": "https://admin.localhost"},
                   b'{"value":0.9}')
    check(resp.status == 415, family, f"admin PUT text/plain: {resp.status}")
    resp = request(ADMIN, "DELETE", "/api/admin/replies/00000000-0000-0000-0000-000000000000",
                   {**auth, "Origin": EVIL_ORIGIN})
    check(resp.status == 403, family, f"admin bodyless DELETE from a foreign origin: {resp.status}")
    # Method override headers have no effect.
    resp = request(PUBLIC, "GET", "/api/ideas", {"X-HTTP-Method-Override": "DELETE",
                                                  "X-Method-Override": "PUT"})
    check(resp.status == 200, "methods", f"X-HTTP-Method-Override ignored ({resp.status})")
    for method in ("TRACE", "PUT", "PATCH", "DELETE", "PROPFIND", "CONNECT"):
        path = "localhost:443" if method == "CONNECT" else "/api/ideas"
        resp = request(PUBLIC, method, path, {"Origin": GOOD_ORIGIN})
        check(resp.status in (400, 404, 405), "methods", f"{method} /api/ideas: {resp.status}")


# --- Host header and URL tricks ---------------------------------------------------------------

def host_tricks() -> None:
    family = "host"
    resp = request(PUBLIC, "GET", "/api/health", host="evil.example")
    check(resp.status == 404, family, f"unknown Host through Traefik: {resp.status} (no router)")
    resp = request(PUBLIC, "GET", "/", host="localhost:443")
    check(resp.status == 200, family, f"Host with explicit port routed normally ({resp.status})")
    # The api and nginx never build a URL from Host / X-Forwarded-*.
    tricks = {"X-Forwarded-Host": "evil.example", "X-Forwarded-Proto": "http",
              "X-Forwarded-Prefix": "/evil", "X-Original-URL": "/api/admin/session",
              "X-Rewrite-URL": "/api/admin/session", "Forwarded": "host=evil.example"}
    resp = request(API_DIRECT, "GET", f"/api/ideas/{SLUG}", tricks, host="evil.example")
    data = resp.json() or {}
    check(resp.status == 200 and data.get("canonical_url", "").startswith("https://localhost/")
          and b"evil" not in resp.body, family,
          f"idea canonical_url ignores Host/X-Forwarded-* ({data.get('canonical_url')})")
    resp = request(API_DIRECT, "GET", "/api/health", tricks)
    check(resp.status == 200 and b"authenticated" not in resp.body, family,
          "X-Original-URL / X-Rewrite-URL do not reroute to the admin API")
    resp = request(FRONT_DIRECT, "GET", "/.well-known/security.txt", tricks, host="evil.example")
    check(b"evil" not in resp.body and b"https://localhost/.well-known/security.txt" in resp.body,
          family, "security.txt Canonical comes from TRAEFIK_HOST, not Host")
    resp = request(FRONT_DIRECT, "GET", "/", tricks, host="evil.example")
    check(b"evil" not in resp.body, family, "index.html does not reflect Host")
    resp = request(FRONT_DIRECT, "GET", "/admin", host="evil.example")
    check(b"evil" not in resp.body and "evil" not in resp.headers.get("location", ""),
          family, f"nginx redirects never use Host ({resp.status})")
    # Absolute-form request target: Traefik routes on its host, like a Host header.
    resp = request(PUBLIC, "GET", "https://admin.localhost/api/admin/session", host="localhost")
    record("info", family, f"absolute-form URI to the admin host: {resp.status} (routed like "
           "its Host; in production dashboard-allowlist@file filters by source address)")
    # Path traversal and encodings on nginx.
    for path in ("/../../etc/passwd", "/assets/..%2f..%2f..%2fetc%2fpasswd",
                 "/%2e%2e/%2e%2e/etc/passwd", "/.git/config", "/.env", "/.htaccess",
                 "/nginx.conf", "/etc/nginx/nginx.conf", "/.well-known/../.env"):
        resp = request(PUBLIC, "GET", path)
        leaked = b"root:" in resp.body or b"[core]" in resp.body or b"server_tokens" in resp.body
        check(not leaked and resp.status in (200, 400, 404), "traversal",
              f"{path}: {resp.status}" + (" LEAK" if leaked else ", nothing leaked"))


# --- Oversized and malformed input ------------------------------------------------------------

def oversized_and_malformed() -> None:
    family = "limits"
    big = json.dumps({"text": "a" * 70_000}).encode()
    resp = request(PUBLIC, "POST", "/api/ideas", {"Content-Type": "application/json"}, big)
    check(resp.status == 413 and resp.code(), family, f"70 kB body: {resp.status} {resp.code()}")
    resp = request(PUBLIC, "POST", "/api/ideas", {"Content-Type": "application/json"}, big,
                   chunked=True)
    check(resp.status == 413, family, f"70 kB chunked body: {resp.status} {resp.code()}")
    resp = request(PUBLIC, "POST", "/api/ideas", {"Content-Type": "application/json"},
                   b"[" * 20_000 + b"]" * 20_000)
    check(resp.status in (400, 422), family, f"JSON nested 20 000 deep: {resp.status} {resp.code()}")
    resp = request(PUBLIC, "POST", "/api/track", {"Content-Type": "application/json"},
                   b'{"token":"\xff\xfe"}')
    check(resp.status in (400, 422), family, f"invalid UTF-8 body: {resp.status} {resp.code()}")
    resp = request(PUBLIC, "POST", "/api/track", {"Content-Type": "application/json"},
                   b'{"token":"' + b"A" * 43 + b'","token":"B"}')
    check(resp.status in (400, 404, 422), family, f"duplicate JSON key: {resp.status} {resp.code()}")
    resp = json_post(PUBLIC, "/api/track", {"token": "A" * 43, "admin": True})
    check(resp.status in (400, 422), family, f"unknown JSON field: {resp.status} {resp.code()}")
    for token in ("A" * 42, "A" * 44, "%" * 43, "' OR 1=1--" + "A" * 33, "\u0000" * 43):
        resp = json_post(PUBLIC, "/api/track", {"token": token})
        # 429 when a previous run within the hour used up the tracking limit of this address.
        check(resp.status in (404, 422, 429), family,
              f"malformed token {token[:12]!r}…: {resp.status}")
    queries = ["per_page=51", "per_page=0", "page=10001", "page=-1", "per_page=1e309",
               "sort=score;DROP%20TABLE%20ideas", "q=" + "a" * 201, "unknown=1",
               "admin_level=%27%20OR%201%3D1--", "category=../../etc"]
    for query in queries:
        resp = request(PUBLIC, "GET", f"/api/ideas?{query}")
        check(resp.status == 400 and resp.code() == "invalid_query", family,
              f"GET /api/ideas?{query[:40]}: {resp.status} {resp.code()}")
    for query in ("q=%27%20OR%201%3D1--", "q=%25%25%25", "q=%5C", "territory=%25_%5C"):
        resp = request(PUBLIC, "GET", f"/api/ideas?{query}")
        check(resp.status == 200, family, f"SQL metacharacters in {query}: {resp.status}")
    for slug in ("%00", "..%2f..%2fadmin", "a" * 300, "%3Cscript%3E", "' OR '1'='1"):
        resp = request(PUBLIC, "GET", f"/api/ideas/{slug.replace(' ', '%20')}")
        check(resp.status in (400, 404) and b"<script>" not in resp.body, family,
              f"slug {slug[:16]!r}: {resp.status}, payload not reflected")
    resp = request(PUBLIC, "GET", "/api/ideas?sort=%3Cscript%3Ealert(1)%3C/script%3E")
    reflected = b"<script>" in resp.body
    record("ok" if not reflected else "info", "reflection",
           f"invalid sort value {'reflected in' if reflected else 'absent from'} the JSON error "
           f"(application/json + nosniff, never HTML)")
    resp = request(PUBLIC, "GET", "/api/ideas?q=" + "a" * 8000)
    check(resp.status in (400, 414, 431), family, f"8 kB query string: {resp.status}")
    resp = request(PUBLIC, "GET", "/api/health", {"X-Big": "a" * 70_000})
    check(resp.status < 500, family, f"70 kB header: {resp.status} (bounded by Traefik's 1 MiB "
          "and hyper's buffer, see the 2 MiB probe)")
    try:
        status = str(request(PUBLIC, "GET", "/api/health", {"X-Big": "a" * 2_100_000}).status)
    except (ConnectionError, http.client.HTTPException, OSError) as err:
        status = f"connection closed ({type(err).__name__})"
    check(not status.startswith(("2", "5")), family, f"2 MiB header refused: {status}")


# --- Connection reuse behind Traefik (ZAP-2) ---------------------------------------------------

def connection_reuse() -> None:
    family = "connection"
    # A body announced but never sent: the api waits for it (drain, at most 2 s), then answers
    # with Connection: close so that no proxy reuses the connection.
    with socket.create_connection(("e2e-api", 8080), timeout=10) as sock:
        sock.sendall(b"POST /api/nope HTTP/1.1\r\nHost: localhost\r\n"
                     b"Content-Type: application/json\r\nContent-Length: 2\r\n\r\n")
        head = b""
        while b"\r\n\r\n" not in head:
            chunk = sock.recv(4096)
            if not chunk:
                break
            head += chunk
    head_text = head.split(b"\r\n\r\n")[0].decode(errors="replace").lower()
    check("\r\nconnection: close" in head_text, family,
          f"body never sent: answer says Connection: close ({head_text.splitlines()[:1]})")
    # Rejected and accepted POSTs interleaved from several clients through Traefik: no 5xx.
    statuses: list[int] = []
    lock = threading.Lock()

    def worker() -> None:
        for i in range(40):
            if i % 2:
                resp = request(PUBLIC, "POST", "/api/nope", {"Content-Type": "text/plain"},
                               b"x" * 4096)
            else:
                resp = json_post(PUBLIC, f"/api/ideas/{SLUG}/reports",
                                 {"reason": "autre", "good_faith": True, "altcha": "x"})
            with lock:
                statuses.append(resp.status)

    threads = [threading.Thread(target=worker) for _ in range(6)]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    errors = [s for s in statuses if s >= 500]
    check(not errors, family, f"{len(statuses)} interleaved POSTs through Traefik, "
          f"{len(errors)} 5xx ({sorted(set(errors))})")


# --- Admin routes on the public host and port -------------------------------------------------

def admin_isolation() -> None:
    family = "admin"
    auth = {"Authorization": f"Bearer {ADMIN_TOKEN}"}
    paths = ["/api/admin/session", "/api/admin/dashboard", "/api/admin//session",
             "/api/admin%2Fsession", "/api/Admin/session", "/api/ADMIN/session",
             "/api/./admin/session", "/api/admin/../admin/session", "/api//admin/session",
             "/api/admin/session/", "/api/admin/session;x", "/api/%61dmin/session",
             "/api/admin/screenings/export.jsonl"]
    for path in paths:
        resp = request(PUBLIC, "GET", path, auth)
        check(resp.status in (400, 404) and b"authenticated" not in resp.body, family,
              f"public host {path} with a valid token: {resp.status}")
    for path in ("/api/admin/session", "/api/admin/dashboard"):
        resp = request(API_DIRECT, "GET", path, auth, host="admin.localhost")
        check(resp.status == 404, family, f"public port 8080 {path} (Host admin.localhost): "
              f"{resp.status}")
    resp = request(PUBLIC, "GET", "/api/openapi.json")
    check(resp.status == 200 and b"/api/admin" not in resp.body, family,
          "public OpenAPI spec has no admin route")
    for path in ("/api/ideas", "/api/track", "/api/altcha/challenge"):
        resp = request(ADMIN_DIRECT, "GET", path)
        check(resp.status == 404, family, f"admin port serves no public route {path}: "
              f"{resp.status}")
    resp = request(ADMIN, "GET", "/api/admin/session", auth)
    check(resp.status == 200, family, f"admin host with the token: {resp.status}")
    for value in ("Bearer", "Bearer ", "Basic YWRtaW46YWRtaW4=", f"bearer {ADMIN_TOKEN}x",
                  f"Bearer {ADMIN_TOKEN[:-1]}"):
        resp = request(ADMIN, "GET", "/api/admin/session", {"Authorization": value})
        check(resp.status in (401, 429), family, f"Authorization {value[:14]!r}…: {resp.status}")
    resp = request(ADMIN, "GET", f"/api/admin/session?token={ADMIN_TOKEN}")
    check(resp.status in (400, 401), family, f"token in the query string ignored ({resp.status})")


# --- X-Forwarded-For spoofing -------------------------------------------------------------------

def xff_spoofing() -> None:
    family = "xff"

    def until_429(target: tuple[str, str, int, str], xff: Any, limit: int = 130) -> int | None:
        for i in range(1, limit + 1):
            headers = {"Content-Type": "application/json"}
            value = xff(i)
            if value:
                headers["X-Forwarded-For"] = value
            resp = request(target, "POST", "/api/track", headers,
                           json.dumps({"token": "A" * 43}).encode())
            if resp.status == 429:
                return i
        return None

    hit = until_429(PUBLIC, lambda i: f"203.0.113.{i % 250}, 198.51.100.{i % 250}")
    check(hit is not None and hit <= 121, family,
          f"through Traefik, rotating X-Forwarded-For does not escape the tracking limit "
          f"(429 at request {hit})")
    hit = until_429(API_DIRECT, lambda i: "192.0.2.77")
    check(hit is not None and hit <= 121, family,
          f"direct to the api, one client address is limited (429 at request {hit})")
    hit = until_429(API_DIRECT, lambda i: f"198.18.0.{i}", limit=125)
    record("info" if hit is None else "ok", family,
           "direct to the api port (no Traefik), the client picks its X-Forwarded-For: "
           f"{'no 429 in 125 requests' if hit is None else f'429 at {hit}'} — residual risk "
           "NET-1 (AGENTS.md §14.4), closed only by a network reachable from Traefik alone")


def main() -> int:
    if not ADMIN_TOKEN:
        print("ADMIN_TOKEN is required", file=sys.stderr)
        return 2
    headers_on_every_family()
    cors_and_csrf()
    host_tricks()
    oversized_and_malformed()
    admin_isolation()
    connection_reuse()
    xff_spoofing()  # last: it exhausts the tracking limit of the proxy's address
    if os.environ.get("RESULTS"):
        with open(os.environ["RESULTS"], "w", encoding="utf-8") as handle:
            json.dump(RESULTS, handle, ensure_ascii=False, indent=1)
    failures = [r for r in RESULTS if r["level"] == "FAIL"]
    infos = [r for r in RESULTS if r["level"] == "info"]
    print(f"{len(RESULTS)} probes: {len(failures)} failure(s), {len(infos)} documented residual(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
