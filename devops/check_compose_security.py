#!/usr/bin/env python3
"""Security invariants of devops/docker-compose.swarm.yml (AGENTS.md §8, §10.1, §14.4, §14.7).

Run in CI by devops/Dockerfile.test (stage `devops-check`) and locally with
`python3 devops/check_compose_security.py [COMPOSE_FILE]` (needs PyYAML). Exits 1 and lists every
broken invariant:

- deployed services (api, worker, frontend): read-only root file system, init, every capability
  dropped and only the listed ones added back, writable paths as tmpfs mounts only, no published
  port, no `DATABASE_URL`;
- networks: only api and frontend on `proxy`; the worker on no routed network; each deployed
  service on a closed list of networks (a shared overlay exposes its ports outside Traefik); the
  CI services on their internal network only;
- Traefik: every router names its service, its entry point and a rule made of exactly one Host()
  on TRAEFIK_HOST or ADMIN_HOST, optionally AND one PathPrefix/PathRegexp (no OR, negation, group
  or other matcher); HTTPS routers on TRAEFIK_HOST carry `global@file`, those on ADMIN_HOST
  `dashboard-allowlist@file`; HTTP routers only redirect; the admin port 8081 is reachable only
  through `Host(ADMIN_HOST) && PathPrefix(/api/admin)` behind `dashboard-allowlist@file`, and the
  worker has no label.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

import yaml

DEPLOYED = ("api", "worker", "frontend")
CI_ONLY = ("test", "test-db")
ALLOWED_CAPS = {
    "api": set(),
    "worker": set(),
    "frontend": {"CHOWN", "SETGID", "SETUID", "NET_BIND_SERVICE"},
}
PROXY_SERVICES = {"api", "frontend"}
# Every shared overlay a service joins exposes its ports to all the other containers on it, outside
# Traefik (docs/ops.md §5.4, findings NET-1/NET-4): the list is closed, a new network is a decision.
ALLOWED_NETWORKS = {
    "api": {"proxy", "postgresqlcluster_internal", "llm"},
    "worker": {"postgresqlcluster_internal", "llm"},
    "frontend": {"proxy"},
}
PUBLIC_HOST = "${TRAEFIK_HOST}"
ADMIN_HOST = "${ADMIN_HOST}"
WWW_HOST = "www.${TRAEFIK_HOST}"
ADMIN_PORT = "8081"
HOST_RULE = re.compile(r"Host\(`([^`]+)`\)")
# Rule grammar accepted on a router (finding DEVOPS-7): one Host(), optionally AND one path
# matcher. No OR, no negation, no group, no other matcher: a substring check would let
# "Host(`a`) || PathPrefix(`/api/admin`)" through, which matches every host.
FORBIDDEN_RULE = re.compile(r"\|\||!|HostRegexp|HostSNI|ClientIP|Header|Query|Method|\(\s*(Host|Path)")
RULE_SHAPE = re.compile(r"Host\(`[^`]+`\)(\s*&&\s*(PathPrefix|PathRegexp)\(`[^`]+`\))?")
ADMIN_API_RULE = f"Host(`{ADMIN_HOST}`) && PathPrefix(`/api/admin`)"


def service_networks(service: dict) -> set[str]:
    networks = service.get("networks") or []
    return set(networks) if isinstance(networks, list) else set(networks.keys())


def labels_of(service: dict) -> dict[str, str]:
    labels = (service.get("deploy") or {}).get("labels") or {}
    if isinstance(labels, dict):
        return {str(key): str(value) for key, value in labels.items()}
    return dict(str(label).split("=", 1) for label in labels)


def check_hardening(name: str, service: dict, errors: list[str]) -> None:
    if service.get("read_only") is not True:
        errors.append(f"{name}: read_only must be true")
    if service.get("init") is not True:
        errors.append(f"{name}: init must be true")
    if [cap.upper() for cap in service.get("cap_drop") or []] != ["ALL"]:
        errors.append(f"{name}: cap_drop must be [ALL]")
    added = {cap.upper().removeprefix("CAP_") for cap in service.get("cap_add") or []}
    if not added <= ALLOWED_CAPS[name]:
        errors.append(f"{name}: unexpected capabilities {sorted(added - ALLOWED_CAPS[name])}")
    if "tmpfs" in service:
        errors.append(f"{name}: the short `tmpfs:` key is ignored by Swarm, use a tmpfs volume")
    for volume in service.get("volumes") or []:
        if not isinstance(volume, dict) or volume.get("type") != "tmpfs":
            errors.append(f"{name}: only tmpfs volumes are allowed, found {volume!r}")
    if service.get("ports"):
        errors.append(f"{name}: no published port (everything goes through Traefik)")
    if service.get("privileged"):
        errors.append(f"{name}: privileged is forbidden")
    if service.get("network_mode") or service.get("pid") or service.get("ipc"):
        errors.append(f"{name}: host namespaces are forbidden")
    environment = service.get("environment") or {}
    keys = environment.keys() if isinstance(environment, dict) else [e.split("=", 1)[0] for e in environment]
    if "DATABASE_URL" in keys:
        errors.append(f"{name}: DATABASE_URL would be deployed in clear (AGENTS.md §10.3)")


def check_networks(services: dict, errors: list[str]) -> None:
    for name, service in services.items():
        networks = service_networks(service)
        if "proxy" in networks and name not in PROXY_SERVICES:
            errors.append(f"{name}: only {sorted(PROXY_SERVICES)} may join the proxy network")
    for name in CI_ONLY:
        if name in services and service_networks(services[name]) != {"test"}:
            errors.append(f"{name}: CI services stay on the internal `test` network")
    for name in PROXY_SERVICES:
        if "proxy" not in service_networks(services.get(name, {})):
            errors.append(f"{name}: must join the proxy network to be routed")
    for name, allowed in ALLOWED_NETWORKS.items():
        extra = service_networks(services.get(name, {})) - allowed
        if extra:
            errors.append(
                f"{name}: unexpected networks {sorted(extra)} (each shared overlay exposes the "
                "service's ports outside Traefik, docs/ops.md §5.4)"
            )


def check_traefik(services: dict, errors: list[str]) -> None:
    worker_labels = labels_of(services.get("worker", {}))
    if worker_labels:
        errors.append(f"worker: must carry no Traefik label, found {sorted(worker_labels)}")

    admin_services: set[str] = set()
    for name, service in services.items():
        labels = labels_of(service)
        if not any(key.startswith("traefik.") for key in labels):
            continue
        if name not in PROXY_SERVICES:
            errors.append(f"{name}: Traefik labels on a service that is not routed")
        if labels.get("traefik.swarm.network") != "proxy":
            errors.append(f"{name}: traefik.swarm.network must be proxy")
        for key, value in labels.items():
            match = re.fullmatch(r"traefik\.http\.services\.([^.]+)\.loadbalancer\.server\.port", key)
            if match and value == ADMIN_PORT:
                admin_services.add(match.group(1))

        routers: dict[str, dict[str, str]] = {}
        for key, value in labels.items():
            match = re.fullmatch(r"traefik\.http\.routers\.([^.]+)\.(.+)", key)
            if match:
                routers.setdefault(match.group(1), {})[match.group(2)] = value

        for router, settings in routers.items():
            where = f"{name}: router {router}"
            rule = settings.get("rule", "")
            if FORBIDDEN_RULE.search(rule) or not RULE_SHAPE.fullmatch(rule.strip()):
                errors.append(
                    f"{where}: rule must be Host(`...`) optionally && one PathPrefix/PathRegexp, "
                    f"with no OR, negation, group or other matcher, got {rule!r}"
                )
                continue
            hosts = set(HOST_RULE.findall(rule))
            middlewares = [m.strip() for m in settings.get("middlewares", "").split(",") if m.strip()]
            entrypoint = settings.get("entrypoints")
            target = settings.get("service")
            if not target:
                errors.append(f"{where}: must name its service explicitly")
            if len(hosts) != 1 or not hosts <= {PUBLIC_HOST, ADMIN_HOST, WWW_HOST}:
                errors.append(f"{where}: rule must match exactly one known Host(), got {rule!r}")
                continue
            host = next(iter(hosts))
            if entrypoint == "web":
                if target != "noop@internal" or not middlewares or not all(
                    m == "redirect-to-https@file" or m.startswith("iapolitique-www-to-apex")
                    for m in middlewares
                ):
                    errors.append(f"{where}: an HTTP router may only redirect (noop@internal)")
            elif entrypoint == "websecure":
                if settings.get("tls.certresolver") != "letsencrypt":
                    errors.append(f"{where}: HTTPS router without the letsencrypt resolver")
                if host == ADMIN_HOST:
                    if "dashboard-allowlist@file" not in middlewares:
                        errors.append(f"{where}: admin host without dashboard-allowlist@file")
                elif "global@file" not in middlewares:
                    errors.append(f"{where}: public router without global@file (WAF, rate limit)")
            else:
                errors.append(f"{where}: unknown entry point {entrypoint!r}")
            if target in admin_services:
                if host != ADMIN_HOST or "dashboard-allowlist@file" not in middlewares:
                    errors.append(f"{where}: admin port {ADMIN_PORT} routed outside ADMIN_HOST")
                if entrypoint != "websecure" or rule.strip() != ADMIN_API_RULE:
                    errors.append(f"{where}: admin port must only serve /api/admin over HTTPS")

    if not admin_services:
        errors.append("api: no Traefik service on the admin port 8081 (AGENTS.md §8)")


def check(compose: dict) -> list[str]:
    errors: list[str] = []
    services = compose.get("services") or {}
    for name in DEPLOYED:
        if name not in services:
            errors.append(f"{name}: service missing")
            continue
        check_hardening(name, services[name], errors)
    unknown = set(services) - set(DEPLOYED) - set(CI_ONLY)
    if unknown:
        errors.append(f"unknown services {sorted(unknown)}: add them to this check")
    check_networks(services, errors)
    check_traefik(services, errors)
    return errors


def main() -> int:
    path = Path(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).with_name("docker-compose.swarm.yml"))
    errors = check(yaml.safe_load(path.read_text(encoding="utf-8")))
    for error in errors:
        print(f"FAIL  {error}", file=sys.stderr)
    if errors:
        print(f"{len(errors)} compose security invariant(s) broken in {path}", file=sys.stderr)
        return 1
    print(f"ok    compose security invariants ({path})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
