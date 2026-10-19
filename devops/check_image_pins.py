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
  (registry.methodinfo.fr) is pinned with `@sha256:`.

A Dockerfile listed in PENDING is reported but does not fail the check yet: remove it from the
list once it is pinned.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

DOCKERFILES = ("backend/Dockerfile", "frontend/Dockerfile", "devops/Dockerfile.test")
# backend/Dockerfile (rust, debian) is owned by the backend: pinning it is tracked by SUP-1/IMG-1.
PENDING = {"backend/Dockerfile"}
COMPOSE = "devops/docker-compose.swarm.yml"
OWN_REGISTRY = "registry.methodinfo.fr/"
DIGEST = re.compile(r"sha256:[0-9a-f]{64}")
ARG_LINE = re.compile(r"^\s*ARG\s+([A-Za-z_][A-Za-z0-9_]*)(?:=(\S*))?\s*$")
FROM_LINE = re.compile(r"^\s*FROM\s+(?:--platform=\S+\s+)?(\S+)(?:\s+AS\s+(\S+))?\s*$", re.IGNORECASE)
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
    print("ok    base images pinned by digest")
    return 0


if __name__ == "__main__":
    sys.exit(main())
