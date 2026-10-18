#!/usr/bin/env python3
"""Check every file of prompts/ against the contracts of AGENTS.md §13.4.

Run from anywhere: python docs/calibration/check_prompts.py
Requires PyYAML and jsonschema (development tools only, not used by the application):
    python -m pip install pyyaml jsonschema
Exits with status 1 and lists every problem found.
"""

from __future__ import annotations

import json
import pathlib
import re
import sys

try:
    import jsonschema
    import yaml
except ImportError as e:  # pragma: no cover - environment check
    sys.exit(f"missing dependency: {e.name} (python -m pip install pyyaml jsonschema)")

ROOT = pathlib.Path(__file__).resolve().parents[2]
PROMPTS = ROOT / "prompts"
SEMVER = re.compile(r"^\d+\.\d+\.\d+$")
VARIABLE = re.compile(r"\{\{\s*([a-z_][a-z0-9_]*)\s*\}\}")
USER_MARKER = "<!-- user -->"
# Keywords known to be accepted by Mistral `response_format` json_schema in strict mode.
STRICT_KEYWORDS = {"type", "properties", "required", "additionalProperties", "items", "enum", "description", "title"}
PROMPT_FILES = {
    "analysis_research.md": None,
    "analysis.md": "analysis.schema.json",
    "second_opinion.md": "second_opinion.schema.json",
    "report_review.md": "report_review.schema.json",
}

errors: list[str] = []


def err(where: str, msg: str) -> None:
    errors.append(f"{where}: {msg}")


def load_yaml(name: str):
    try:
        return yaml.safe_load((PROMPTS / name).read_text(encoding="utf-8"))
    except Exception as e:
        err(name, f"invalid YAML: {e}")
        return None


def check_taxonomy() -> dict | None:
    tax = load_yaml("taxonomy.yaml")
    if tax is None:
        return None
    if not SEMVER.match(str(tax.get("version", ""))):
        err("taxonomy.yaml", "version must be a quoted semver string")
    lists = ("admin_levels", "impact_publics", "impact_effects", "idea_kinds", "confidence_levels",
             "legal_vehicles", "report_reasons", "contribution_kinds")
    for name in lists:
        items = tax.get(name)
        if not isinstance(items, list) or not items:
            err("taxonomy.yaml", f"{name} must be a non-empty list")
            continue
        seen = set()
        for it in items:
            if not isinstance(it, dict) or not it.get("key") or not it.get("label"):
                err("taxonomy.yaml", f"{name}: every item needs key and label: {it!r}")
            elif it["key"] in seen:
                err("taxonomy.yaml", f"{name}: duplicate key {it['key']}")
            else:
                seen.add(it["key"])
    expected = {
        "admin_levels": ["etat", "securite_sociale", "region", "departement", "intercommunalite", "commune", "multi"],
        "impact_effects": ["positive", "neutral", "negative", "uncertain"],
        "idea_kinds": ["savings", "revenue", "mixed"],
        "confidence_levels": ["low", "medium", "high"],
        "legal_vehicles": ["loi", "loi_de_finances", "decret", "arrete", "deliberation", "circulaire", "organisation_interne", "autre"],
        "report_reasons": ["erreur_chiffrage", "source_erronee", "contenu_problematique", "doublon", "autre"],
        "contribution_kinds": ["amelioration", "objection"],
    }
    for name, keys in expected.items():
        got = [it.get("key") for it in tax.get(name) or []]
        if got != keys:
            err("taxonomy.yaml", f"{name} keys {got} differ from the contract {keys}")
    cats = tax.get("categories")
    if not isinstance(cats, dict) or not cats:
        err("taxonomy.yaml", "categories must be a map group -> list")
        return tax
    all_keys: list[str] = []
    for group, items in cats.items():
        for it in items or []:
            if not it.get("key") or not it.get("label"):
                err("taxonomy.yaml", f"categories.{group}: item without key/label")
            all_keys.append(it.get("key"))
    dup = {k for k in all_keys if all_keys.count(k) > 1}
    if dup:
        err("taxonomy.yaml", f"category keys must be unique across groups: {sorted(dup)}")
    for level in tax.get("admin_levels") or []:
        groups = level.get("category_groups")
        if not groups or set(groups) - set(cats):
            err("taxonomy.yaml", f"admin level {level.get('key')}: unknown or empty category_groups {groups}")
    return tax


