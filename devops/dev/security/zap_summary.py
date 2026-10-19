#!/usr/bin/env python3
"""Triage of the OWASP ZAP reports of devops/dev/security/zap.sh.

Reads the traditional JSON reports, applies zap-rules.tsv (IGNORE/INFO rules are false
positives or intended behaviours, each with its justification) and writes a Markdown summary.
Exit code 1 when an alert of risk Low or above comes from a rule that is not triaged.

  python -I zap_summary.py zap-rules.tsv SUMMARY_MD REPORT_JSON...
"""

from __future__ import annotations

import json
import os
import sys

RISKS = {"0": "Informational", "1": "Low", "2": "Medium", "3": "High"}


def load_rules(path: str) -> dict[str, tuple[str, str]]:
    rules = {}
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            if line.startswith("#") or not line.strip():
                continue
            rule, action, reason = line.rstrip("\n").split("\t", 2)
            rules[rule] = (action, reason)
    return rules


def main() -> int:
    rules_path, summary_path, *reports = sys.argv[1:]
    rules = load_rules(rules_path)
    lines = ["# Résultats ZAP (pile locale)", ""]
    untriaged = []
    for report in reports:
        name = os.path.basename(report).removesuffix(".json")
        with open(report, encoding="utf-8") as handle:
            data = json.load(handle)
        alerts = [a for site in data.get("site", []) for a in site.get("alerts", [])]
        alerts.sort(key=lambda a: (-int(a["riskcode"]), a["pluginid"]))
        lines += [f"## {name}", "", "| Règle | Alerte | Risque | Occurrences | Triage |",
                  "|---|---|---|---|---|"]
        if not alerts:
            lines.append("| — | aucune alerte | — | 0 | — |")
        for alert in alerts:
            rule = alert["pluginid"]
            risk = RISKS.get(alert["riskcode"], alert["riskcode"])
            count = len(alert.get("instances", [])) or int(alert.get("count", 0))
            if rule in rules:
                triage = f"{rules[rule][0]} : {rules[rule][1]}"
            elif alert["riskcode"] == "0":
                triage = "informatif"
            else:
                triage = "**NON TRIÉE**"
                urls = ", ".join(sorted({i["method"] + " " + i["uri"]
                                         for i in alert.get("instances", [])})[:5])
                untriaged.append(f"{name}: [{rule}] {alert['name']} ({risk}) {urls}")
            lines.append(f"| {rule} | {alert['name']} | {risk} | {count} | {triage} |")
        lines.append("")
    with open(summary_path, "w", encoding="utf-8") as handle:
        handle.write("\n".join(lines))
    print("\n".join(lines))
    if untriaged:
        print("FAIL  untriaged ZAP alerts:\n  " + "\n  ".join(untriaged), file=sys.stderr)
        return 1
    print("ok    every ZAP alert of risk Low or above is triaged")
    return 0


if __name__ == "__main__":
    sys.exit(main())
