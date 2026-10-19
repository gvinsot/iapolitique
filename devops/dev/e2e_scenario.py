#!/usr/bin/env python3
"""End-to-end scenario against a running local stack (api + worker + mock Mistral).

Python standard library only. Drives the public and admin APIs the way the frontend does
(ALTCHA solved programmatically, tracking tokens only in POST bodies) and checks every step of
the pipeline (AGENTS.md §5 to §8). See devops/dev/README.md for how to start the stack.

  python devops/dev/e2e_scenario.py --public http://127.0.0.1:18080 \
      --admin http://127.0.0.1:18081 --admin-token <ADMIN_API_TOKEN> [--secrets-out FILE]

Phases (`--phase`, default `main`):
  main    legitimate idea -> accepted -> dedup -> analysis -> published; insulting idea ->
          rejected with a statement; near duplicate -> merged; contribution -> re-analysis;
          exports downloadable with LICENSE; report `contenu_problematique` -> hidden +
          escalation; tracking and deletion; admin endpoints.
  budget  with the worker restarted on a tiny MISTRAL_DAILY_BUDGET_EUR: a new accepted idea
          leaves its Mistral job queued until the next Paris midnight, attempts untouched.

`--secrets-out` writes the tracking tokens, client IPs and text fragments used, one per line,
so that the caller can check that none of them appears in the container logs.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import sys
import time
import urllib.error
import urllib.request
from typing import Any

SECRETS: list[str] = []
CHECKS: list[str] = []

LEGIT_IDEA = (
    "Mutualiser les achats de fournitures médicales courantes entre les hôpitaux publics "
    "d'une même région, au travers d'une centrale d'achat commune, afin d'obtenir de meilleurs "
    "prix grâce à des volumes plus importants et de réduire les coûts de passation des marchés."
)
NEAR_DUPLICATE = (
    "Mutualiser les achats de fournitures médicales courantes entre les hôpitaux publics "
    "d'une même région, au travers d'une centrale d'achat commune, afin d'obtenir de meilleurs "
    "prix grâce à des volumes plus importants et de réduire les coûts de passation des marchés "
    "publics."
)
OTHER_IDEA = (
    "Remplacer progressivement les licences de logiciels de bureautique des administrations de "
    "l'État par des logiciels libres déjà éprouvés, avec un plan de formation des agents, pour "
    "réduire durablement les dépenses de licences et la dépendance aux éditeurs."
)
BUDGET_IDEA = (
    "Réduire la consommation d'énergie des bâtiments publics de l'État en généralisant la "
    "régulation automatique du chauffage et l'extinction de l'éclairage la nuit, afin de "
    "diminuer durablement les factures d'électricité et de gaz des ministères."
)
INSULTING_IDEA = (
    "Les fonctionnaires sont tous des connards et des parasites inutiles, il faut virer ces "
    "abrutis sans discussion, ces ordures coûtent bien trop cher aux contribuables honnêtes."
)
CONTRIBUTION = (
    "Il faudrait préciser que la centrale d'achat doit rester ouverte aux petites entreprises "
    "locales grâce à un allotissement adapté des marchés."
)
REPORT_DETAILS = "Cette fiche contient des accusations graves et diffamatoires envers des agents."


class Api:
    def __init__(self, public: str, admin: str, token: str) -> None:
        self.public = public.rstrip("/")
        self.admin = admin.rstrip("/")
        self.token = token
        self.ip_counter = 10

    def next_ip(self) -> str:
        self.ip_counter += 1
        ip = f"203.0.113.{self.ip_counter}"
        SECRETS.append(ip)
        return ip

    def request(self, method: str, url: str, body: Any = None, headers: dict | None = None,
                ip: str | None = None) -> tuple[int, Any, bytes]:
        data = None if body is None else json.dumps(body).encode()
        req = urllib.request.Request(url, data=data, method=method)
        if body is not None:
            req.add_header("Content-Type", "application/json")
        req.add_header("X-Forwarded-For", ip or "198.51.100.7")
        for key, value in (headers or {}).items():
            req.add_header(key, value)

        class NoRedirect(urllib.request.HTTPRedirectHandler):
            def redirect_request(self, *args: Any, **kwargs: Any) -> None:
                return None

        opener = urllib.request.build_opener(NoRedirect)
        try:
            with opener.open(req, timeout=120) as resp:
                return resp.status, resp.headers, resp.read()  # case-insensitive
        except urllib.error.HTTPError as err:
            return err.code, err.headers, err.read()

    def get(self, path: str, **kw: Any) -> tuple[int, Any]:
        status, _, raw = self.request("GET", self.public + path, **kw)
        return status, parse(raw)

    def post(self, path: str, body: Any, **kw: Any) -> tuple[int, Any]:
        status, _, raw = self.request("POST", self.public + path, body, **kw)
        return status, parse(raw)

    def admin_call(self, method: str, path: str, body: Any = None) -> tuple[int, Any]:
        status, _, raw = self.request(method, self.admin + "/api/admin" + path, body,
                                      headers={"Authorization": f"Bearer {self.token}"})
        return status, parse(raw)

    def altcha(self) -> str:
        status, challenge = self.get("/api/altcha/challenge")
        expect(status == 200, f"ALTCHA challenge served ({status})")
        for number in range(challenge["maxnumber"] + 1):
            digest = hashlib.sha256(f"{challenge['salt']}{number}".encode()).hexdigest()
            if digest == challenge["challenge"]:
                payload = {"algorithm": challenge["algorithm"], "challenge": digest,
                           "number": number, "salt": challenge["salt"],
                           "signature": challenge["signature"]}
                return base64.b64encode(json.dumps(payload).encode()).decode()
        raise SystemExit("ALTCHA challenge has no solution")

    def submit_idea(self, text: str) -> tuple[int, Any]:
        SECRETS.append(text[:60])
        body = {"text": text, "accept_licence": True, "accept_publication": True,
                "altcha": self.altcha()}
        status, answer = self.post("/api/ideas", body, ip=self.next_ip())
        if isinstance(answer, dict) and answer.get("tracking_token"):
            SECRETS.append(answer["tracking_token"])
        return status, answer

    def track(self, token: str) -> Any:
        status, answer = self.post("/api/track", {"token": token})
        expect(status == 200, f"track answers 200 ({status})")
        return answer


def parse(raw: bytes) -> Any:
    try:
        return json.loads(raw)
    except (ValueError, UnicodeDecodeError):
        return raw


def expect(condition: bool, label: str) -> None:
    if not condition:
        print(f"FAIL  {label}", flush=True)
        raise SystemExit(1)
    CHECKS.append(label)
    print(f"ok    {label}", flush=True)


def wait_for(label: str, probe: Any, timeout: float = 240, interval: float = 2) -> Any:
    deadline = time.monotonic() + timeout
    last = None
    while time.monotonic() < deadline:
        last = probe()
        if last:
            expect(True, label)
            return last
        time.sleep(interval)
    print(f"FAIL  {label} (timeout, last value: {last!r})", flush=True)
    raise SystemExit(1)


def track_status(api: Api, token: str, *statuses: str) -> Any:
    def probe() -> Any:
        item = api.track(token)
        return item if item["status"] in statuses else None
    return probe


def submit_published(api: Api, text: str) -> tuple[str, str]:
    status, answer = api.submit_idea(text)
    expect(status == 201, f"idea submitted (201, got {status})")
    expect(answer["status"] in ("accepted", "second_opinion", "submitted"),
           f"idea not rejected ({answer['status']}, {answer.get('reason_code')})")
    token = answer["tracking_token"]
    item = wait_for("idea published", track_status(api, token, "published"), timeout=300)
    return token, item["idea"]["slug"]


def main_phase(api: Api) -> None:
    status, body = api.get("/api/health")
    expect(status == 200, "api health")
    status, body = api.admin_call("GET", "/session")
    expect(status == 200 and body == {"authenticated": True}, "admin session with token")
    status, _, _ = api.request("GET", api.admin + "/api/admin/session")
    expect(status == 401, "admin refused without token")
    status, _, _ = api.request("GET", api.public + "/api/admin/session")
    expect(status == 404, "admin routes absent from the public port")

    # 1. Legitimate idea through the whole pipeline.
    token_a, slug_a = submit_published(api, LEGIT_IDEA)
    status, ideas = api.get("/api/ideas")
    expect(status == 200 and any(i["slug"] == slug_a for i in ideas["items"]),
           "published idea listed by GET /api/ideas")
    status, detail = api.get(f"/api/ideas/{slug_a}")
    expect(status == 200 and detail["ai"]["ai_generated"] is True
           and detail["ai"]["web_search_count"] == 2
           and len(detail["analysis"]["sources"]) == 2, "idea detail with AI marking and sources")
    expect(detail["original"]["personal_data_masked"] in (True, False)
           and detail["score"]["label"] == "score calculé par algorithme", "detail score label")

    # 2. Insulting idea: refused with a statement of reasons.
    status, answer = api.submit_idea(INSULTING_IDEA)
    expect(status == 201, "insulting idea answered 201")
    if answer["status"] in ("second_opinion", "submitted"):
        answer = wait_for("insulting idea settled",
                          track_status(api, answer["tracking_token"], "rejected"), timeout=180)
        statement = answer["decision"]
    else:
        statement = answer["statement"]
    expect(answer["status"] == "rejected", f"insulting idea rejected ({answer['status']})")
    expect(statement is not None and statement["measure"] == "rejected"
           and statement["automated"] is True and statement["rule"]["url"].startswith("/regles")
           and statement["purge_on"] is not None, "statement of reasons complete")
    token_rejected = answer.get("tracking_token") or ""

    # 3. Near duplicate: merged into the published idea.
    status, answer = api.submit_idea(NEAR_DUPLICATE)
    expect(status == 201 and answer["status"] != "rejected",
           f"near duplicate accepted ({answer['status']})")
    merged = wait_for("near duplicate merged",
                      track_status(api, answer["tracking_token"], "merged"), timeout=180)
    expect(merged["idea"]["slug"] == slug_a, "merged tracking points to the parent idea")
    wait_for("parent counts two proposals",
             lambda: api.get(f"/api/ideas/{slug_a}")[1]["proposal_count"] == 2, timeout=60)

    # 4. Contribution: accepted then integrated by a re-analysis.
    body = {"kind": "amelioration", "text": CONTRIBUTION, "accept_licence": True,
            "accept_publication": True, "altcha": api.altcha()}
    SECRETS.append(CONTRIBUTION[:60])
    status, answer = api.post(f"/api/ideas/{slug_a}/contributions", body, ip=api.next_ip())
    expect(status == 201 and answer["status"] != "rejected",
           f"contribution accepted ({status}, {answer.get('status')})")
    SECRETS.append(answer["tracking_token"])
    wait_for("contribution integrated",
             track_status(api, answer["tracking_token"], "integrated"), timeout=240)
    status, detail = api.get(f"/api/ideas/{slug_a}")
    expect(detail["contribution_count"] >= 1 and detail["ai"]["analysis_version"] >= 2
           and any(c["kind"] == "amelioration" for c in detail["contributions"]),
           "re-analysis integrates the contribution")

    # 5. Another published idea, deleted by its author.
    token_b, slug_b = submit_published(api, OTHER_IDEA)
    status, body = api.post("/api/track/delete", {"token": token_b})
    expect(status == 200 and body == {"status": "deleted"}, "author deletes a published idea")
    expect(api.track(token_b)["status"] == "deleted", "tracking shows the tombstone")
    status, _ = api.get(f"/api/ideas/{slug_b}")
    expect(status == 404, "deleted idea leaves the site")
    status, body = api.post("/api/track/delete", {"token": token_b})
    expect(status == 200, "deletion is idempotent")
    status, body = api.post("/api/track", {"token": "x" * 43})
    expect(status == 404 and body["error"]["code"] == "tracking_not_found", "unknown token 404")

    # 6. Human review request against the refusal, then admin confirmation.
    if token_rejected:
        status, body = api.post("/api/track/review-request",
                                {"token": token_rejected, "message": "Merci de revoir."})
        expect(status == 201 and body["status"] == "pending", "review request recorded")
        status, page = api.admin_call("GET", "/review-requests?status=pending")
        expect(status == 200 and page["total"] >= 1, "admin lists the review request")
        rid = page["items"][0]["id"]
        status, _ = api.admin_call("POST", f"/review-requests/{rid}/resolve",
                                   {"resolution": "confirmed", "note": "Refus confirmé."})
        expect(status == 200, "admin confirms the refusal")

    # 7. Exports: regenerated 2 minutes after the publications.
    def exports_ready() -> Any:
        status, _, raw = api.request("GET", api.public + "/api/exports/ideas.json")
        if status != 200:
            return None
        data = json.loads(raw)
        slugs = [i["slug"] for i in data["ideas"]]
        return data if slug_a in slugs and slug_b not in slugs else None
    data = wait_for("exports contain the published idea, not the deleted one", exports_ready,
                    timeout=400, interval=10)
    expect(data["metadata"]["licence"]["spdx"] == "etalab-2.0", "export carries the licence")
    status, headers, raw = api.request("GET", api.public + "/api/exports/LICENSE")
    expect(status == 200 and b"LICENCE OUVERTE" in raw.upper()
           and "attachment" in headers.get("Content-Disposition", ""), "LICENSE downloadable")
    status, headers, raw = api.request("GET", api.public + "/api/exports/ideas.csv")
    expect(status == 200 and raw.startswith(b"\xef\xbb\xbf"), "CSV downloadable with BOM")
    etag = headers.get("ETag")
    status, _, _ = api.request("GET", api.public + "/api/exports/ideas.csv",
                               headers={"If-None-Match": etag or ""})
    expect(status == 304, "CSV ETag answers 304")
    status, _, raw = api.request("GET", api.public + "/api/exports/metadata.json")
    expect(status == 200 and b"generated_at" in raw, "metadata.json downloadable")

    # 8. Report `contenu_problematique`: hidden at once and escalated.
    body = {"reason": "contenu_problematique", "details": REPORT_DETAILS, "good_faith": True,
            "altcha": api.altcha()}
    status, answer = api.post(f"/api/ideas/{slug_a}/reports", body, ip=api.next_ip())
    expect(status == 201 and answer["status"] != "rejected",
           f"report accepted ({status}, {answer.get('status')})")
    SECRETS.append(answer["tracking_token"])
    wait_for("report reviewed", track_status(api, answer["tracking_token"], "reviewed"),
             timeout=180)
    expect(api.track(answer["tracking_token"])["report_outcome"] == "hide",
           "report outcome visible on the tracking page")
    status, _ = api.get(f"/api/ideas/{slug_a}")
    expect(status == 404, "escalated idea hidden from the portal")
    hidden = api.track(token_a)
    expect(hidden["status"] == "hidden" and hidden["decision"]["measure"] == "hidden",
           "author sees the masking with its statement")
    status, page = api.admin_call("GET", "/escalations?status=pending")
    expect(status == 200 and page["total"] == 1, "escalation queued for the admin")
    eid = page["items"][0]["id"]
    status, _ = api.admin_call("POST", f"/escalations/{eid}/handle",
                               {"outcome": "transmitted", "note": "Transmis à PHAROS."})
    expect(status == 200, "admin closes the escalation")

    # 9. Other admin endpoints.
    status, dash = api.admin_call("GET", "/dashboard")
    expect(status == 200 and dash["ai_costs"]["today_eur"] > 0, "dashboard shows AI costs")
    for path in ("/jobs", "/ideas", "/reports", "/screenings", "/settings", "/actions",
                 "/review-requests", "/escalations"):
        status, _ = api.admin_call("GET", path)
        expect(status == 200, f"admin GET {path}")
    status, page = api.admin_call("GET", "/screenings?kind=idea")
    sid = page["items"][0]["id"]
    status, _ = api.admin_call("PUT", f"/screenings/{sid}/label", {"label": "correct"})
    expect(status == 200, "admin labels a screening")
    status, _, raw = api.request("GET", api.admin + "/api/admin/screenings/export.jsonl",
                                 headers={"Authorization": f"Bearer {api.token}"})
    expect(status == 200 and raw.strip(), "calibration export")
    status, _ = api.admin_call("PUT", "/settings/dedup-threshold", {"value": 0.97})
    expect(status == 200, "admin sets the dedup threshold")
    status, _ = api.admin_call("PUT", "/settings/dedup-threshold", {"value": None})
    expect(status == 200, "admin resets the dedup threshold")
    status, page = api.admin_call("GET", "/actions")
    expect(page["total"] >= 5, "admin actions journaled")


def budget_phase(api: Api) -> None:
    status, dash = api.admin_call("GET", "/dashboard")
    expect(status == 200 and dash["ai_costs"]["budget_reached"] is True,
           "dashboard reports the daily budget reached")
    status, answer = api.submit_idea(BUDGET_IDEA)
    expect(status == 201 and answer["status"] != "rejected",
           f"idea accepted under budget exhaustion ({answer['status']})")
    token = answer["tracking_token"]

    def postponed() -> Any:
        status, page = api.admin_call("GET", "/jobs?kind=dedup_idea&status=queued")
        if status != 200:
            return None
        for job in page["items"]:
            if job["attempts"] == 0 and job["run_after"] > job["created_at"] and \
                    time.time() + 600 < parse_ts(job["run_after"]):
                return job
        return None
    job = wait_for("dedup job postponed to the next Paris midnight", postponed, timeout=120)
    print(f"      run_after = {job['run_after']}")
    item = api.track(token)
    expect(item["status"] == "accepted", "idea waits in `accepted` until the budget resets")


def parse_ts(value: str) -> float:
    from datetime import datetime
    return datetime.fromisoformat(value.replace("Z", "+00:00")).timestamp()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--public", default="http://127.0.0.1:18080")
    parser.add_argument("--admin", default="http://127.0.0.1:18081")
    parser.add_argument("--admin-token", required=True)
    parser.add_argument("--phase", choices=("main", "budget"), default="main")
    parser.add_argument("--secrets-out")
    args = parser.parse_args()
    api = Api(args.public, args.admin, args.admin_token)
    if args.phase == "budget":
        api.ip_counter = 200
    try:
        (main_phase if args.phase == "main" else budget_phase)(api)
    finally:
        if args.secrets_out:
            with open(args.secrets_out, "a", encoding="utf-8") as out:
                out.write("\n".join(SECRETS) + "\n")
    print(f"{len(CHECKS)} checks passed")


if __name__ == "__main__":
    sys.exit(main())
