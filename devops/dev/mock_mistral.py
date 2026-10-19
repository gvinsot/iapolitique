#!/usr/bin/env python3
"""Local stand-in for the Mistral API, for end-to-end runs of the stack (never in production).

Python standard library only. Implements the three endpoints the backend calls
(backend/src/ai/mistral.rs), with the response shapes of the Mistral API reference:

  POST /v1/chat/completions  structured outputs (`response_format` json_schema, strict), answered
                             according to the schema name: `analysis` (a VALID analysis per
                             prompts/analysis.schema.json and the backend's extra checks: its
                             sources cite the pages listed in <pages_consultees> by their
                             identifier R1, R2…, never by URL), `second_opinion`,
                             `report_review`;
  POST /v1/conversations     web research with `web_search`: a memo citing [S1], [S2], with
                             `tool.execution` entries (`name`, `arguments`, `info`) and
                             `tool_reference` chunks (`tool`, `title`, `url`, `favicon`,
                             `description`), the shapes of the OpenAPI spec
                             (ToolExecutionEntry, ToolReferenceChunk);
  POST /v1/embeddings        deterministic 1024-dimension vectors (normalised bag of words), so
                             that two near-identical texts get a cosine similarity close to 1.

Reasoning (`reasoning_effort`, AGENTS.md §6.1): top level in chat completions, inside
`completion_args` in conversations. With `high`, the answer content becomes a list of chunks
like Mistral's: a `thinking` chunk first (its text carries THINKING_MARKER, which must never
reach a page, an export or a log; in conversations it also holds a `tool_reference`), then the
`text` chunks; the thinking tokens are counted in `completion_tokens`. With `none` or without
the parameter, answers keep their plain shape. A value outside Mistral's enum is refused (400).

Answers are deterministic and driven by the prompt text:
  - second opinion: `reject` (reason `disrespectful`) when the text contains an insult marker,
    `accept` otherwise;
  - report review: `hide` + `escalate` when a report has the reason « Contenu problématique »,
    `update` (with corrections) for « Erreur de chiffrage » or « Source erronée », `maintain`
    otherwise;
  - analysis: considers every contribution id listed in <contributions>;
  - hostile mode (devops/dev/security/): when the idea, the report or the text carries the marker
    `sondexss`, every free-text field of the answer echoes XSS, CSV-formula and JSON-LD breakout
    payloads, and the web research returns pages whose titles and http(s) URLs carry such
    payloads (the published title and URL of a source come from the research), so that the
    security checks can follow them through the whole pipeline down to a real browser.

Usage:
  python devops/dev/mock_mistral.py [--host 0.0.0.0] [--port 8090]
Then point the api and the worker to it:
  MISTRAL_API_BASE_URL=http://<host>:8090  MISTRAL_API_KEY=<any non-empty value>

Logs show the method, path and status only: never a prompt (prompts carry visitor texts).
GET /stats returns the number of calls per endpoint (for test scripts); calls refused by the
key switch below are counted under `refused_auth`.

Key switch (devops/dev/e2e_scenario.py, phases `auth-*`): POST /control {"auth_status": 401}
(or 403) makes every /v1 call answer that status, as Mistral does with a revoked or wrong key;
{"auth_status": null} accepts the key again. Development only, like the whole mock.

Reasoning switch (phase `reasoning-refused`, AGENTS.md §6.1): POST /control
{"reject_reasoning_effort": ["mistral-large-latest"]} (or `--reject-reasoning-effort MODEL`,
repeatable) makes a chat or conversation request for one of these models that carries
`reasoning_effort` answer HTTP 400 with Mistral's error shape (`param: "reasoning_effort"`), as
a model that does not document the parameter; the same request without it is answered. `[]`
lifts it. Refusals are counted under `refused_reasoning_effort` in GET /stats.

GET /v1/models lists a few models without billing anything, like Mistral: the key check of the
admin dashboard (POST /api/admin/provider-alerts/mistral/check) calls it; the key switch applies.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import re
import sys
import threading
import unicodedata
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

EMBEDDING_DIMENSIONS = 1024
UUID_RE = re.compile(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
WORD_RE = re.compile(r"[a-z0-9]+")
INSULT_MARKERS = ("connard", "connards", "abruti", "abrutis", "imbecile", "imbeciles", "crétin",
                  "cretin", "cretins", "debile", "debiles", "pourri", "pourris", "ordure")
STOP_WORDS = frozenset(
    "le la les un une des du de d l et ou a au aux en dans par pour sur que qui ne pas plus est "
    "sont ce cet cette ces il elle ils elles on nous vous leur leurs son sa ses se y".split()
)

STATS: dict[str, int] = {}
# Status answered to every /v1 call while the key is "refused" (POST /control), None otherwise.
AUTH_REFUSAL: dict[str, int | None] = {"status": None}
# Models that refuse `reasoning_effort` (POST /control, --reject-reasoning-effort).
REASONING_REFUSED: set[str] = set()

# Text of the mock's thinking chunks: checked absent from the API, the exports and the logs.
THINKING_MARKER = "PENSEE-PRIVEE-MOCK"
REASONING_EFFORTS = ("none", "minimal", "low", "medium", "high", "xhigh")
THINKING_TOKENS = 1_500
STATS_LOCK = threading.Lock()

SOURCES = [
    {
        "id": "S1",
        "title": "La politique d'achat de l'État",
        "publisher": "Cour des comptes",
        "url": "https://www.ccomptes.fr/fr/publications/la-politique-dachat-de-letat",
        "used_for": "Montant des achats publics et gains constatés des mutualisations.",
    },
    {
        "id": "S2",
        "title": "Projet de loi de finances : documents budgétaires",
        "publisher": "Direction du budget",
        "url": "https://www.budget.gouv.fr/documentation/documents-budgetaires",
        "used_for": "Crédits de la mission concernée.",
    },
]


# Hostile mode (devops/dev/security/): payloads echoed by the AI answers. Each one, if it were
# ever interpreted as HTML or script, would set `window.__xss` (or fire a CSP violation, which
# devops/dev/security/xss_check.mjs also treats as a failure). The leading `=`, `+`, `-`, `@`
# exercise the CSV formula neutralisation of the exports.
HOSTILE_MARKER = "sondexss"
XSS_IMG = '<img src=x onerror="window.__xss=1">'
XSS_SCRIPT = "</script><script>window.__xss=2</script>"
XSS_SVG = "<svg/onload=window.__xss=3>"
XSS_IFRAME = '<iframe src="javascript:window.__xss=4"></iframe>'
XSS_ATTR = '" autofocus onfocus="window.__xss=5" x="'
HOSTILE_SOURCES = [
    {
        "id": "S1",
        "title": f"Rapport {XSS_SCRIPT}",
        "publisher": f"Éditeur {XSS_IMG}",
        "url": 'https://evil.example/rapport/"><script>window.__xss=6</script>',
        "used_for": f"Montants {XSS_SVG}",
    },
    {
        "id": "S2",
        "title": "=HYPERLINK(\"https://evil.example/\",\"cliquer\")",
        "publisher": "@SUM(1+1)*cmd|' /C calc'!A0",
        "url": f"https://evil.example/recherche?q={XSS_ATTR}",
        "used_for": f"Crédits {XSS_IFRAME}",
    },
]


def is_hostile(text: str) -> bool:
    return HOSTILE_MARKER in normalise(text)


def normalise(text: str) -> str:
    """Lower case without accents."""
    decomposed = unicodedata.normalize("NFKD", text.lower())
    return "".join(c for c in decomposed if not unicodedata.combining(c))


def words(text: str) -> list[str]:
    return [w for w in WORD_RE.findall(normalise(text)) if w not in STOP_WORDS]


def tokens(text: str) -> int:
    """Rough token count (4 characters per token)."""
    return max(1, len(text) // 4)


def between(text: str, tag: str) -> str:
    match = re.search(rf"<{tag}>\s*(.*?)\s*</{tag}>", text, re.S)
    return match.group(1) if match else ""


def embedding(text: str) -> list[float]:
    vector = [0.0] * EMBEDDING_DIMENSIONS
    for word in words(text):
        digest = hashlib.sha256(word.encode()).digest()
        index = int.from_bytes(digest[:4], "big") % EMBEDDING_DIMENSIONS
        sign = 1.0 if digest[4] & 1 else -1.0
        vector[index] += sign
    norm = math.sqrt(sum(x * x for x in vector))
    if norm == 0:
        vector[0] = 1.0
        return vector
    return [x / norm for x in vector]


def title_from(idea: str) -> str:
    first = re.split(r"(?<=[.!?])\s", idea.strip(), maxsplit=1)[0].rstrip(".!? ")
    first = " ".join(first.split())
    if len(first) > 90:
        first = first[:87].rsplit(" ", 1)[0]
    return first[:1].upper() + first[1:] if first else "Mesure proposée"


PAGE_ID_RE = re.compile(r"^(R[1-9][0-9]*) — ", re.M)


def citations(user: str, used_for: list[str]) -> list[dict]:
    """Sources S1, S2… citing the pages of <pages_consultees> by identifier (the last page again
    when there are fewer pages than sources)."""
    pages = PAGE_ID_RE.findall(between(user, "pages_consultees")) or ["R1"]
    return [{"id": f"S{i + 1}", "ref": pages[min(i, len(pages) - 1)], "used_for": text}
            for i, text in enumerate(used_for)]


def analysis(schema: dict, user: str) -> dict:
    idea = between(user, "idee_originale") or "Mesure proposée"
    contributions = between(user, "contributions")
    contribution_ids = UUID_RE.findall(contributions)
    report = between(user, "signalement")
    previous = between(user, "analyse_precedente")
    publics = list(schema["properties"]["impacts"]["properties"].keys())
    title = title_from(idea)
    seed = int(hashlib.sha256(normalise(idea).encode()).hexdigest()[:6], 16)
    median = 20_000_000 + (seed % 80) * 1_000_000
    changes = ""
    if previous.strip() and previous.strip() not in ("(aucune)", ""):
        parts = []
        if contribution_ids:
            parts.append(f"{len(contribution_ids)} contribution(s) intégrée(s)")
        if report.strip() and report.strip() != "(aucun)":
            parts.append("corrections issues d'un signalement appliquées")
        changes = ("Nouvelle analyse : " + ", ".join(parts) + ".") if parts else "Analyse relancée."
    return {
        "title": title,
        "summary": f"La mesure propose : {title.lower()}. Elle vise à réduire la dépense publique "
                   "en rationalisant les achats concernés [S1].",
        "improved_proposal": "Mutualiser les marchés concernés au niveau national, avec un "
                             "calendrier sur deux ans et un suivi des économies réalisées [S1].",
        "existing_situation": "Des centrales d'achat publiques existent déjà et couvrent une "
                              "partie des besoins [S1].",
        "admin_level": "multi",
        "territory": {"name": "", "insee_code": ""},
        "category": "achats_publics",
        "kind": "savings",
        "annual_gain_eur": {
            "low": median // 2,
            "median": median,
            "high": median * 2,
            "calculation": "Assiette des achats concernés × taux d'économie de 2 à 8 % [S1] [S2].",
        },
        "implementation_cost_eur": {
            "low": 1_000_000,
            "median": 3_000_000,
            "high": 6_000_000,
            "calculation": "Coût de passation et d'outillage des marchés mutualisés [S1].",
        },
        "time_to_effect_months": 12 + seed % 24,
        "legal_vehicle": "organisation_interne",
        "feasibility": {"score": 3 + seed % 3, "obstacles": ["Durée des marchés en cours."]},
        "confidence": ["low", "medium", "high"][seed % 3],
        "confidence_explanation": "Données agrégées disponibles, ventilation fine absente [S2].",
        "assumptions": ["Taux d'économie observé sur les mutualisations passées [S1]."],
        "pros": ["Effet de volume sur les prix.", "Réduction des coûts de passation.",
                 "Meilleure connaissance des achats."],
        "cons": ["Moindre accès des petites entreprises.", "Délais de mise en place.",
                 "Rigidité des marchés nationaux."],
        "risks": ["Dépendance à un fournisseur unique.", "Économies surestimées."],
        "impacts": {
            key: {"effect": "neutral" if i % 2 else "uncertain",
                  "explanation": "Effet limité et indirect pour ce public."}
            for i, key in enumerate(publics)
        },
        "sources": citations(user, [s["used_for"] for s in SOURCES]),
        "contributions_considered": [
            {
                "contribution_id": cid,
                "summary": "La contribution propose de préciser le périmètre de la mesure.",
                "taken_into_account": True,
                "response": "Le périmètre a été précisé dans la proposition améliorée.",
            }
            for cid in contribution_ids
        ],
        "changes_since_previous": changes,
    }


def hostile_analysis(schema: dict, user: str) -> dict:
    """A schema-valid analysis whose every free text carries a payload (hostile mode)."""
    content = analysis(schema, user)
    content.update({
        "title": f'=HYPERLINK("https://evil.example/","x") {XSS_SCRIPT}',
        "summary": f"+SUM(1;2) {XSS_IMG} {XSS_SCRIPT} [S1].",
        "improved_proposal": f"-2+3 {XSS_SVG} [S2]",
        "existing_situation": f"@cmd {XSS_IFRAME} [S1]",
        "confidence_explanation": f"{XSS_ATTR} [S2]",
        "changes_since_previous": XSS_SCRIPT if content["changes_since_previous"] else "",
        "assumptions": [f"{XSS_IMG} [S1]"],
        "pros": [XSS_SCRIPT, XSS_SVG, "javascript:window.__xss=7"],
        "cons": [XSS_IFRAME, XSS_ATTR, "{{constructor.constructor('window.__xss=8')()}}"],
        "risks": [XSS_IMG, "<!--", "]]>"],
        "sources": citations(user, [s["used_for"] for s in HOSTILE_SOURCES]),
    })
    content["annual_gain_eur"]["calculation"] = f"=1+1 {XSS_IMG} [S1]"
    content["implementation_cost_eur"]["calculation"] = f"{XSS_SCRIPT} [S2]"
    content["feasibility"]["obstacles"] = [XSS_SVG]
    for impact in content["impacts"].values():
        impact["explanation"] = XSS_IMG
    for item in content["contributions_considered"]:
        item["summary"] = f"=CMD() {XSS_SCRIPT}"
        item["response"] = XSS_IMG
    return content


def second_opinion(user: str) -> dict:
    text = normalise(between(user, "texte"))
    if is_hostile(text):
        return {"decision": "accept", "reason_code": "none", "explanation": XSS_IMG}
    if any(marker in WORD_RE.findall(text) for marker in map(normalise, INSULT_MARKERS)):
        return {"decision": "reject", "reason_code": "disrespectful",
                "explanation": "Le texte contient des propos irrespectueux."}
    return {"decision": "accept", "reason_code": "none",
            "explanation": "Le texte respecte les critères de la grille."}


def report_review(user: str) -> dict:
    reports = normalise(between(user, "signalements"))
    if is_hostile(reports):
        return {"outcome": "maintain", "escalate": False,
                "explanation": f"{XSS_SCRIPT} {XSS_IMG}", "corrections": ""}
    if "contenu problematique" in reports:
        return {"outcome": "hide", "escalate": True,
                "explanation": "Le signalement relève un contenu grave : l'idée est masquée et "
                               "transmise pour examen humain.",
                "corrections": ""}
    if "erreur de chiffrage" in reports or "source erronee" in reports:
        return {"outcome": "update", "escalate": False,
                "explanation": "Le signalement met en doute le chiffrage : l'analyse est refaite.",
                "corrections": "Revoir l'hypothèse de taux d'économie à la lumière du signalement."}
    return {"outcome": "maintain", "escalate": False,
            "explanation": "L'analyse reste valable au vu des signalements reçus.",
            "corrections": ""}


def reasoning_effort(body: dict) -> str | None:
    """`reasoning_effort` of a request: top level (chat) or in `completion_args` (conversations)."""
    args = body.get("completion_args")
    nested = args.get("reasoning_effort") if isinstance(args, dict) else None
    return body.get("reasoning_effort", nested)


def thinking_chunk(extra: list[dict] | None = None) -> dict:
    """A closed `thinking` chunk (Mistral's ThinkChunk) that must never be shown."""
    text = {"type": "text",
            "text": f"{THINKING_MARKER} Je relis la demande et les sources avant de répondre."}
    return {"type": "thinking", "thinking": [text, *(extra or [])], "closed": True}


def chat_completion(body: dict) -> tuple[int, dict]:
    messages = body.get("messages") or []
    system = next((m.get("content", "") for m in messages if m.get("role") == "system"), "")
    user = next((m.get("content", "") for m in messages if m.get("role") == "user"), "")
    response_format = body.get("response_format") or {}
    spec = response_format.get("json_schema") or {}
    name = spec.get("name")
    if response_format.get("type") != "json_schema" or spec.get("strict") is not True:
        return 400, {"object": "error", "message": "strict json_schema expected"}
    if name == "analysis":
        hostile = is_hostile(between(user, "idee_originale"))
        content = (hostile_analysis if hostile else analysis)(spec["schema"], user)
    elif name == "second_opinion":
        content = second_opinion(user)
    elif name == "report_review":
        content = report_review(user)
    else:
        return 400, {"object": "error", "message": f"unknown schema {name!r}"}
    text = json.dumps(content, ensure_ascii=False)
    prompt_tokens = tokens(system) + tokens(user)
    completion_tokens = tokens(text)
    message_content: str | list[dict] = text
    if reasoning_effort(body) == "high":
        # The JSON answer split over two text chunks after the thinking, as Mistral may do.
        half = len(text) // 2
        message_content = [thinking_chunk(), {"type": "text", "text": text[:half]},
                           {"type": "text", "text": text[half:]}]
        completion_tokens += THINKING_TOKENS
    return 200, {
        "id": "cmpl-mock",
        "object": "chat.completion",
        "model": body.get("model", "mock"),
        "created": 0,
        "choices": [{"index": 0, "finish_reason": "stop",
                     "message": {"role": "assistant", "content": message_content,
                                 "tool_calls": None}}],
        "usage": {"prompt_tokens": prompt_tokens, "completion_tokens": completion_tokens,
                  "total_tokens": prompt_tokens + completion_tokens},
    }


def conversation(body: dict) -> tuple[int, dict]:
    tools = body.get("tools") or []
    if not any(t.get("type") == "web_search" for t in tools):
        return 400, {"object": "error", "message": "web_search tool expected"}
    if body.get("store") is not False:
        return 400, {"object": "error", "message": "store must be false"}
    inputs = body.get("inputs") or ""
    if isinstance(inputs, list):
        inputs = json.dumps(inputs, ensure_ascii=False)
    sources = HOSTILE_SOURCES if is_hostile(inputs) else SOURCES

    def reference(source: dict) -> dict:
        return {"type": "tool_reference", "tool": "web_search", "title": source["title"],
                "url": source["url"], "favicon": None, "description": None}

    memo_chunks: list[dict] = [
        {"type": "text", "text": "1. Mesure étudiée : mutualisation des achats publics [S1]."},
        reference(sources[0]),
        {"type": "text", "text": "\n2. Montants : crédits de la mission concernée [S2]."},
        reference(sources[1]),
        {"type": "text", "text": "\n3. Sources :\n[S1] " + sources[0]["title"] + " — "
                                 + sources[0]["publisher"] + " — " + sources[0]["url"]
                                 + "\n[S2] " + sources[1]["title"] + " — "
                                 + sources[1]["publisher"] + " — " + sources[1]["url"]},
    ]
    completion_tokens = 300
    if reasoning_effort(body) == "high":
        # Pages met while thinking are pages the search returned: the backend keeps them.
        memo_chunks.insert(0, thinking_chunk([reference(sources[0])]))
        completion_tokens += THINKING_TOKENS
    prompt_tokens = tokens(body.get("instructions") or "") + tokens(inputs)
    return 200, {
        "object": "conversation.response",
        "conversation_id": "conv_mock",
        "outputs": [
            {"object": "entry", "type": "tool.execution", "name": "web_search", "id": "t1",
             "arguments": "{\"query\": \"achats publics\"}", "info": {}},
            {"object": "entry", "type": "tool.execution", "name": "web_search", "id": "t2",
             "arguments": "{\"query\": \"crédits mission\"}", "info": {}},
            {"object": "entry", "type": "message.output", "role": "assistant", "id": "m1",
             "model": body.get("model", "mock"), "content": memo_chunks},
        ],
        "usage": {"prompt_tokens": prompt_tokens, "completion_tokens": completion_tokens,
                  "total_tokens": prompt_tokens + completion_tokens + 2_000,
                  "connector_tokens": 2_000, "connectors": {"web_search": 2}},
    }


def embeddings(body: dict) -> tuple[int, dict]:
    inputs = body.get("input")
    if isinstance(inputs, str):
        inputs = [inputs]
    if not isinstance(inputs, list) or not inputs:
        return 400, {"object": "error", "message": "input expected"}
    return 200, {
        "id": "emb-mock",
        "object": "list",
        "model": body.get("model", "mistral-embed"),
        "data": [{"object": "embedding", "index": i, "embedding": embedding(str(text))}
                 for i, text in enumerate(inputs)],
        "usage": {"prompt_tokens": sum(tokens(str(t)) for t in inputs), "total_tokens": 0},
    }


ROUTES = {
    "/v1/chat/completions": chat_completion,
    "/v1/conversations": conversation,
    "/v1/embeddings": embeddings,
}


class Handler(BaseHTTPRequestHandler):
    server_version = "mock-mistral"
    sys_version = ""

    def log_message(self, format: str, *args: object) -> None:  # noqa: A002 - stdlib signature
        # Method, path and status only: the default would also be fine, but keep it explicit.
        sys.stderr.write("%s\n" % (format % args))

    def send_json(self, status: int, payload: dict) -> None:
        data = json.dumps(payload, ensure_ascii=False).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self) -> None:  # noqa: N802 - stdlib naming
        if self.path == "/stats":
            with STATS_LOCK:
                self.send_json(200, dict(STATS))
        elif self.path == "/v1/models":
            # Lists models without billing anything: the admin's key check calls it.
            if not self.headers.get("Authorization", "").startswith("Bearer "):
                self.send_json(401, {"object": "error", "message": "missing bearer key"})
                return
            with STATS_LOCK:
                refusal = AUTH_REFUSAL["status"]
                name = "refused_auth" if refusal is not None else "/v1/models"
                STATS[name] = STATS.get(name, 0) + 1
            if refusal is not None:
                self.send_json(refusal, {"message": "Unauthorized", "request_id": "mock"})
                return
            self.send_json(200, {"object": "list", "data": [
                {"id": model, "object": "model", "owned_by": "mistralai"}
                for model in ("mistral-large-latest", "mistral-large-4-0", "mistral-embed")]})
        else:
            self.send_json(404, {"object": "error", "message": "not found"})

    def control(self) -> None:
        try:
            length = int(self.headers.get("Content-Length", "0"))
            body = json.loads(self.rfile.read(length) or b"{}")
        except (ValueError, json.JSONDecodeError):
            body = None
        if not isinstance(body, dict) or not body.keys() & {"auth_status",
                                                            "reject_reasoning_effort"}:
            self.send_json(400, {"object": "error",
                                 "message": "auth_status or reject_reasoning_effort expected"})
            return
        status = body.get("auth_status")
        models = body.get("reject_reasoning_effort", [])
        if status not in (None, 401, 403) or not isinstance(models, list) or                 not all(isinstance(m, str) for m in models):
            self.send_json(400, {"object": "error", "message":
                                 "auth_status: 401, 403 or null; reject_reasoning_effort: models"})
            return
        with STATS_LOCK:
            if "auth_status" in body:
                AUTH_REFUSAL["status"] = status
            if "reject_reasoning_effort" in body:
                REASONING_REFUSED.clear()
                REASONING_REFUSED.update(models)
            answer = {"auth_status": AUTH_REFUSAL["status"],
                      "reject_reasoning_effort": sorted(REASONING_REFUSED)}
        self.send_json(200, answer)

    def do_POST(self) -> None:  # noqa: N802 - stdlib naming
        if self.path == "/control":
            self.control()
            return
        route = ROUTES.get(self.path)
        if route is None:
            self.send_json(404, {"object": "error", "message": "not found"})
            return
        if not self.headers.get("Authorization", "").startswith("Bearer "):
            self.send_json(401, {"object": "error", "message": "missing bearer key"})
            return
        with STATS_LOCK:
            refusal = AUTH_REFUSAL["status"]
            if refusal is not None:
                STATS["refused_auth"] = STATS.get("refused_auth", 0) + 1
        if refusal is not None:
            # Mistral's answer to a wrong or revoked key.
            self.send_json(refusal, {"message": "Unauthorized", "request_id": "mock"})
            return
        try:
            length = int(self.headers.get("Content-Length", "0"))
            body = json.loads(self.rfile.read(length) or b"{}")
        except (ValueError, json.JSONDecodeError):
            self.send_json(400, {"object": "error", "message": "invalid JSON"})
            return
        if not isinstance(body, dict):
            self.send_json(400, {"object": "error", "message": "JSON object expected"})
            return
        effort = reasoning_effort(body)
        if effort is not None and effort not in REASONING_EFFORTS:
            self.send_json(400, {"object": "error", "message": "invalid reasoning_effort"})
            return
        with STATS_LOCK:
            refuse = effort is not None and body.get("model") in REASONING_REFUSED
            if refuse:
                STATS["refused_reasoning_effort"] = STATS.get("refused_reasoning_effort", 0) + 1
        if refuse:
            # Mistral's error shape (docs.mistral.ai, error glossary) for a parameter the model
            # does not take.
            self.send_json(400, {
                "object": "error",
                "message": f"Extra inputs are not permitted: reasoning_effort is not supported "
                           f"by model {body.get('model')}",
                "type": "invalid_request_error",
                "param": "reasoning_effort",
                "code": None,
            })
            return
        status, payload = route(body)
        # Calls per endpoint, and per endpoint and reasoning effort sent (`omitted` without).
        key = f"{self.path} reasoning_effort={effort or 'omitted'}"
        with STATS_LOCK:
            STATS[self.path] = STATS.get(self.path, 0) + 1
            STATS[key] = STATS.get(key, 0) + 1
        self.send_json(status, payload)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--host", default="0.0.0.0")
    parser.add_argument("--port", type=int, default=8090)
    parser.add_argument("--reject-reasoning-effort", action="append", default=[],
                        metavar="MODEL", help="model that refuses reasoning_effort (repeatable)")
    args = parser.parse_args()
    REASONING_REFUSED.update(args.reject_reasoning_effort)
    server = ThreadingHTTPServer((args.host, args.port), Handler)
    print(f"mock Mistral listening on http://{args.host}:{args.port}", file=sys.stderr)
    server.serve_forever()


if __name__ == "__main__":
    main()