def check_screening() -> dict | None:
    grid = load_yaml("screening.yaml")
    if grid is None:
        return None
    if not SEMVER.match(str(grid.get("version", ""))):
        err("screening.yaml", "version must be a quoted semver string")
    # AGENTS.md §5.3: the refusal reason is the first failing criterion in this order.
    order = {"idea": ["injection", "respectful", "targeted", "personal_data", "on_topic", "lawful", "realism"],
             "contribution": ["injection", "respectful", "targeted", "personal_data", "relevant"],
             "report": ["injection", "respectful", "relevant"]}
    for section, keys in order.items():
        crits = grid.get(section)
        if not isinstance(crits, list):
            err("screening.yaml", f"section {section} must be a list")
            continue
        if [c.get("key") for c in crits] != keys:
            err("screening.yaml", f"{section}: criteria order must be {keys}")
        for c in crits:
            where = f"screening.yaml {section}.{c.get('key')}"
            for field in ("key", "type", "instructions", "pass_when", "reject_below", "accept_above", "reason_code", "reason_message"):
                if field not in c:
                    err(where, f"missing {field}")
            if c.get("type") == "noul":
                if not isinstance(c.get("pass_when"), bool):
                    err(where, "pass_when must be a boolean for noul")
                crit = c.get("criteria")
                if crit is not None and set(crit) != {"true", "false"}:
                    err(where, 'noul criteria keys must be the strings "true" and "false"')
            elif c.get("type") == "score":
                levels = c.get("criteria")
                if not isinstance(levels, list) or not 2 <= len(levels) <= 4:
                    err(where, "score criteria must list 2 to 4 levels (coarse scales only)")
                elif not isinstance(c.get("pass_when"), int) or not 0 < c["pass_when"] < len(levels):
                    err(where, "pass_when must be a level index between 1 and len(criteria) - 1")
            else:
                err(where, f"unsupported type {c.get('type')!r}")
            rb, aa = c.get("reject_below"), c.get("accept_above")
            if not all(isinstance(x, (int, float)) for x in (rb, aa)) or not 0 <= rb <= aa <= 1:
                err(where, "need 0 <= reject_below <= accept_above <= 1")
    return grid


def walk_schema(schema, where: str) -> None:
    if not isinstance(schema, dict):
        err(where, "schema node must be an object")
        return
    unknown = set(schema) - STRICT_KEYWORDS
    if unknown:
        err(where, f"keywords not allowed in Mistral strict mode: {sorted(unknown)}")
    if not isinstance(schema.get("type"), str):
        err(where, "every node needs a single string `type`")
    if schema.get("type") == "object":
        props = schema.get("properties", {})
        if schema.get("additionalProperties") is not False:
            err(where, "objects need additionalProperties: false")
        if sorted(schema.get("required", [])) != sorted(props):
            err(where, "strict mode: every property must be required")
        for k, v in props.items():
            walk_schema(v, f"{where}.{k}")
    if schema.get("type") == "array":
        walk_schema(schema.get("items"), f"{where}[]")


