#!/usr/bin/env python3
"""Base images pinned by digest (AGENTS.md §14.7, finding SUP-1).

Run in CI by devops/Dockerfile.test (stage `devops-check`) and locally with
`python3 devops/check_image_pins.py [ROOT]` (standard library only). Exits 1 and lists every
problem:

- every `FROM` of the Dockerfiles below names a previous stage, or an image pinned with
  `@${<NAME>_DIGEST}` (an `ARG` whose default is `sha256:` + 64 hex digits) or `@sha256:…`;
- an `ARG` declared in several Dockerfiles (RUST_VERSION, RUST_DIGEST, NGINX_DIGEST…) has the
  same default everywhere, so the CI checks the image that ships;
- every `image:` of devops/docker-compose.swarm.yml that is not built here
  (registry.methodinfo.fr) is pinned with `@sha256:`;
- the version ARGs of MINIMUM_VERSIONS name an exact `x.y.z` release of a supported line, at
  least the first release that fixes the known CVEs of that line (a digest freezes a release:
  without this floor, a vulnerable one could stay pinned).

A Dockerfile listed in PENDING is reported but does not fail the check yet: remove it from the
list once it is pinned.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

DOCKERFILES = ("backend/Dockerfile", "frontend/Dockerfile", "devops/Dockerfile.test")
# Dockerfiles not pinned yet (reported without failing). Empty since backend/Dockerfile was
# pinned (IMG-1): every Dockerfile is checked strictly.
PENDING: set[str] = set()
COMPOSE = "devops/docker-compose.swarm.yml"
OWN_REGISTRY = "registry.methodinfo.fr/"
DIGEST = re.compile(r"sha256:[0-9a-f]{64}")
ARG_LINE = re.compile(r"^\s*ARG\s+([A-Za-z_][A-Za-z0-9_]*)(?:=(\S*))?\s*$")
FROM_LINE = re.compile(r"^\s*FROM\s+(?:--platform=\S+\s+)?(\S+)(?:\s+AS\s+(\S+))?\s*$", re.IGNORECASE)
# Oldest acceptable release per supported line (major for Node.js, major.minor for nginx); a line
# missing here (end of life, or not reviewed yet) is refused. Raise a floor with each security
# release, in the same commit as the new version and digest (AGENTS.md §14.7).
MINIMUM_VERSIONS: dict[str, dict[str, tuple[int, int, int]]] = {
    # Node.js security release of 2026-10-21 (CVE-2026-111497 Brotli, CVE-2026-105483,
    # CVE-2026-106069, CVE-2026-106072 TLS hostname, CVE-2026-105481 keep-alive, CVE-2026-106074,
    # CVE-2026-106071, CVE-2026-106070). Releases of 2026-10-20 (UTC) on nodejs.org.
    "NODE_VERSION": {"22": (22, 23, 4), "24": (24, 22, 1), "26": (26, 12, 1)},
    # nginx batch of 2026-10-14 (CVE-2026-15176, CVE-2026-93479, CVE-2026-97205, CVE-2026-95101,
    # CVE-2026-97629): fixed in 1.30.6 (stable) and 1.31.7 (mainline).
    "NGINX_VERSION": {"1.30": (1, 30, 6), "1.31": (1, 31, 7)},
}
EXACT_VERSION = re.compile(r"(\d+)\.(\d+)\.(\d+)")
IMAGE_LINE = re.compile(r"^\s*image:\s*[\"']?([^\"'\s#]+)")


def logical_lines(text: str) -> list[str]:
    """Dockerfile lines with backslash continuations joined and comments dropped."""
    lines, current = [], ""
    for raw in text.splitlines():
        if raw.lstrip().startswith("#") and not current:
            continue
        if raw.rstrip().endswith("\\"):
            current += raw.rstrip()[:-1] + " "
            continue
        lines.append(current + raw)
        current = ""
    if current:
        lines.append(current)
    return lines


def check_dockerfile(name: str, text: str) -> tuple[list[str], dict[str, str]]:
    """Problems of one Dockerfile, and its ARG defaults."""
    problems: list[str] = []
    args: dict[str, str] = {}
    stages: set[str] = set()
    for line in logical_lines(text):
        arg = ARG_LINE.match(line)
        if arg and arg.group(2) is not None:
            args.setdefault(arg.group(1), arg.group(2))
            continue
        source = FROM_LINE.match(line)
        if not source:
            continue
        image, stage = source.group(1), source.group(2)
        if stage:
            stages.add(stage.lower())
        if image.lower() in stages or image == "scratch":
            continue
        if "@" not in image:
            problems.append(f"{name}: FROM {image} is not pinned by digest")
            continue
        pin = image.split("@", 1)[1]
        ref = re.fullmatch(r"\$\{?([A-Za-z_][A-Za-z0-9_]*)\}?", pin)
        if ref:
            value = args.get(ref.group(1))
            if value is None or not DIGEST.fullmatch(value):
                problems.append(f"{name}: FROM {image}: ARG {ref.group(1)} must default to sha256:<64 hex>")
        elif not DIGEST.fullmatch(pin):
            problems.append(f"{name}: FROM {image}: the digest must be sha256:<64 hex>")
    return problems, args


def check_minimum_versions(name: str, args: dict[str, str]) -> list[str]:
    """Version ARGs below the floor of their line, floating, or on an unsupported line."""
    problems = []
    for key, floors in MINIMUM_VERSIONS.items():
        value = args.get(key)
        if value is None:
            continue
        exact = EXACT_VERSION.fullmatch(value)
        if not exact:
            problems.append(f"{name}: ARG {key}={value} must be an exact x.y.z release")
            continue
        version = tuple(int(part) for part in exact.groups())
        line = str(version[0]) if key == "NODE_VERSION" else f"{version[0]}.{version[1]}"
        floor = floors.get(line)
        if floor is None:
            supported = ", ".join(sorted(floors))
            problems.append(f"{name}: ARG {key}={value}: line {line} is not supported ({supported})")
        elif version < floor:
            minimum = ".".join(str(part) for part in floor)
            problems.append(f"{name}: ARG {key}={value} has known CVEs, use {minimum} or later")
    return problems


def check_compose(text: str) -> list[str]:
    problems = []
    for line in text.splitlines():
        match = IMAGE_LINE.match(line)
        if not match:
            continue
        image = match.group(1)
        if image.startswith(OWN_REGISTRY):
            continue
        pin = image.split("@", 1)[1] if "@" in image else ""
        if not DIGEST.fullmatch(pin):
            problems.append(f"{COMPOSE}: image {image} is not pinned with @sha256:<64 hex>")
    return problems


def check(root: Path) -> tuple[list[str], list[str]]:
    """(blocking problems, pending notes)."""
    errors: list[str] = []
    notes: list[str] = []
    defaults: dict[str, dict[str, str]] = {}
    for name in DOCKERFILES:
        path = root / name
        if not path.exists():
            errors.append(f"{name}: missing")
            continue
        problems, args = check_dockerfile(name, path.read_text(encoding="utf-8"))
        problems.extend(check_minimum_versions(name, args))
        (notes if name in PENDING else errors).extend(problems)
        if name in PENDING and not problems:
            notes.append(f"{name}: now pinned, remove it from PENDING in devops/check_image_pins.py")
        for key, value in args.items():
            defaults.setdefault(key, {})[name] = value
    for key, values in sorted(defaults.items()):
        if len(set(values.values())) > 1:
            listed = ", ".join(f"{name}={value}" for name, value in sorted(values.items()))
            errors.append(f"ARG {key} differs between Dockerfiles: {listed}")
    compose = root / COMPOSE
    if compose.exists():
        errors.extend(check_compose(compose.read_text(encoding="utf-8")))
    else:
        errors.append(f"{COMPOSE}: missing")
    return errors, notes


def main() -> int:
    root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parents[1]
    errors, notes = check(root)
    for note in notes:
        print(f"note  {note}")
    for error in errors:
        print(f"FAIL  {error}", file=sys.stderr)
    if errors:
        print(f"{len(errors)} image pinning problem(s)", file=sys.stderr)
        return 1
    print("ok    base images pinned by digest, at or above the CVE floors")
    return 0


if __name__ == "__main__":
    sys.exit(main())
