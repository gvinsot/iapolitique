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
          escalation; tracking and deletion; withdrawal by its author of a published idea
          that carries another visitor's contribution (D2) and of the escalated idea, whose
          sealed copy stays readable by the admin (D3); admin endpoints.
  auth-refused   the mock refuses the API key (401, `--mock`): the Mistral jobs of new ideas
          wait (rescheduled 15 min later, attempts untouched), only one job per period calls
          Mistral, the dashboard shows the alert; then the key is accepted again. Writes the
          ideas to `--state`.
  auth-restored  once the retry delay has passed (e2e.sh moves the clock of the queue between
          the two phases), the waiting jobs run, the alert closes, the ideas are published and
          each job used a single attempt.
  reasoning-refused  the mock refuses `reasoning_effort` for the configured model (HTTP 400,
          `--mock`): a new idea is still published, each process sends the parameter once and
          then goes without it, the dashboard advises another model and the methodology page
          publishes the effort really sent (none). The refusal is lifted at the end.
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
import urllib.parse
import urllib.request
from typing import Any

SECRETS: list[str] = []
CHECKS: list[str] = []
# Text of the mock's `thinking` chunks (devops/dev/mock_mistral.py, reasoning_effort high, the
# analysis default): never in the API answers nor in the logs (AGENTS.md §6.1).
THINKING_MARKER = "PENSEE-PRIVEE-MOCK"

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
# About 3,900 characters: the longest kind of text the decision service sees (AGENTS.md §5.3).
LONG_IDEA = " ".join([
    "Créer un service national de gestion de la flotte automobile des administrations de l'État,",
    "chargé d'acheter, d'entretenir, de partager et de revendre les véhicules de service des",
    "ministères et de leurs opérateurs, aujourd'hui gérés séparément par chaque administration.",
    "Chaque ministère dispose de son propre parc, de ses propres contrats d'entretien et de ses",
    "propres règles d'usage, ce qui conduit à des taux d'utilisation faibles, à des véhicules",
    "immobilisés la plupart du temps et à des coûts de gestion dispersés entre de nombreux",
    "services. Le service national tiendrait un inventaire unique des véhicules, mesurerait leur",
    "utilisation réelle grâce aux carnets de bord déjà tenus, et mettrait en commun les véhicules",
    "sous-utilisés entre les administrations d'une même ville au moyen d'un outil de réservation",
    "partagé. Les véhicules dont l'usage resterait trop faible après mutualisation seraient",
    "revendus aux enchères publiques, et les déplacements correspondants seraient assurés par",
    "des véhicules partagés, des transports en commun ou des locations de courte durée lorsque",
    "cela revient moins cher. Les contrats d'entretien, d'assurance et de carburant seraient",
    "regroupés dans des marchés nationaux allotis par région pour rester accessibles aux",
    "garages et entreprises locales, avec des prix négociés sur des volumes bien plus",
    "importants qu'aujourd'hui. Le renouvellement des véhicules suivrait une règle commune",
    "fondée sur le coût total de possession, en tenant compte du kilométrage, de l'âge, des",
    "frais d'entretien et de la consommation, au lieu de calendriers fixes propres à chaque",
    "ministère. Les véhicules opérationnels des forces de sécurité, des services de secours et",
    "des armées resteraient hors du périmètre, de même que les véhicules spécialisés dont",
    "l'usage ne peut pas être partagé. La mise en place se ferait en deux temps : d'abord un",
    "inventaire complet et la mutualisation dans quelques grandes villes pilotes, puis une",
    "généralisation progressive sur trois ans, avec un bilan public annuel des économies",
    "constatées, du nombre de véhicules revendus et du taux d'utilisation moyen des véhicules",
    "conservés. Les agents chargés aujourd'hui de la gestion des parcs dans chaque ministère",
    "seraient affectés au nouveau service ou à d'autres missions, sans suppression brutale de",
    "postes, et formés aux nouveaux outils de réservation et de suivi. Les économies attendues",
    "viennent de la baisse du nombre de véhicules à acheter, assurer et entretenir, des",
    "meilleurs prix obtenus sur les contrats regroupés et des recettes de revente des véhicules",
    "inutiles. Un suivi par la direction du budget permettrait de vérifier chaque année que les",
    "crédits correspondants sont bien réduits dans les budgets des ministères concernés, et",
    "que les économies ne sont pas compensées par une hausse des frais de déplacement",
    "remboursés aux agents. Les collectivités territoriales qui le souhaitent pourraient",
    "adhérer au dispositif par convention, afin de partager les mêmes marchés et les mêmes",
    "outils, ce qui augmenterait encore les volumes négociés et la taille du parc partagé dans",
    "les villes où l'État et les collectivités disposent de nombreux bâtiments voisins.",
    "Enfin, les données d'utilisation anonymisées seraient publiées en données ouvertes afin",
    "que chacun puisse suivre l'évolution du parc et des coûts au fil des années.",
    "Les premiers résultats des villes pilotes serviraient à ajuster les règles de partage, la",
    "durée de conservation des véhicules et le découpage des lots avant la généralisation, et",
    "un comité réunissant les ministères concernés validerait chaque étape au vu de ces",
    "résultats, afin que la qualité du service rendu aux agents et aux usagers soit préservée",
    "pendant toute la transition et que les besoins particuliers des territoires ruraux, où",
    "les transports en commun sont rares, soient pris en compte dans la taille des parcs locaux.",
])
INSULTING_IDEA = (
    "Les fonctionnaires sont tous des connards et des parasites inutiles, il faut virer ces "
    "abrutis sans discussion, ces ordures coûtent bien trop cher aux contribuables honnêtes."
)
CONTRIBUTION = (
    "Il faudrait préciser que la centrale d'achat doit rester ouverte aux petites entreprises "
    "locales grâce à un allotissement adapté des marchés."
)
REPORT_DETAILS = "Cette fiche contient des accusations graves et diffamatoires envers des agents."
OTHER_CONTRIBUTION = (
    "Il faudrait prévoir une période de double usage des logiciels pendant la formation des "
    "agents, pour ne pas interrompre le service rendu aux usagers."
)
# Ideas of the `auth-*` phases: unrelated to each other and to the others (no merge).
AUTH_IDEAS = (
    "Dématérialiser entièrement les convocations et notifications adressées par les tribunaux "
    "aux justiciables qui l'acceptent, afin de réduire les frais d'affranchissement et "
    "d'impression des juridictions et d'accélérer le traitement des dossiers.",
    "Regrouper les contrats de maintenance des ascenseurs et des installations techniques des "
    "lycées d'une même académie dans un marché unique, pour obtenir des prix plus bas et "
    "réduire le temps passé par les établissements à gérer ces contrats.",
)
REASONING_IDEA = (
    "Mutualiser entre les départements d'une même région l'achat des véhicules et engins "
    "d'entretien des routes, aujourd'hui acquis séparément, afin de payer moins cher et de "
    "partager les engins rarement utilisés comme les saleuses de grande capacité."
)
# Model of the e2e stack (MISTRAL_ANALYSIS_MODEL, default; the review uses it too).
E2E_MODEL = "mistral-large-latest"
# Kinds of the jobs that call Mistral (AGENTS.md §13.1).
MISTRAL_KINDS = ("second_opinion", "dedup_idea", "analyze_idea", "review_reports")


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
    SECRETS.append(THINKING_MARKER)
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
    expect(THINKING_MARKER not in json.dumps(detail, ensure_ascii=False),
           "analysis read from the text chunks only, never from the thinking")
    status, methodology = api.get("/api/methodology")
    expect(status == 200 and methodology["models"]["analysis"]["reasoning_effort"] == "high"
           and methodology["models"]["review"]["reasoning_effort"] == "none",
           "methodology publishes the reasoning efforts in use")

    # 1b. A long idea is settled (by the synchronous call or by the screening job), never stuck.
    expect(3_500 <= len(LONG_IDEA) <= 4_000, f"long idea of {len(LONG_IDEA)} characters")
    status, answer = api.submit_idea(LONG_IDEA)
    expect(status == 201, f"long idea answered 201 ({status})")
    print(f"      synchronous outcome of the long idea: {answer['status']}")
    wait_for("long idea leaves `submitted`",
             lambda: api.track(answer["tracking_token"])["status"] != "submitted", timeout=240)

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
    merged_token = answer["tracking_token"]
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

    # 5. Another published idea, with another visitor's contribution, withdrawn by its author
    #    (D2): gone from the public site, kept for the admin, the contributor is told why.
    token_b, slug_b = submit_published(api, OTHER_IDEA)
    body = {"kind": "amelioration", "text": OTHER_CONTRIBUTION, "accept_licence": True,
            "accept_publication": True, "altcha": api.altcha()}
    SECRETS.append(OTHER_CONTRIBUTION[:60])
    status, answer = api.post(f"/api/ideas/{slug_b}/contributions", body, ip=api.next_ip())
    expect(status == 201 and answer["status"] != "rejected",
           f"contribution on the idea to withdraw accepted ({status}, {answer.get('status')})")
    token_contrib_b = answer["tracking_token"]
    SECRETS.append(token_contrib_b)
    wait_for("contribution on the idea to withdraw integrated",
             track_status(api, token_contrib_b, "integrated"), timeout=240)
    status, body = api.post("/api/track/delete", {"token": token_b})
    expect(status == 200 and body == {"status": "deleted"}, "author deletes a published idea")
    expect(api.track(token_b)["status"] == "deleted", "author's tracking shows the deletion done")
    status, _ = api.get(f"/api/ideas/{slug_b}")
    expect(status == 404, "withdrawn idea leaves the site")
    status, ideas = api.get("/api/ideas")
    expect(status == 200 and all(i["slug"] != slug_b for i in ideas["items"]),
           "withdrawn idea absent from GET /api/ideas")
    contrib = api.track(token_contrib_b)
    expect(contrib["idea_withdrawn"] is True and contrib.get("idea") is None,
           "contributor's tracking says the idea was withdrawn by its author")
    status, body = api.post(f"/api/ideas/{slug_b}/contributions",
                            {"kind": "amelioration", "text": CONTRIBUTION, "accept_licence": True,
                             "accept_publication": True, "altcha": api.altcha()},
                            ip=api.next_ip())
    expect(status == 404, f"no contribution to a withdrawn idea ({status})")
    status, page = api.admin_call("GET", f"/ideas?status=withdrawn&q={slug_b}")
    expect(status == 200 and any(i["slug"] == slug_b and i["status"] == "withdrawn"
                                 and i.get("withdrawn_at") for i in page["items"]),
           "admin still sees the withdrawn idea, marked withdrawn")
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
    token_report_a = answer["tracking_token"]
    wait_for("report reviewed", track_status(api, answer["tracking_token"], "reviewed"),
             timeout=180)
    expect(api.track(answer["tracking_token"])["report_outcome"] == "hide",
           "report outcome visible on the tracking page")
    status, _ = api.get(f"/api/ideas/{slug_a}")
    expect(status == 404, "escalated idea hidden from the portal")
    # A masking regenerates the exports right away, with the top priority (no 2-minute grouping,
    # AGENTS.md §7.4): the hidden idea leaves the downloadable files within seconds.
    def export_without_a() -> Any:
        status, _, raw = api.request("GET", api.public + "/api/exports/ideas.json")
        return status == 200 and slug_a.encode() not in raw
    wait_for("hidden escalated idea out of the exports within seconds", export_without_a,
             timeout=60, interval=1)
    hidden = api.track(token_a)
    expect(hidden["status"] == "hidden" and hidden["decision"]["measure"] == "hidden",
           "author sees the masking with its statement")
    status, page = api.admin_call("GET", "/escalations?status=pending")
    expect(status == 200 and page["total"] == 1, "escalation queued for the admin")
    eid = page["items"][0]["id"]
    status, _ = api.admin_call("POST", f"/escalations/{eid}/handle",
                               {"outcome": "transmitted", "note": "Transmis à PHAROS."})
    expect(status == 200, "admin closes the escalation")

    # 8b. The author deletes the escalated idea (D2 + D3): withdrawn, and the sealed copy of the
    #     escalation stays readable by the admin, each read journaled.
    status, body = api.post("/api/track/delete", {"token": token_a})
    expect(status == 200 and body == {"status": "deleted"}, "author deletes the escalated idea")
    expect(api.track(token_a)["status"] == "deleted", "author's tracking shows the deletion done")
    expect(api.track(token_report_a)["idea_withdrawn"] is True,
           "reporter's tracking says the idea was withdrawn by its author")
    expect(api.track(merged_token)["idea_withdrawn"] is True,
           "merged duplicate's tracking says the idea was withdrawn by its author")
    status, page = api.admin_call("GET", "/escalation-seals")
    expect(status == 200 and any(s["escalation_id"] == eid and s["idea_status"] == "withdrawn"
                                 for s in page["items"]),
           "escalation seal listed, its idea withdrawn")
    status, seal = api.admin_call("GET", f"/escalations/{eid}/seal")
    expect(status == 200 and seal["content"]["idea"]["slug"] == slug_a
           and LEGIT_IDEA[:60] in (seal["content"]["idea"]["original_text"] or "")
           and any(r["reason"] == "contenu_problematique" for r in seal["content"]["reports"])
           and seal["content"]["review"]["escalate"] is True,
           "admin reads the sealed copy of the withdrawn escalated idea")
    status, page = api.admin_call("GET", f"/actions?action=escalation.seal_read&target_id={eid}")
    expect(status == 200 and page["total"] >= 1, "sealed copy read journaled")
    # The withdrawal regenerates them right away too.
    wait_for("withdrawn escalated idea absent from the exports", export_without_a,
             timeout=60, interval=1)

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


