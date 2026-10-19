#!/usr/bin/env python3
"""Tests of devops/check_compose_security.py: the real compose file passes, and each broken
invariant is reported. Run with `python3 -m unittest devops/test_check_compose_security.py`
(CI: devops/Dockerfile.test, stage `devops-check`)."""

from __future__ import annotations

import copy
import sys
import unittest
from pathlib import Path

import yaml

sys.path.insert(0, str(Path(__file__).parent))
import check_compose_security as checker  # noqa: E402

COMPOSE = yaml.safe_load(
    (Path(__file__).with_name("docker-compose.swarm.yml")).read_text(encoding="utf-8")
)


def labels(compose: dict, service: str) -> list[str]:
    return compose["services"][service]["deploy"].setdefault("labels", [])


def set_label(compose: dict, service: str, key: str, value: str) -> None:
    entries = labels(compose, service)
    for index, entry in enumerate(entries):
        if entry.split("=", 1)[0] == key:
            entries[index] = f"{key}={value}"
            return
    entries.append(f"{key}={value}")


class ComposeSecurityTest(unittest.TestCase):
    def errors_after(self, mutate) -> list[str]:
        compose = copy.deepcopy(COMPOSE)
        mutate(compose)
        return checker.check(compose)

    def assert_reported(self, mutate, fragment: str) -> None:
        errors = self.errors_after(mutate)
        self.assertTrue(any(fragment in error for error in errors), f"{fragment!r} not in {errors}")

    def test_the_repository_compose_file_passes(self) -> None:
        self.assertEqual(checker.check(COMPOSE), [])

    def test_hardening_is_required_on_every_deployed_service(self) -> None:
        for service in checker.DEPLOYED:
            with self.subTest(service=service):
                self.assert_reported(
                    lambda c, s=service: c["services"][s].pop("read_only"), f"{service}: read_only"
                )
                self.assert_reported(lambda c, s=service: c["services"][s].pop("init"), f"{service}: init")
                self.assert_reported(
                    lambda c, s=service: c["services"][s].pop("cap_drop"), f"{service}: cap_drop"
                )

    def test_resource_limits_are_required_on_every_deployed_service(self) -> None:
        # DOS-1: a memory limit alone lets a flooded api take every core of a shared node.
        for service in checker.DEPLOYED:
            for key in ("cpus", "memory", "pids"):
                with self.subTest(service=service, key=key):
                    self.assert_reported(
                        lambda c, s=service, k=key: c["services"][s]["deploy"]["resources"]["limits"].pop(k),
                        f"{service}: deploy.resources.limits must set ['{key}']",
                    )
            with self.subTest(service=service, key="reservations"):
                self.assert_reported(
                    lambda c, s=service: c["services"][s]["deploy"]["resources"].pop("reservations"),
                    f"{service}: deploy.resources.reservations.memory",
                )

    def test_extra_capabilities_are_refused(self) -> None:
        self.assert_reported(
            lambda c: c["services"]["api"].setdefault("cap_add", []).append("NET_ADMIN"),
            "api: unexpected capabilities ['NET_ADMIN']",
        )
        self.assert_reported(
            lambda c: c["services"]["frontend"]["cap_add"].append("SYS_ADMIN"),
            "frontend: unexpected capabilities ['SYS_ADMIN']",
        )

    def test_short_tmpfs_syntax_bind_mounts_and_ports_are_refused(self) -> None:
        self.assert_reported(lambda c: c["services"]["worker"].update(tmpfs=["/tmp"]), "short `tmpfs:`")
        self.assert_reported(
            lambda c: c["services"]["frontend"]["volumes"].append("/srv:/usr/share/nginx/html"),
            "only tmpfs volumes",
        )
        self.assert_reported(lambda c: c["services"]["api"].update(ports=["8081:8081"]), "published port")

    def test_database_url_is_refused(self) -> None:
        self.assert_reported(
            lambda c: c["services"]["worker"]["environment"].update(DATABASE_URL="postgres://x"),
            "DATABASE_URL",
        )

    def test_dev_origins_are_never_deployed(self) -> None:
        self.assert_reported(
            lambda c: c["services"]["api"]["environment"].update(
                ALLOWED_DEV_ORIGINS="http://localhost:5173"
            ),
            "api: ALLOWED_DEV_ORIGINS is for local development only",
        )

    def test_worker_stays_off_the_proxy_network(self) -> None:
        self.assert_reported(
            lambda c: c["services"]["worker"]["networks"].append("proxy"),
            "worker: only ['api', 'frontend'] may join the proxy network",
        )
        self.assert_reported(
            lambda c: c["services"]["worker"]["deploy"].update(labels=["traefik.enable=true"]),
            "worker: must carry no Traefik label",
        )
        self.assert_reported(
            lambda c: c["services"]["test"]["networks"].append("proxy"), "test: CI services"
        )

    def test_no_new_shared_network_without_a_decision(self) -> None:
        # NET-1/NET-4: every overlay the api joins can reach 8080 and 8081 without Traefik.
        self.assert_reported(
            lambda c: c["services"]["api"]["networks"].append("monitoring"),
            "api: unexpected networks ['monitoring']",
        )
        self.assert_reported(
            lambda c: c["services"]["frontend"]["networks"].append("llm"),
            "frontend: unexpected networks ['llm']",
        )

    def test_admin_port_only_behind_admin_host_and_allowlist(self) -> None:
        self.assert_reported(
            lambda c: set_label(
                c, "api", "traefik.http.routers.iapolitique-api.service", "iapolitique-api-admin"
            ),
            "admin port 8081 routed outside ADMIN_HOST",
        )
        self.assert_reported(
            lambda c: set_label(
                c,
                "api",
                "traefik.http.routers.iapolitique-admin-api.middlewares",
                "security-headers@file",
            ),
            "admin host without dashboard-allowlist@file",
        )
        self.assert_reported(
            lambda c: set_label(
                c,
                "api",
                "traefik.http.routers.iapolitique-admin-api.rule",
                "Host(`${ADMIN_HOST}`) && PathPrefix(`/api`)",
            ),
            "admin port must only serve /api/admin",
        )

    def test_router_rules_are_one_host_and_one_path_matcher(self) -> None:
        # DEVOPS-7: a substring check accepted these; each would widen the admin router.
        admin_rule = "traefik.http.routers.iapolitique-admin-api.rule"
        for rule in (
            "Host(`${ADMIN_HOST}`) || PathPrefix(`/api/admin`)",
            "(Host(`${ADMIN_HOST}`) || Host(`${TRAEFIK_HOST}`)) && PathPrefix(`/api/admin`)",
            "Host(`${ADMIN_HOST}`) && (PathPrefix(`/api/admin`) || PathPrefix(`/`))",
            "HostRegexp(`.+`) && PathPrefix(`/api/admin`)",
            "Host(`${ADMIN_HOST}`) && !PathPrefix(`/api/public`)",
            "Host(`${ADMIN_HOST}`) && PathPrefix(`/api/admin`) && Header(`X-A`, `b`)",
            "Host(`${ADMIN_HOST}`) && ClientIP(`0.0.0.0/0`)",
        ):
            with self.subTest(rule=rule):
                self.assert_reported(lambda c, r=rule: set_label(c, "api", admin_rule, r), "rule must be Host(")

    def test_admin_port_serves_exactly_the_admin_prefix(self) -> None:
        self.assert_reported(
            lambda c: set_label(
                c,
                "api",
                "traefik.http.routers.iapolitique-admin-api.rule",
                "Host(`${ADMIN_HOST}`) && PathPrefix(`/api/admin-and-more`)",
            ),
            "admin port must only serve /api/admin",
        )

    def test_public_admin_path_redirects_to_the_admin_host(self) -> None:
        # FE-5: the admin pages must never run on the public origin.
        redirect = "traefik.http.routers.iapolitique-admin-redirect"
        self.assert_reported(
            lambda c: labels(c, "frontend").remove(f"{redirect}.service=noop@internal"),
            "no HTTPS router redirecting",
        )
        self.assert_reported(
            lambda c: set_label(
                c,
                "frontend",
                "traefik.http.middlewares.iapolitique-admin-redirect.redirectregex.replacement",
                "https://${TRAEFIK_HOST}/",
            ),
            "no HTTPS router redirecting",
        )
        self.assert_reported(
            lambda c: set_label(c, "frontend", f"{redirect}.middlewares", "iapolitique-admin-redirect"),
            "public router without global@file",
        )

    def test_public_routers_keep_global_middleware(self) -> None:
        self.assert_reported(
            lambda c: set_label(
                c, "frontend", "traefik.http.routers.iapolitique-front.middlewares", "security-headers@file"
            ),
            "public router without global@file",
        )

    def test_http_routers_only_redirect(self) -> None:
        self.assert_reported(
            lambda c: set_label(
                c, "frontend", "traefik.http.routers.iapolitique-front-http.service", "iapolitique-front"
            ),
            "an HTTP router may only redirect",
        )

    def test_router_rules_name_one_known_host(self) -> None:
        self.assert_reported(
            lambda c: set_label(c, "frontend", "traefik.http.routers.iapolitique-front.rule", "PathPrefix(`/`)"),
            "rule must be Host(",
        )
        self.assert_reported(
            lambda c: set_label(c, "frontend", "traefik.http.routers.extra.rule", "Host(`evil.example`)"),
            "rule must match exactly one known Host()",
        )

    def test_routers_name_their_service(self) -> None:
        self.assert_reported(
            lambda c: labels(c, "api").remove("traefik.http.routers.iapolitique-api.service=iapolitique-api"),
            "router iapolitique-api: must name its service explicitly",
        )

    def test_unknown_services_must_be_classified(self) -> None:
        self.assert_reported(
            lambda c: c["services"].update(debug={"image": "busybox"}), "unknown services ['debug']"
        )


if __name__ == "__main__":
    unittest.main()
