#!/usr/bin/env python3
"""Seeds the local stack for the security checks (devops/dev/security/run.sh), LOCAL ONLY.

Python standard library only; reuses the client of devops/dev/e2e_scenario.py (ALTCHA solved
programmatically, tokens only in POST bodies). Against the api of devops/dev/e2e.sh with the
mock Mistral (devops/dev/mock_mistral.py), it publishes:

  - three ordinary ideas (the scanners need real slugs and pages);
  - one "hostile" idea carrying the marker `sondexss`: the mock Mistral then echoes XSS,
    JSON-LD breakout and CSV formula payloads into every free text of the analysis, the
    contribution summaries and the sources (http(s) URLs carrying payloads, cited by the web
    research so that they pass the allowed-sources check, INJ-3);
  - a contribution, a report (payloads in `details` and `reporter_name`) and an admin right of
    reply (payload in `body`) on that idea;
  - one idea whose visitor text itself carries HTML (published or refused by the decision
    model: both are recorded);
  - a second hostile idea, reported as `contenu_problematique`: the mock review hides and
    escalates it, so that its sealed copy (AGENTS.md §7.5, D3), full of the payloads of its
    analysis, can be rendered by the admin pages of xss_check.mjs.

Then it waits for the exports and checks the CSV round trip: every cell that starts with
`=`, `+`, `-`, `@`, TAB or CR is prefixed with `'` (AGENTS.md §14.1).

Writes the slugs and outcomes as JSON to `--out` (read by probes.py and xss_check.mjs).

  python devops/dev/security/seed.py --public http://127.0.0.1:18080 \
      --admin http://127.0.0.1:18081 --admin-token <ADMIN_API_TOKEN> --out seed.json
"""

from __future__ import annotations

import argparse
import csv
import io
import json
import os
import sys
import time

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
import e2e_scenario as e2e  # noqa: E402  (devops/dev/e2e_scenario.py)

HOSTILE_IDEA = (
    "Regrouper par région les contrats de maintenance des ascenseurs et des chaudières des "
    "bâtiments de l'État, aujourd'hui passés séparément par chaque administration, pour obtenir "
    "de meilleurs prix et réduire les frais de gestion des marchés (dossier sondexss)."
)
VISITOR_HTML_IDEA = (
    "Publier en données ouvertes le détail des loyers payés par l'État pour ses bureaux afin "
    "de renégocier les baux les plus chers <img src=x onerror=\"window.__xss=10\"> et de "
    "libérer les surfaces inutilisées </script><script>window.__xss=11</script>."
)
CONTRIBUTION = (
    "Il faudrait préciser que les contrats régionaux restent ouverts aux petites entreprises de "
    "maintenance locales grâce à un découpage en lots adapté (complément sondexss)."
)
ESCALATED_IDEA = (
    "Vendre aux enchères les véhicules de service inutilisés des préfectures et des directions "
    "départementales plutôt que de les laisser vieillir sur les parkings, afin de dégager des "
    "recettes et d'économiser l'assurance et l'entretien (escalade sondexss)."
)
REPORT_DETAILS = "Le chiffrage me semble surestimé <svg/onload=window.__xss=12> (sondexss)."
REPORTER_NAME = "<script>window.__xss=13</script>"
REPLY_BODY = (
    "Réponse de l'administration concernée <img src=x onerror=\"window.__xss=14\"> "
    "</script><script>window.__xss=15</script> =HYPERLINK(\"https://evil.example\")"
)
FORMULA_PREFIXES = ("=", "+", "-", "@", "\t", "\r")


class SeedApi(e2e.Api):
    """Polls the tracking route from rotating client addresses: the seed polls far more often
    than the 120 tracking requests per hour and fingerprint of the rate limit (§7.2)."""

    track_calls = 0

    def track(self, token: str) -> dict:
        self.track_calls += 1
        ip = f"198.51.100.{10 + (self.track_calls // 100) % 200}"
        status, answer = self.post("/api/track", {"token": token}, ip=ip)
        if status != 200:
            e2e.expect(False, f"track answers 200 ({status})")
        return answer


def publish(api: e2e.Api, text: str) -> tuple[str, str]:
    return e2e.submit_published(api, text)


def try_publish(api: e2e.Api, text: str) -> dict:
    """Submits a text that the decision model may refuse; returns its outcome."""
    status, answer = api.submit_idea(text)
    e2e.expect(status == 201, f"visitor-HTML idea answered 201 ({status})")
    token = answer["tracking_token"]
    item = e2e.wait_for(
        "visitor-HTML idea settled",
        lambda: (lambda t: t if t["status"] in ("published", "rejected", "analysis_failed")
                 else None)(api.track(token)),
        timeout=300,
    )
    return {"status": item["status"], "slug": (item.get("idea") or {}).get("slug"),
            "token": token}


def escalate(api: e2e.Api) -> dict:
    """Publishes a second hostile idea and has it escalated by a `contenu_problematique` report
    (no details: accepted without the decision model, and the mock review stays out of its
    hostile mode, which never escalates). Returns the escalation and its idea."""
    _, slug = publish(api, ESCALATED_IDEA)
    body = {"reason": "contenu_problematique", "good_faith": True, "altcha": api.altcha()}
    status, answer = api.post(f"/api/ideas/{slug}/reports", body, ip=api.next_ip())
    e2e.expect(status == 201 and answer["status"] != "rejected",
               f"escalation report accepted ({status}, {answer.get('status')})")
    e2e.wait_for("escalation report reviewed",
                 e2e.track_status(api, answer["tracking_token"], "reviewed"), timeout=180)

    def escalation() -> dict | None:
        status, page = api.admin_call("GET", "/escalations?status=pending&per_page=50")
        if status != 200:
            return None
        return next((e for e in page["items"] if e["idea"]["slug"] == slug), None)

    item = e2e.wait_for("hostile idea escalated", escalation, timeout=60)
    status, _ = api.admin_call("GET", f"/escalations/{item['id']}/seal")
    e2e.expect(status == 200, f"sealed copy of the escalated hostile idea ({status})")
    return {"escalation_id": item["id"], "slug": slug}