def mistral_jobs(api: Api, idea_ids: set[str], status: str) -> list[dict]:
    """Jobs of the given ideas that call Mistral, in the given status."""
    jobs = []
    for kind in MISTRAL_KINDS:
        code, page = api.admin_call("GET", f"/jobs?kind={kind}&status={status}&per_page=50")
        if code != 200:
            expect(False, f"admin lists {status} {kind} jobs ({code})")
        jobs += [j for j in page["items"] if str(j["payload"].get("idea_id")) in idea_ids]
    return jobs


def mock_auth(mock: str, auth_status: int | None) -> None:
    req = urllib.request.Request(mock.rstrip("/") + "/control", method="POST",
                                 data=json.dumps({"auth_status": auth_status}).encode(),
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=10) as resp:
        expect(resp.status == 200, f"mock Mistral answers {auth_status or 200} to the key")


def mock_stats(mock: str) -> dict:
    with urllib.request.urlopen(mock.rstrip("/") + "/stats", timeout=10) as resp:
        return json.loads(resp.read())


def mock_refused(mock: str) -> int:
    return mock_stats(mock).get("refused_auth", 0)


def mock_reasoning(mock: str, models: list[str]) -> None:
    req = urllib.request.Request(mock.rstrip("/") + "/control", method="POST",
                                 data=json.dumps({"reject_reasoning_effort": models}).encode(),
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=10) as resp:
        expect(resp.status == 200, f"mock Mistral refuses reasoning_effort for {models}")


