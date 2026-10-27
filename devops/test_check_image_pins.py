#!/usr/bin/env python3
"""Tests of devops/check_image_pins.py (finding SUP-1). Run with
`python3 -m unittest devops/test_check_image_pins.py` (CI: devops/Dockerfile.test, stage
`devops-check`)."""

from __future__ import annotations

import re
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import check_image_pins as checker  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
DIGEST_A = "sha256:" + "a" * 64
DIGEST_B = "sha256:" + "b" * 64


class ImagePinsTest(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp)
        for name in (*checker.DOCKERFILES, checker.COMPOSE):
            target = self.tmp / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / name, target)

    def write(self, name: str, text: str) -> None:
        (self.tmp / name).write_text(text, encoding="utf-8")

    def errors(self) -> list[str]:
        return checker.check(self.tmp)[0]

    def assert_reported(self, fragment: str) -> None:
        errors = self.errors()
        self.assertTrue(any(fragment in error for error in errors), f"{fragment!r} not in {errors}")

    def test_the_repository_passes(self) -> None:
        self.assertEqual(checker.check(ROOT)[0], [])

    def test_a_floating_tag_is_refused(self) -> None:
        self.write("frontend/Dockerfile", "FROM node:22-alpine AS build\nFROM build AS runtime\n")
        self.assert_reported("frontend/Dockerfile: FROM node:22-alpine is not pinned by digest")

    def test_a_digest_argument_must_be_a_sha256(self) -> None:
        self.write(
            "frontend/Dockerfile",
            "ARG NODE_DIGEST=latest\nFROM node:22-alpine@${NODE_DIGEST} AS build\n",
        )
        self.assert_reported("ARG NODE_DIGEST must default to sha256:<64 hex>")
        self.write("frontend/Dockerfile", "FROM node:22-alpine@sha256:1234 AS build\n")
        self.assert_reported("the digest must be sha256:<64 hex>")

    def test_stages_and_literal_digests_are_accepted(self) -> None:
        self.write(
            "frontend/Dockerfile",
            f"ARG NODE_DIGEST={DIGEST_A}\nARG NGINX_DIGEST={DIGEST_A}\n"
            "FROM node:22-alpine@${NODE_DIGEST} AS build\n"
            f"FROM nginx:1.31-alpine@{DIGEST_A} AS runtime\nFROM runtime AS extra\n",
        )
        self.assertEqual(
            [e for e in self.errors() if e.startswith("frontend/Dockerfile")], []
        )

    def test_shared_arguments_must_agree(self) -> None:
        text = (self.tmp / "frontend/Dockerfile").read_text(encoding="utf-8")
        self.write("frontend/Dockerfile", re.sub(r"ARG NGINX_DIGEST=\S+", f"ARG NGINX_DIGEST={DIGEST_B}", text))
        self.assert_reported("ARG NGINX_DIGEST differs between Dockerfiles")

    def test_compose_images_built_elsewhere_are_pinned(self) -> None:
        text = (self.tmp / checker.COMPOSE).read_text(encoding="utf-8")
        self.write(checker.COMPOSE, text.replace("pgvector/pgvector:pg18@", "pgvector/pgvector:pg18#", 1))
        self.assert_reported("image pgvector/pgvector:pg18 is not pinned")
        self.write(checker.COMPOSE, "services:\n  x:\n    image: postgres:18\n")
        self.assert_reported("image postgres:18 is not pinned")

    def set_arg(self, name: str, key: str, value: str) -> None:
        text = (self.tmp / name).read_text(encoding="utf-8")
        self.write(name, re.sub(rf"ARG {key}=\S+", f"ARG {key}={value}", text))

    def test_versions_with_known_cves_are_refused(self) -> None:
        self.set_arg("frontend/Dockerfile", "NODE_VERSION", "22.23.3")
        self.assert_reported("ARG NODE_VERSION=22.23.3 has known CVEs, use 22.23.4 or later")
        self.set_arg("frontend/Dockerfile", "NGINX_VERSION", "1.31.6")
        self.assert_reported("ARG NGINX_VERSION=1.31.6 has known CVEs, use 1.31.7 or later")
        self.set_arg("frontend/Dockerfile", "NGINX_VERSION", "1.30.5")
        self.assert_reported("ARG NGINX_VERSION=1.30.5 has known CVEs, use 1.30.6 or later")

    def test_floating_or_unsupported_versions_are_refused(self) -> None:
        self.set_arg("frontend/Dockerfile", "NODE_VERSION", "22")
        self.assert_reported("ARG NODE_VERSION=22 must be an exact x.y.z release")
        self.set_arg("frontend/Dockerfile", "NODE_VERSION", "20.19.9")
        self.assert_reported("ARG NODE_VERSION=20.19.9: line 20 is not supported")
        self.set_arg("frontend/Dockerfile", "NGINX_VERSION", "1.29.9")
        self.assert_reported("ARG NGINX_VERSION=1.29.9: line 1.29 is not supported")

    def test_fixed_versions_are_accepted(self) -> None:
        for key, value in (("NODE_VERSION", "24.22.1"), ("NODE_VERSION", "26.12.1"), ("NGINX_VERSION", "1.30.6")):
            with self.subTest(value=value):
                self.assertEqual(checker.check_minimum_versions("x", {key: value}), [])

    def test_pending_dockerfiles_are_reported_without_failing(self) -> None:
        for name in checker.PENDING:
            with self.subTest(name=name):
                self.write(name, "FROM debian:trixie-slim AS runtime\n")
                errors, notes = checker.check(self.tmp)
                self.assertFalse([e for e in errors if e.startswith(name)])
                self.assertIn(f"{name}: FROM debian:trixie-slim is not pinned by digest", notes)
        self.write("frontend/Dockerfile", "FROM debian:trixie-slim AS runtime\n")
        self.assert_reported("frontend/Dockerfile: FROM debian:trixie-slim is not pinned by digest")


if __name__ == "__main__":
    unittest.main()