def check_schemas(tax: dict | None, grid: dict | None) -> dict:
    schemas = {}
    for name in ("analysis.schema.json", "second_opinion.schema.json", "report_review.schema.json"):
        try:
            schema = json.loads((PROMPTS / name).read_text(encoding="utf-8"))
            jsonschema.Draft202012Validator.check_schema(schema)
        except Exception as e:
            err(name, f"invalid JSON Schema: {e}")
            continue
        walk_schema(schema, name)
        schemas[name] = schema
    a = schemas.get("analysis.schema.json")
    if a and tax:
        p = a["properties"]

        def enum_of(prop):
            return p[prop]["enum"]

        checks = {
            "admin_level": [x["key"] for x in tax["admin_levels"]],
            "kind": [x["key"] for x in tax["idea_kinds"]],
            "legal_vehicle": [x["key"] for x in tax["legal_vehicles"]],
            "confidence": [x["key"] for x in tax["confidence_levels"]],
            "category": [c["key"] for g in tax["categories"].values() for c in g],
        }
        for prop, keys in checks.items():
            if enum_of(prop) != keys:
                err("analysis.schema.json", f"{prop} enum out of sync with taxonomy.yaml")
        if list(p["impacts"]["properties"]) != [x["key"] for x in tax["impact_publics"]]:
            err("analysis.schema.json", "impacts properties out of sync with taxonomy impact_publics")
        effects = [x["key"] for x in tax["impact_effects"]]
        for k, v in p["impacts"]["properties"].items():
            if v["properties"]["effect"]["enum"] != effects:
                err("analysis.schema.json", f"impacts.{k}.effect enum out of sync with taxonomy impact_effects")
        for field in ("annual_gain_eur", "implementation_cost_eur"):
            if list(p[field]["properties"])[:3] != ["low", "median", "high"]:
                err("analysis.schema.json", f"{field} must expose low, median, high")
    so = schemas.get("second_opinion.schema.json")
    if so and grid:
        codes = sorted({c["reason_code"] for s in ("idea", "contribution", "report") for c in grid.get(s) or []})
        if sorted(so["properties"]["reason_code"]["enum"]) != sorted(["none"] + codes):
            err("second_opinion.schema.json", "reason_code enum out of sync with screening.yaml reason codes")
        if so["properties"]["decision"]["enum"] != ["accept", "reject"]:
            err("second_opinion.schema.json", "decision must be accept|reject")
    rr = schemas.get("report_review.schema.json")
    if rr:
        rp = rr["properties"]
        if list(rp) != ["outcome", "escalate", "explanation", "corrections"]:
            err("report_review.schema.json", "properties must be outcome, escalate, explanation, corrections")
        if rp.get("outcome", {}).get("enum") != ["maintain", "update", "hide"]:
            err("report_review.schema.json", "outcome must be maintain|update|hide")
        if rp.get("escalate", {}).get("type") != "boolean":
            err("report_review.schema.json", "escalate must be a boolean (AGENTS.md §7.5)")
    return schemas


def check_prompt(name: str, schema_name: str | None) -> None:
    path = PROMPTS / name
    try:
        text = path.read_text(encoding="utf-8")
    except FileNotFoundError:
        err(name, "missing file")
        return
    m = re.match(r"^---\n(.*?)\n---\n(.*)$", text, re.S)
    if not m:
        err(name, "must start with a YAML front matter delimited by --- lines")
        return
    try:
        front = yaml.safe_load(m.group(1))
    except Exception as e:
        err(name, f"invalid front matter: {e}")
        return
    body = m.group(2)
    if not SEMVER.match(str(front.get("version", ""))):
        err(name, "version must be a quoted semver string")
    if not front.get("model_hint"):
        err(name, "missing model_hint")
    if not isinstance(front.get("tools"), list) or set(front["tools"]) - {"web_search"}:
        err(name, "tools must be a list (only web_search is used)")
    if front.get("output_schema") != schema_name:
        err(name, f"output_schema must be {schema_name!r}")
    declared = front.get("variables")
    if not isinstance(declared, dict) or not all(isinstance(v, str) and v for v in declared.values()):
        err(name, "variables must map each name to a description")
        declared = {}
    if body.count(USER_MARKER) != 1:
        err(name, f"body must contain exactly one {USER_MARKER!r} line")
        return
    system, user = body.split(USER_MARKER)
    if not system.strip() or not user.strip():
        err(name, "system and user parts must both be non-empty")
    in_system = set(VARIABLE.findall(system)) - {"today"}
    if in_system:
        # {{today}} is allowed in the system part: it changes once a day, which keeps prompt caching useful.
        err(name, f"variables other than today must stay in the user part: {sorted(in_system)}")
    used = set(VARIABLE.findall(body))
    if used != set(declared):
        err(name, f"declared variables {sorted(declared)} differ from used {sorted(used)}")
    if re.search(r"\{\{(?!\s*[a-z_][a-z0-9_]*\s*\}\})", body):
        err(name, "malformed {{variable}}")


def main() -> int:
    tax = check_taxonomy()
    grid = check_screening()
    check_schemas(tax, grid)
    for name, schema_name in PROMPT_FILES.items():
        check_prompt(name, schema_name)
    known = set(PROMPT_FILES) | {"taxonomy.yaml", "screening.yaml", "analysis.schema.json",
                                 "second_opinion.schema.json", "report_review.schema.json"}
    for extra in sorted(p.name for p in PROMPTS.iterdir() if p.name not in known):
        err(extra, "unexpected file in prompts/ (update AGENTS.md §13.4 and this checker)")
    if errors:
        print("\n".join(errors))
        return 1
    print(f"prompts/ OK ({len(known)} files)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