def reasoning_refused_phase(api: Api, mock: str) -> None:
    status, dash = api.admin_call("GET", "/dashboard")
    expect(status == 200 and dash["alerts"] == [], "no alert before the refusal")
    mock_reasoning(mock, [E2E_MODEL])
    try:
        before = mock_stats(mock)
        token, slug = submit_published(api, REASONING_IDEA)
        after = mock_stats(mock)
        refused = after.get("refused_reasoning_effort", 0) - before.get(
            "refused_reasoning_effort", 0)
        omitted = sum(after.get(f"{path} reasoning_effort=omitted", 0)
                      - before.get(f"{path} reasoning_effort=omitted", 0)
                      for path in ("/v1/chat/completions", "/v1/conversations"))
        # Once per process at most (api: inline second opinion, worker), then never again.
        expect(1 <= refused <= 2, f"the model refused reasoning_effort once per process ({refused})")
        expect(omitted >= 2, f"research and structuring sent without the parameter ({omitted})")
        status, detail = api.get(f"/api/ideas/{slug}")
        expect(status == 200 and len(detail["analysis"]["sources"]) == 2,
               "analysis complete without reasoning_effort")
        status, dash = api.admin_call("GET", "/dashboard")
        alert = next((a for a in dash.get("alerts", [])
                      if a["code"] == "mistral_reasoning_unsupported"), None)
        expect(status == 200 and alert is not None and alert["http_status"] == 400
               and E2E_MODEL in alert["message"] and "MISTRAL_ANALYSIS_MODEL" in alert["message"]
               and "mistral-large-4-0" in alert["message"],
               "dashboard advises a model that takes reasoning_effort")
        status, methodology = api.get("/api/methodology")
        analysis = methodology["models"]["analysis"]
        expect(status == 200 and analysis["reasoning_effort"] is None
               and analysis["configured_reasoning_effort"] == "high",
               "methodology publishes the effort really sent")
        status, page = api.admin_call("GET", "/ideas?q=" + urllib.parse.quote(slug))
        expect(status == 200 and page["items"][0]["status"] == "published",
               "no analysis failed on the refused parameter")
        SECRETS.append(token)
    finally:
        mock_reasoning(mock, [])


