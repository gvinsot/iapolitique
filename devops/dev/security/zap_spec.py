#!/usr/bin/env python3
"""Builds the OpenAPI document of the targeted ZAP active scan (devops/dev/security/zap.sh).

From the committed spec (frontend/src/api/openapi.json) it keeps the public routes only (the
admin API is never routed on the public host) and gives every path and query parameter a real
value from the seeded stack (seed.json written by seed.py): the slug of the hostile idea, valid
sort and filter values. ZAP's OpenAPI importer uses these `example` values, so the active rules
attack parameters that reach the database instead of 404 answers.

  python -I zap_spec.py SPEC_IN SEED_JSON SPEC_OUT
"""

from __future__ import annotations

import json
import sys


def main() -> int:
    spec_in, seed_in, spec_out = sys.argv[1:4]
    with open(spec_in, encoding="utf-8") as handle:
        spec = json.load(handle)
    with open(seed_in, encoding="utf-8") as handle:
        seed = json.load(handle)
    examples = {
        "slug": seed["hostile_slug"],
        "sort": "score",
        "admin_level": "etat",
        "category": "achats_publics",
        "kind": "savings",
        "territory": "Paris",
        "q": "achats",
        "page": 1,
        "per_page": 20,
    }
    spec["paths"] = {path: item for path, item in spec["paths"].items()
                     if not path.startswith("/api/admin")}
    for item in spec["paths"].values():
        for operation in item.values():
            if not isinstance(operation, dict):
                continue
            for parameter in operation.get("parameters", []):
                if parameter.get("name") in examples:
                    parameter["example"] = examples[parameter["name"]]
                    parameter.setdefault("schema", {})["default"] = examples[parameter["name"]]
    spec["servers"] = [{"url": "https://localhost"}]
    with open(spec_out, "w", encoding="utf-8") as handle:
        json.dump(spec, handle, ensure_ascii=False, indent=1)
    print(f"{len(spec['paths'])} public paths with seeded examples -> {spec_out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