def check_csv(api: e2e.Api, hostile_slug: str) -> dict:
    def ready() -> bytes | None:
        status, _, raw = api.request("GET", api.public + "/api/exports/ideas.csv")
        return raw if status == 200 and hostile_slug.encode() in raw else None

    raw = e2e.wait_for("CSV export contains the hostile idea", ready, timeout=420, interval=10)
    e2e.expect(raw.startswith(b"\xef\xbb\xbf"), "CSV starts with a UTF-8 BOM")
    rows = list(csv.reader(io.StringIO(raw[3:].decode("utf-8"), newline="")))
    cells = [cell for row in rows for cell in row]
    dangerous = [c for c in cells if c.startswith(FORMULA_PREFIXES)]
    neutralised = [c for c in cells if c.startswith("'") and c[1:].startswith(FORMULA_PREFIXES)]
    e2e.expect(not dangerous, f"no CSV cell starts with a formula character ({dangerous[:3]})")
    e2e.expect(any(c.startswith("'=HYPERLINK") for c in neutralised),
               "the AI title =HYPERLINK(...) is exported as '=HYPERLINK(...)")
    e2e.expect(any(c.startswith("'+SUM") for c in neutralised),
               "the AI summary +SUM(...) is exported as '+SUM(...)")
    status, _, raw_json = api.request("GET", api.public + "/api/exports/ideas.json")
    data = json.loads(raw_json)
    titles = [i["analysis"]["title"] for i in data["ideas"]]
    e2e.expect(any(t.startswith("=HYPERLINK") for t in titles),
               "the JSON export keeps the title as data (no prefix needed in JSON)")
    return {"csv_cells": len(cells), "neutralised_cells": len(neutralised)}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--public", default="http://127.0.0.1:18080")
    parser.add_argument("--admin", default="http://127.0.0.1:18081")
    parser.add_argument("--admin-token", required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    api = SeedApi(args.public, args.admin, args.admin_token)
    api.ip_counter = 100
    started = time.monotonic()

    ordinary = [publish(api, text)[1] for text in (e2e.LEGIT_IDEA, e2e.OTHER_IDEA,
                                                    e2e.BUDGET_IDEA)]
    hostile_token, hostile_slug = publish(api, HOSTILE_IDEA)
    status, detail = api.get(f"/api/ideas/{hostile_slug}")
    e2e.expect(status == 200 and detail["title"].startswith("=HYPERLINK")
               and "<script>" in detail["summary"],
               "hostile analysis published as data (payloads stored verbatim)")
    e2e.expect(any("evil.example" in s["url"] for s in detail["analysis"]["sources"]),
               "hostile http(s) source URLs accepted (cited by the research)")

    # Contribution, report and right of reply on the hostile idea.
    body = {"kind": "amelioration", "text": CONTRIBUTION, "accept_licence": True,
            "accept_publication": True, "altcha": api.altcha()}
    status, answer = api.post(f"/api/ideas/{hostile_slug}/contributions", body,
                              ip=api.next_ip())
    contribution = {"status": status, "outcome": answer.get("status")}
    if status == 201 and answer.get("status") != "rejected":
        e2e.wait_for("contribution integrated",
                     e2e.track_status(api, answer["tracking_token"], "integrated", "rejected"),
                     timeout=300)
        contribution["outcome"] = api.track(answer["tracking_token"])["status"]
    body = {"reason": "erreur_chiffrage", "details": REPORT_DETAILS, "good_faith": True,
            "reporter_name": REPORTER_NAME, "reporter_email": "signalant@example.org",
            "altcha": api.altcha()}
    status, answer = api.post(f"/api/ideas/{hostile_slug}/reports", body, ip=api.next_ip())
    report = {"status": status, "outcome": answer.get("status")}
    status, page = api.admin_call("GET", f"/ideas?q={hostile_slug}")
    e2e.expect(status == 200 and page["total"] >= 1, "admin finds the hostile idea")
    idea_id = page["items"][0]["id"]
    status, _ = api.admin_call("POST", f"/ideas/{idea_id}/replies", {
        "author_label": "Ministère <b>test</b>", "body": REPLY_BODY,
        "requested_on": time.strftime("%Y-%m-%d"),
    })
    e2e.expect(status == 201, f"admin right of reply with payloads ({status})")

    visitor_html = try_publish(api, VISITOR_HTML_IDEA)
    print(f"      visitor-HTML idea: {visitor_html['status']}")
    escalation = escalate(api)

    # The contribution re-analysis may still be running: wait until the hostile idea is
    # published again before checking the exports.
    e2e.wait_for("hostile idea published after re-analysis",
                 lambda: api.get(f"/api/ideas/{hostile_slug}")[0] == 200, timeout=300)
    csv_result = check_csv(api, hostile_slug)

    out = {
        "ordinary_slugs": ordinary,
        "hostile_slug": hostile_slug,
        "hostile_tracking_token": hostile_token,
        "contribution": contribution,
        "report": report,
        "visitor_html_idea": visitor_html,
        "escalation": escalation,
        "csv": csv_result,
        "seconds": round(time.monotonic() - started),
    }
    with open(args.out, "w", encoding="utf-8") as handle:
        json.dump(out, handle, ensure_ascii=False, indent=2)
    print(f"{len(e2e.CHECKS)} seed checks passed")


if __name__ == "__main__":
    sys.exit(main())