def idea_id(api: Api, text: str) -> str:
    """Admin id of an idea, found by a fragment of its text."""
    status, page = api.admin_call("GET", "/ideas?q=" + urllib.parse.quote(text[:40]))
    expect(status == 200 and len(page["items"]) == 1, "admin finds the idea by its text")
    return str(page["items"][0]["id"])


def auth_refused_phase(api: Api, mock: str, state_path: str) -> None:
    status, dash = api.admin_call("GET", "/dashboard")
    expect(status == 200 and dash["alerts"] == [], "no alert while Mistral accepts the key")
    mock_auth(mock, 401)
    refused_before = mock_refused(mock)

    # First idea: its first Mistral call is refused, the alert opens, the job waits.
    status, answer = api.submit_idea(AUTH_IDEAS[0])
    expect(status == 201 and answer["status"] != "rejected",
           f"idea accepted while Mistral refuses the key ({answer['status']})")
    tokens = [answer["tracking_token"]]

    def alert() -> Any:
        _, dash = api.admin_call("GET", "/dashboard")
        return next((a for a in dash.get("alerts", []) if a["code"] == "mistral_auth"), None)
    found = wait_for("dashboard shows the refused Mistral key", alert, timeout=120)
    expect(found["http_status"] == 401 and "Clé Mistral refusée" in found["message"],
           "alert carries the HTTP status and the French message")
    ids = {idea_id(api, AUTH_IDEAS[0])}

    def waiting(count: int) -> Any:
        def probe() -> Any:
            jobs = mistral_jobs(api, ids, "queued")
            return jobs if len(jobs) >= count and all(
                j["attempts"] == 0 and parse_ts(j["run_after"]) > time.time() + 14 * 60
                for j in jobs) else None
        return probe
    wait_for("refused job rescheduled about 15 min later, no attempt used", waiting(1),
             timeout=120)
    expect(not mistral_jobs(api, ids, "dead"), "no job died of the refused key")
    refused = mock_refused(mock)
    expect(refused > refused_before, f"Mistral was called and refused the key ({refused})")

    # Second idea: while the alert lasts, its job waits without calling Mistral.
    status, answer = api.submit_idea(AUTH_IDEAS[1])
    expect(status == 201 and answer["status"] != "rejected",
           f"second idea accepted during the alert ({answer['status']})")
    tokens.append(answer["tracking_token"])
    ids.add(idea_id(api, AUTH_IDEAS[1]))
    jobs = wait_for("second idea's job postponed without calling Mistral", waiting(2),
                    timeout=120)
    expect(mock_refused(mock) == refused, "no Mistral call while the alert lasts")
    for token in tokens:
        step = api.track(token)["status"]
        expect(step in ("accepted", "second_opinion", "dedup"),
               f"idea waits for Mistral in its current step ({step})")

    # « Vérifier la clé Mistral maintenant »: still refused, one call that bills nothing, and
    # the check is journaled.
    status, check = api.admin_call("POST", "/provider-alerts/mistral/check")
    expect(status == 200 and check["result"] == "refused" and check["http_status"] == 401
           and check["alert_cleared"] is False, "admin key check: still refused (HTTP 401)")
    status, page = api.admin_call("GET", "/actions?action=provider_alert.mistral_check")
    expect(status == 200 and page["total"] >= 1, "key check journaled")

    # The operator fixes the key: nothing runs before the retry delay.
    mock_auth(mock, None)
    time.sleep(5)
    expect(len(mistral_jobs(api, ids, "queued")) == len(jobs), "jobs still wait their delay")
    with open(state_path, "w", encoding="utf-8") as out:
        json.dump({"tokens": tokens, "ids": sorted(ids)}, out)


