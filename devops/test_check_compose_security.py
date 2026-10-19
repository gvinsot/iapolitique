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
            "rule must match exactly one known Host()",
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