def auth_restored_phase(api: Api, state_path: str) -> None:
    with open(state_path, encoding="utf-8") as src:
        state = json.load(src)
    ids = set(state["ids"])
    for token in state["tokens"]:
        wait_for("waiting idea published once the key works",
                 track_status(api, token, "published"), timeout=300)
    status, dash = api.admin_call("GET", "/dashboard")
    expect(status == 200 and dash["alerts"] == [], "alert closed by the first accepted call")
    done = mistral_jobs(api, ids, "succeeded")
    expect({j["kind"] for j in done} >= {"dedup_idea", "analyze_idea"},
           f"dedup and analysis ran ({sorted(j['kind'] for j in done)})")
    expect(all(j["attempts"] == 1 for j in done),
           "each job used a single attempt " + str([(j["kind"], j["attempts"]) for j in done]))
    expect(not mistral_jobs(api, ids, "dead"), "no dead job")
    for token in state["tokens"]:
        item = api.track(token)
        status, detail = api.get(f"/api/ideas/{item['idea']['slug']}")
        expect(status == 200 and len(detail["analysis"]["sources"]) == 2,
               "complete analysis, no step skipped")


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
    parser.add_argument("--phase", choices=("main", "auth-refused", "auth-restored",
                                             "reasoning-refused", "budget"), default="main")
    parser.add_argument("--mock", default="http://127.0.0.1:18090",
                        help="mock Mistral (phases auth-refused, reasoning-refused)")
    parser.add_argument("--state", help="state file shared by the auth-* phases")
    parser.add_argument("--secrets-out")
    args = parser.parse_args()
    if args.phase.startswith("auth-") and not args.state:
        parser.error("--state is required by the auth-* phases")
    api = Api(args.public, args.admin, args.admin_token)
    api.ip_counter = {"main": 10, "auth-refused": 150, "auth-restored": 170,
                      "reasoning-refused": 185, "budget": 200}[args.phase]
    phases = {
        "main": lambda: main_phase(api),
        "auth-refused": lambda: auth_refused_phase(api, args.mock, args.state),
        "auth-restored": lambda: auth_restored_phase(api, args.state),
        "reasoning-refused": lambda: reasoning_refused_phase(api, args.mock),
        "budget": lambda: budget_phase(api),
    }
    try:
        phases[args.phase]()
    finally:
        if args.secrets_out:
            with open(args.secrets_out, "a", encoding="utf-8") as out:
                out.write("\n".join(SECRETS) + "\n")
    print(f"{len(CHECKS)} checks passed")


if __name__ == "__main__":
    sys.exit(main())
