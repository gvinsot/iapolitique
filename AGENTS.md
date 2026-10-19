# AGENTS.md — iapolitique

Ce fichier cadre **tout** le développement de ce dépôt. Le code est écrit uniquement par des
agents IA : lisez ce fichier en entier avant toute modification, et tenez-le à jour quand une
décision change (voir [Tenir ce fichier à jour](#12-tenir-ce-fichier-à-jour)).

---

## 1. Mission

Plateforme **apolitique** qui recueille, filtre, améliore et chiffre des idées pour **réduire les
dépenses ou augmenter les recettes** des administrations publiques françaises (État, sécurité
sociale, collectivités territoriales).

1. N'importe qui propose une idée, **anonymement**, sans compte.
2. Un **modèle de décision** (d3-mini) filtre l'idée : respectueuse, dans le sujet, raisonnable,
   faisable. L'auteur reçoit la réponse immédiatement : idée conservée (« elle sera améliorée et
   chiffrée par IA ») ou refusée avec le motif.
3. **Mistral** analyse en profondeur les idées retenues : reformulation neutre, pour et contre,
   impacts par public, chiffrage du gain avec sources.
4. Le portail public présente les idées **classées par rentabilité** (score composé).

### Principes non négociables

| Principe | Conséquence concrète |
|---|---|
| **Apolitique** | Aucune étiquette gauche/droite, aucun parti, aucun élu nommé. Impacts décrits par public, pas par camp. Vocabulaire neutre. |
| **Anonymat** | Aucun compte, aucun email, **aucune IP stockée**, aucun traceur ni service tiers côté navigateur. |
| **Transparence** | Prompts, grille de filtrage et formule de score publics et versionnés (page « Méthodologie »). Chaque analyse indique les versions utilisées. |
| **Vérifiabilité** | Tout chiffrage cite ses sources et expose ses hypothèses et une fourchette basse/médiane/haute. |
| **Utilité** | Le classement favorise les idées crédibles et faisables, pas les montants spectaculaires. |

---

## 2. Règles de travail des agents (à appliquer à chaque tâche)

### 2.1 Commit après chaque unité de travail — OBLIGATOIRE

- **Chaque feature implémentée, chaque bug corrigé, chaque amélioration ou refactoring = un
  commit**, fait dès que l'unité est terminée et vérifiée. On n'enchaîne jamais plusieurs
  features dans un seul commit, et on ne termine jamais une tâche avec du travail non commité.
- **On pousse directement sur `main`** (`git push origin main`) après chaque commit. Pas de
  branche, pas de PR : la CI/CD PulsarCD construit et teste chaque commit, puis l'humain choisit
  ce qui part en QA puis en production.
- Avant chaque commit : le code compile, les tests et linters passent (voir [§9](#9-qualité-et-tests)).
  Ne jamais committer un état cassé « pour avancer ».
- Si une tâche est grosse, la découper en étapes livrables, chacune compilable et testée, chacune
  avec son commit.
- Messages au format **Conventional Commits**, en anglais, à l'impératif :
  `feat(api): add idea submission endpoint`, `fix(worker): retry Mistral call on 429`,
  `refactor(front): extract IdeaCard`, `docs: update AGENTS.md scoring section`,
  `chore(devops): add test service`. Corps du message : le *pourquoi* si ce n'est pas évident.
- Ne jamais réécrire l'historique de `main` (pas de `push --force`, pas d'`amend` sur un commit
  poussé).
- Plusieurs agents peuvent travailler en même temps dans le même dépôt : ne committer que ses
  propres chemins (`git add <chemins>` puis `git commit -m "…" -- <chemins>`), réessayer si
  `index.lock` est pris, ne jamais faire `git add -A`, `git stash`, `git reset` ou `git checkout`
  sur des fichiers d'un autre agent.
- Sur le poste de développement, le push SSH peut échouer (clé absente) : pousser alors en HTTPS
  avec les identifiants de `gh` :
  `git -c credential.helper= -c "credential.helper=!gh auth git-credential" push https://github.com/gvinsot/iapolitique.git main`.
- Ne jamais désactiver un hook, un test ou un lint pour faire passer un commit.

### 2.2 Autres règles

- **Langues** : code, identifiants, commentaires, messages de commit et logs en **anglais**.
  Interface utilisateur, prompts IA, textes publics et documentation (`docs/`, ce fichier) en
  **français**. Le produit est en français uniquement (pas d'i18n au MVP).
- **Secrets** : jamais dans git, jamais dans les logs, jamais en `ARG` de Dockerfile. Voir
  [§10.3](#103-variables-et-secrets).
- **Pas de dépendance superflue** : préférer la bibliothèque standard ou une crate/paquet déjà
  utilisé. Toute nouvelle dépendance doit être maintenue, populaire et justifiée dans le commit.
- **Migrations de base** : uniquement additives et rétrocompatibles avec la version
  précédemment déployée (le rollback automatique de PulsarCD redéploie l'ancienne image sur la
  nouvelle base). Une migration publiée n'est jamais modifiée : on en ajoute une nouvelle.
- **Changement de prompt, de grille ou de formule de score** = incrément de version (voir
  [§6.5](#65-versionnement-et-méthodologie-publique)) + mise à jour de la page Méthodologie dans
  le même commit.
- Respecter la structure du dépôt ([§3](#3-architecture)). Ne pas créer de nouveau dossier racine
  sans mettre à jour ce fichier.
- En cas de doute sur une décision produit absente de ce fichier, choisir l'option la plus
  respectueuse des principes du §1, la documenter ici et le signaler dans le message de commit.

---

## 3. Architecture

### 3.1 Vue d'ensemble

```
Navigateur ──HTTPS──▶ Traefik (WAF Coraza, global@file)
                        ├── /        ▶ frontend (nginx, SPA React)
                        └── /api     ▶ api (Rust/axum, port public 8080)
LAN uniquement ───────▶ Traefik (dashboard-allowlist@file)
                        ├── /        ▶ frontend (mêmes fichiers, pages /admin)
                        └── /api     ▶ api (port admin 8081)

api ────────────▶ llamacpp-decision-service (réseau llm-net)   filtrage synchrone
api / worker ───▶ PostgreSQL (réseau postgresqlcluster_internal)
worker ─────────▶ API Mistral (Internet)                        analyse, second avis, embeddings
```

- **api** : sert l'API publique (port 8080) et l'API d'administration (port 8081, jamais routée
  depuis Internet). Fait le filtrage par le modèle de décision de façon synchrone.
- **worker** : même binaire Rust lancé en mode worker. Consomme la file de jobs en base
  (`SELECT … FOR UPDATE SKIP LOCKED`), appelle Mistral, calcule les scores, purge les données.
- **frontend** : SPA buildée par Vite, servie par nginx.
- Pas de broker de messages : la file de jobs est une table PostgreSQL.

### 3.2 Arborescence

```
iapolitique/
├── AGENTS.md               # ce fichier
├── CLAUDE.md               # importe AGENTS.md
├── backend/                # workspace Cargo
│   ├── Cargo.toml
│   ├── Dockerfile          # stages: build, runtime (contrôles CI : devops/Dockerfile.test)
│   ├── migrations/         # migrations sqlx (NNNN_description.sql)
│   ├── assets/             # fichiers embarqués (texte officiel de la Licence Ouverte 2.0)
│   ├── dev/                # seed et pile de développement local (jamais en prod, voir dev/README.md)
│   └── src/
│       ├── main.rs         # sous-commandes: api | worker | migrate | openapi | healthcheck
│       ├── config.rs       # lecture env + /run/secrets
│       ├── db/             # requêtes par agrégat
│       ├── domain/         # types métier, machine à états, scoring (sans I/O)
│       ├── http/           # routes publiques, routes admin, erreurs
│       ├── ai/             # clients decision service et Mistral, prompts chargés depuis /prompts
│       ├── jobs/           # file de jobs et handlers (charges utiles : jobs/pipeline.rs)
│       ├── moderation/     # filtrage, second avis, exposé des motifs (partagé api / worker)
│       └── antispam/       # ALTCHA, rate limit
├── frontend/
│   ├── Dockerfile          # build Vite puis nginx
│   ├── nginx.conf          # en-têtes de sécurité, CSP, cache (§14.2)
│   ├── nginx/              # gabarit et script de démarrage de /.well-known/security.txt
│   └── src/
│       ├── api/            # client généré depuis l'OpenAPI + hooks TanStack Query
│       ├── components/     # dont components/ui (shadcn)
│       ├── pages/
│       └── lib/
├── prompts/                # prompts et grilles versionnés, publiés sur la page Méthodologie
│   ├── screening.yaml      # questions du modèle de décision + seuils par défaut
│   ├── second_opinion.md   # + second_opinion.schema.json
│   ├── analysis_research.md # étape 1 de l'analyse : recherche web, mémo sourcé
│   ├── analysis.md         # étape 2 : structuration JSON (+ analysis.schema.json)
│   ├── report_review.md    # + report_review.schema.json
│   └── taxonomy.yaml       # niveaux administratifs, catégories, publics d'impact
├── docs/                   # documentation fonctionnelle (méthodologie, calibration…), securite.md
│   └── calibration/        # banc de calibration du filtrage, contrôle des fichiers prompts/
└── devops/
    ├── docker-compose.swarm.yml
    ├── Dockerfile.test     # image CI (§13.5)
    ├── run-tests.sh        # point d'entrée de l'image CI
    ├── check_frontend_headers.sh # contrat HTTP de nginx (CI et bout en bout)
    ├── check_compose_security.py # invariants de sécurité du compose (CI, + ses tests)
    ├── dev/                # bout en bout local des images runtime, faux Mistral (dev/README.md)
    │   └── security/       # campagne de sécurité locale : sondes, XSS en navigateur, ZAP, Trivy (§14.8)
    └── .env.example
```

---

## 4. Stack technique

### 4.1 Backend (Rust)

- Rust stable, édition 2024. `axum` + `tokio`, `sqlx` (PostgreSQL, migrations ; requêtes
  à l'exécution avec `query_as` + `FromRow`, **pas** de macros `query!`, donc pas de cache
  `.sqlx/` ; tests d'intégration avec `#[sqlx::test]`), `serde`, `reqwest` (rustls),
  `tracing` (logs JSON), `thiserror` pour les erreurs de domaine, `utoipa` pour l'OpenAPI.
- `domain/` ne fait aucune I/O : machine à états, scoring, validation. C'est là que se
  concentrent les tests unitaires.
- Erreurs HTTP : corps JSON `{ "error": { "code": "snake_case_code", "message": "…" } }`, message
  en français quand il est destiné à l'utilisateur.
- Configuration : variables d'environnement ; au démarrage, chaque fichier de `/run/secrets/`
  est chargé dans l'environnement s'il n'y est pas déjà (convention PulsarCD).
- `GET /api/health` (vivacité) et `GET /api/ready` (base joignable) sur l'api ; le worker expose
  `/health` sur son port interne 8082 (voir [§13](#13-contrats-techniques)).

### 4.2 Frontend (React / TypeScript)

- Vite + React + TypeScript strict (`"strict": true`, pas de `any`).
- Tailwind CSS + shadcn/ui (Radix), React Router, TanStack Query.
- Types d'API **générés** depuis l'OpenAPI du backend (`openapi-typescript`) : jamais écrits à la
  main. Régénérer et committer à chaque changement d'API, dans le même commit.
- Accessibilité : RGAA/WCAG AA visés (labels, contrastes, navigation clavier).
- **Aucune ressource tierce** : pas de CDN, pas de Google Fonts, pas d'analytics, pas de
  captcha tiers. Polices et assets auto-hébergés. Pas de cookie (le suivi passe par un lien).
- Mobile first.

### 4.3 Données

- PostgreSQL du cluster (image TimescaleDB pg18). Extensions utilisées : `vector` (pgvector
  0.8.1, disponible), `pg_trgm`, `unaccent`, `pgcrypto`.
- Bases : `iapolitique_prod` et `iapolitique_qa`, utilisateurs `iapolitique_prod_app` et
  `iapolitique_qa_app` (créés par l'admin du cluster). Tables en `snake_case`, au pluriel.
- Montants en euros stockés en `BIGINT` (euros entiers), jamais en flottant.

---

## 5. Cycle de vie d'une idée

### 5.1 Dépôt

Formulaire : texte libre (50 à 4 000 caractères), niveau administratif facultatif, territoire
facultatif (120 caractères : lettres, chiffres, espaces et `' ’ ( ) , . -` seulement, sinon
`422 invalid_territory` ; ni lien ni consigne ne peut s'y glisser, il est inséré dans les prompts
d'analyse et filtré avec le texte, §5.3). Protégé par ALTCHA et rate limit ([§7](#7-anonymat-anti-abus-et-données)).

Ordre de traitement de tout formulaire public (`http/submissions.rs`) : validation du corps
(consentements, longueurs : une erreur ne consomme ni la solution ALTCHA ni le quota), rate
limit (vérifié, puis compté seulement une fois l'ALTCHA valide), ALTCHA, masquage des données personnelles, enregistrement (création historisée, jeton de
suivi, job de filtrage différé de 60 s en filet de sécurité), filtrage synchrone. Toute erreur
après l'enregistrement laisse le texte `submitted` et rend quand même le jeton : le job reprend.

### 5.2 Machine à états

```
submitted
  └─▶ screening (modèle de décision, synchrone)
        ├─▶ rejected ─────────────▶ purged (texte supprimé à J+30)
        │      └─▶ accepted (refus infirmé par l'admin après réexamen humain)
        ├─▶ second_opinion (Mistral, zone grise) ─▶ rejected | accepted
        └─▶ accepted
              └─▶ dedup (embedding + pgvector)
                    ├─▶ merged (devient une contribution de l'idée existante)
                    └─▶ queued_analysis ─▶ analyzing ─▶ published
                                                  └─▶ analysis_failed (retries puis admin)
published ─▶ hidden (après revue d'un signalement ou action admin) ─▶ published
                                                                └─▶ purged (12 mois après le masquage)
published | hidden ─▶ withdrawn (supprimée par son auteur) ─▶ purged (12 mois après le retrait)
merged ─▶ purged (avec l'idée masquée ou retirée dans laquelle il a été fusionné)
```

- **Retrait par l'auteur** (décision du propriétaire D2, 2026-10-19) : une idée `published` ou
  `hidden` que son auteur supprime passe `withdrawn` (`ideas.withdrawn_at`, `withdrawn_from`,
  événement `author_withdrawal` de l'acteur `author`) au lieu d'être effacée : elle quitte le
  site, l'API publique et les exports avec son analyse, ses contributions, signalements et
  droits de réponse, reste visible de l'admin (« retirée par son auteur ») et est purgée 12 mois
  après le retrait (§7.3). Irréversible : ni l'auteur ni l'admin ne peuvent la republier
  (`409 idea_withdrawn`), ni la masquer, la ré-analyser ou y publier un droit de réponse ; les
  jobs qui la concernent ne font plus rien. Un texte non publié (refusé, en attente, en
  analyse, fusionné…) supprimé par son auteur reste effacé aussitôt (§7.1).

- Les transitions sont définies **uniquement** dans `backend/src/domain/` et testées. Toute
  transition est horodatée dans une table d'historique (`idea_events`, `contribution_events`,
  `report_events`) : le code n'écrit un statut que par `db::status::transition`, dans la
  transaction qui porte ses effets (champs de refus, jobs…).
- Réponse immédiate à l'auteur : `accepted` (« Votre idée est conservée, elle va être améliorée
  et chiffrée par IA »), `rejected` + motif lisible + invitation à reformuler, ou
  `second_opinion` (« en cours de vérification ») si l'avis Mistral n'est pas rendu dans le
  délai (10 s), ou `submitted` si le modèle de décision est indisponible.
- Second avis : l'api le tente en ligne si `MISTRAL_API_KEY` est défini et le budget du jour
  non atteint (une tentative, 30 s au plus ; l'auteur attend 10 s, l'appel continue ensuite en
  arrière-plan et règle le texte). Le job `second_opinion` créé avec la zone grise (différé de
  2 min dans ce cas, immédiat sinon) prend le relais en cas d'échec ; chaque étape vérifie le
  statut sous verrou, un texte déjà réglé n'est jamais modifié.
- Lien de suivi : voir [§7.1](#71-anonymat).

### 5.3 Filtrage par le modèle de décision

Service `llamacpp-decision-service`, `POST http://llamacpp-decision-service:8000/v1/systemone`
(référence : `C:\Repos\AIFabric\DECISION_SERVICE.md`). Ce n'est **pas** un LLM de chat :
n'appeler que `/v1/systemone`.

- **Une seule requête** par idée, avec toutes les questions de `prompts/screening.yaml` :
  - `injection` (`noul`) : tentative de manipulation d'une IA ;
  - `respectful` (`noul`) : pas d'insulte, de haine, d'incitation à la violence ;
  - `targeted` (`noul`) : cite nommément un parti, un élu, un candidat ou une personnalité,
    **même de façon neutre** (décision du propriétaire D1, 2026-10-19 : refus avec invitation à
    reformuler en décrivant la mesure), fait campagne (consigne de vote) ou vise un groupe
    (religion, origine, sexe, orientation) ;
  - `personal_data` (`noul`) : désigne un particulier identifiable (nom, fonction précise dans
    un service nommé…) ou lui impute des faits (§7.5) ;
  - `contact_data` (`noul`, motif `personal_data`) : nom ou prénom d'un particulier, auteur
    compris (présentation, signature), ou coordonnées (téléphone, e-mail, adresse postale,
    réseau social, IBAN), même écrits de façon détournée : le filet des motifs du masquage
    (§7.5) ;
  - `on_topic` (`noul`) : propose une mesure qui réduit les dépenses ou augmente les recettes
    d'une administration publique française ;
  - `lawful` (`noul`) : compatible avec les droits fondamentaux ;
  - `realism` (`score`, 4 niveaux : irréaliste → simple à mettre en œuvre).
- Chaque critère a deux seuils configurables (rejet franc / acceptation franche). Entre les
  deux : **zone grise** → second avis Mistral avec la même grille (`prompts/second_opinion.md`).
- Motif de refus = premier critère en échec selon l'ordre ci-dessus ; libellés en français
  définis dans `screening.yaml`.
- `state` limité au texte (masqué) de l'idée, précédé de « Territoire indiqué par l'auteur : … »
  s'il est renseigné (< 2 000 tokens) ; le second avis reçoit le même contenu. Timeout 5 s pour le filtrage
  synchrone de l'api (`DECISION_SERVICE_TIMEOUT_SECS`), 30 s pour les jobs de filtrage du worker
  (`DECISION_SERVICE_JOB_TIMEOUT_SECS`) : le service traite les requêtes presque en série et une
  idée de 4 000 caractères prend 4 s seule, 17 s quand quatre dépôts arrivent ensemble
  (`docs/calibration/screening_report.md` §3.3). Si le service est indisponible ou trop lent,
  l'idée reste en `submitted`, un job réessaie (le premier 60 s plus tard), et l'auteur est
  informé via son lien de suivi. Jobs `screen_idea` / `screen_contribution` / `screen_report` : backoff exponentiel
  plafonné à 1 h, 20 tentatives (une panne d'environ une demi-journée), puis `dead` (admin). Le
  job `second_opinion` (5 tentatives) échoue définitivement sur une erreur Mistral non
  transitoire (4xx hors 401/403/408/429) ; le texte reste alors en `second_opinion` jusqu'à
  l'admin. Une clé refusée (401/403) n'est pas un échec du job : voir [§6.4](#64-budget).
- Les scores bruts sont stockés pour **calibrer les seuils** (l'admin peut étiqueter des
  décisions, voir [§8](#8-administration-lan-uniquement), et les exporter au format du banc
  `docs/calibration/bench_screening.py`). Les probabilités ne sont pas
  calibrées d'origine : ne jamais coder de seuil en dur.
- Contributions et signalements passent par le même service avec leurs propres questions
  (pertinence par rapport à l'idée, respect, injection).

### 5.4 Doublons

- Après acceptation : embedding du texte (`mistral-embed`) stocké en `vector`, recherche du plus
  proche voisin parmi les idées publiées (distance cosinus, index HNSW).
- Similarité ≥ `DEDUP_SIMILARITY_THRESHOLD` → l'idée devient une **contribution** de l'idée
  existante (`merged`), le compteur « proposée N fois » augmente et l'idée parente est
  ré-analysée. Le lien de suivi de l'auteur pointe vers l'idée parente.
- Le seuil se calibre via l'admin (réglage `dedup.similarity_threshold`, §13.6, prioritaire sur
  la variable) ; en cas de doute, ne pas fusionner.
- Mise en œuvre (job `dedup_idea`, `jobs/dedup.rs`) : `accepted → dedup`, embedding du texte
  masqué (réutilisé s'il a déjà été calculé pour le même texte et le même modèle), recherche
  parmi les idées `published` uniquement, puis `merged` (contribution `doublon` acceptée de
  l'idée parente, sans texte : celui de l'idée fusionnée est fourni à la ré-analyse, déclencheur
  `merge`) ou `queued_analysis` (première analyse). Si l'embedding est impossible (erreur
  Mistral non transitoire autre qu'une clé refusée, ou dernière tentative du job), l'idée part
  en analyse sans dédoublonnage. Une clé refusée (401/403) ne fait jamais sauter le
  dédoublonnage : l'idée reste en `dedup` et le job attend une clé valide (§6.4).

### 5.5 Contributions (améliorations et objections)

Pas de commentaires libres. Sur une idée publiée, un visiteur peut soumettre une amélioration ou
une objection (même protection anti-abus que le dépôt, 30 à 2 000 caractères, mêmes deux
consentements). Elle est filtrée, puis, si acceptée, déclenche une **ré-analyse** de l'idée
qui l'intègre (job `analyze_idea`, déclencheur `contribution` ; un job déjà en file pour l'idée
absorbe les contributions suivantes). Le portail affiche le nombre de
contributions et le résumé neutre de celles qui ont été prises en compte, jamais le texte brut.

### 5.6 Signalements

Bouton « Signaler » sur chaque idée publiée : motif (erreur de chiffrage, source erronée,
contenu problématique, doublon, autre) + précision facultative. Le signalement est filtré, puis
traité **par IA** : Mistral réévalue l'idée avec le signalement (`prompts/report_review.md`) et
conclut `maintain`, `update` (nouvelle analyse) ou `hide` (masquée, visible dans l'admin), avec
un drapeau `escalate` pour les contenus graves (masquage immédiat et file d'escalade humaine,
§7.5). Les signalements d'une même idée sont regroupés : au plus une réévaluation par idée et
par 24 h, sauf `contenu_problematique`, traité sans délai. Formulaire et obligations : §7.5.
Mise en œuvre : un seul job `review_reports` par idée (clé `review_reports:<idée>`), dû 24 h
après `ideas.last_report_review_at`, avancé à l'instant par un signalement
`contenu_problematique`. Un signalement sans précision n'a pas de texte à filtrer : il est
accepté sans appel au modèle de décision.
Revue (`jobs/reports.rs`) : les signalements `accepted` de l'idée (50 au plus par revue) sont
examinés ensemble si la dernière revue date de plus de 24 h ou si l'un d'eux est
`contenu_problematique` (sinon le job attend l'échéance) ; la revue est enregistrée dans
`report_reviews` (avec `escalate`), les signalements passent `reviewed` (issue visible sur la
page de suivi), `ideas.last_report_review_at` est mis à jour. `update` met en file la
ré-analyse (déclencheur `report`, qui reçoit les `corrections`) ; `hide` masque l'idée publiée
(`hidden_reason = report_review`) et régénère les exports ; `escalate` force `hide`
(`hidden_reason = escalation`) et ajoute l'idée à la table `escalations` (`pending` puis `done`
par l'admin, une seule entrée en attente par idée). `ideas.hidden_reason` vaut
`report_review`, `escalation` (décisions automatisées) ou `admin`. Les signalements arrivés
pendant une revue relancent le job (aussitôt si `contenu_problematique`, 24 h plus tard sinon).

### 5.7 Déclencheurs de ré-analyse

Uniquement : contribution acceptée, doublon fusionné, signalement réévalué en `update`, ou
action admin. Pas de ré-analyse périodique ni de ré-analyse en masse lors d'un changement de
version de prompt (l'admin peut relancer manuellement).

---

## 6. Analyse approfondie par Mistral

### 6.1 Modèle et appels

- Modèle d'analyse : `mistral-large-latest` (variable `MISTRAL_ANALYSIS_MODEL`). Second avis et
  revue de signalement : même variable par défaut, surchargeable (`MISTRAL_REVIEW_MODEL`).
  Embeddings : `mistral-embed`.
- **Effort de raisonnement** (`reasoning_effort`, décision de l'éditeur) : `high` pour les deux
  étapes de l'analyse (`MISTRAL_ANALYSIS_REASONING_EFFORT`), `none` pour le second avis et la
  revue de signalement (`MISTRAL_REVIEW_REASONING_EFFORT`) ; valeurs admises `high`, `none`, ou
  vide (variable définie mais vide : paramètre omis, défaut de Mistral, non documenté). Jamais
  pour les embeddings. Envoyé au premier niveau en chat completions, dans `completion_args` en
  Conversations. Mistral ne le documente que pour certains modèles (`mistral-large-4-0`,
  `mistral-medium-3-5`, `mistral-small-latest` à la date de la décision ; le modèle par défaut
  `mistral-large-latest` peut le refuser). **Repli** (`ai::mistral`) : un appel qui l'a envoyé
  et reçoit HTTP 400 (ou 422) dont l'erreur le nomme (champs `message`, `param`, `code`, `loc`,
  `msg` du corps `{ object, message, type, param, code }` contenant `reasoning` ; jamais les
  `input` qui recopient la requête ; corps jamais journalisé) est renvoyé aussitôt sans le
  paramètre (hors des 3 tentatives) ; si ce second envoi reçoit une réponse (2xx), le modèle est
  retenu en mémoire pour la vie du processus (ses appels suivants partent directement sans le
  paramètre, un seul `WARN` par modèle et par processus : « model X rejects reasoning_effort,
  sending without it ») et dans la table `mistral_reasoning_refusals` (alerte
  `mistral_reasoning_unsupported` du tableau de bord, effort réellement envoyé publié par
  `/api/methodology`) ; sinon rien n'est retenu (le paramètre n'était pas en cause) et l'erreur
  suit son cours. Le premier appel accepté **avec** le paramètre efface la ligne (après un
  redémarrage, chaque processus le retente une fois). `ai_calls.reasoning_effort` enregistre
  l'effort réellement envoyé à chaque tentative (`NULL` après le repli). Les autres 400 gardent
  leur comportement (échec non transitoire, `analysis_failed`). Pour appliquer l'effort voulu,
  choisir un modèle qui le prend en charge ; pour s'en passer, vider la variable. Le worker
  journalise au démarrage modèles et efforts configurés. Délais
  par requête (`ai::mistral::Timeouts`) : chat 180 s et Conversations 300 s, portés à 900 s et
  1 200 s avec l'effort `high` (la réflexion allonge la réponse ; une recherche détaillée prend
  15 min ou plus, et des délais de 480 s pour la recherche et de 360 s pour la structuration,
  qui cite jusqu'à 100 pages, ont coupé des appels en production le 2026-10-19) ; le verrou du job est
  entretenu pendant ce temps (heartbeat, `JOB_LOCK_TIMEOUT_SECS`).
- **Recherche web obligatoire** pour le chiffrage (outil `web_search` de l'API Agents /
  Conversations de Mistral). Vérifier la documentation Mistral en vigueur au moment de
  l'implémentation (noms d'outils, sortie structurée). Le modèle décide lui-même d'appeler
  l'outil (guide web search : « autonomously decides whether to invoke the search ») ;
  `completion_args.tool_choice` (`auto|none|any|required`) figure bien dans le schéma
  `CompletionArgs` des Conversations, mais seule la doc des chat completions dit que `any` /
  `required` force un appel d'outil, et rien n'en décrit l'effet sur les connecteurs intégrés,
  exécutés côté serveur dans une boucle outil → modèle (relevé le 2026-10-19). Il n'est donc
  pas envoyé : le prompt `analysis_research.md` (1.2.0) exige la recherche (« Tu DOIS utiliser
  l'outil de recherche web avant de répondre ») et une recherche sans page est redemandée
  (§6.2, `sources`).
- Sortie **JSON validée contre un schéma** côté Rust. Si la recherche web et la sortie
  structurée ne sont pas compatibles dans un même appel : étape 1 recherche (texte + sources),
  étape 2 structuration via `response_format` JSON schema. Toute sortie invalide est rejetée et
  réessayée (3 tentatives max, backoff exponentiel), puis `analysis_failed`.
- Réponses avec raisonnement : le `content` d'un message (chat completions comme sorties
  `message.output` des Conversations) peut être une chaîne ou une liste de fragments. Seuls les
  fragments `text` forment la réponse (concaténés dans l'ordre ; le JSON de la sortie structurée
  n'est lu que là) ; un fragment `thinking` n'est **jamais** lu, stocké ni journalisé (il peut
  reprendre le texte d'un visiteur), seuls les `tool_reference` qu'il contient comptent parmi les
  pages consultées par la recherche web. Une réponse sans fragment `text` est une sortie
  invalide. Les tokens de raisonnement sont des tokens de sortie :
  `max(completion_tokens + completion_tokens_details.reasoning_tokens, total_tokens − entrée)`,
  pour que le coût estimé reste un majorant (§6.4).
- **Pages consultées** (formes de la spec OpenAPI vérifiées le 2026-10-19 :
  `ConversationResponse.outputs` = `MessageOutputEntry`, `ToolExecutionEntry` — `name`,
  `arguments`, `info` libre —, `FunctionCallEntry`, `AgentHandoffEntry` ; `ToolReferenceChunk` =
  `tool`, `title`, `url`?, `favicon`?, `description`?, plus `source` dans le guide web search ;
  `ThinkChunk.thinking` = `TextChunk`, `ToolReferenceChunk`, `ReferenceChunk` à
  `reference_ids` seuls) : une page est citable par un fragment `tool_reference`, dans ou hors
  d'un `thinking`, puis par les **résultats des recherches** : dans l'`info` (non documenté) des
  entrées `tool.execution` dont le `name` commence par `web_search`, tout objet portant une
  adresse http(s) dans `url` (à défaut `link`, `href`, `uri`, `source_url`) est une page, titrée
  par `title` (à défaut `name`) ; un JSON écrit en chaîne est lu aussi (`ai::mistral::
  collect_search_pages`, 100 pages au plus, profondeur et nombre de valeurs bornés). Relevé en
  production le 2026-10-19 : `mistral-large-4` avec l'effort `high` ne renvoie **aucun**
  `tool_reference`, ses pages sont dans `info.result` (12 à 15 recherches, toutes analyses en
  échec « consulted no page » auparavant). Pages dédoublonnées par la clé de comparaison
  d'INJ-3, celles des `tool_reference` d'abord. Le `type` d'une entrée ou
  d'un fragment a une valeur par défaut dans la spec sans être obligatoire : absent, il se déduit
  des champs requis (`content` ; `name` + `arguments` ; `thinking` ; `text` ; `tool` + `title` ;
  `reference_ids`). Le texte, la réflexion, les `reference` et les autres champs d'un résultat
  (extrait, snippet…) ne donnent jamais de page : l'outil de recherche écrit ces pages, pas le
  modèle. Après chaque réponse de recherche, **une** ligne `INFO` « web research answer structure »
  décrit la structure seule (modèle, effort envoyé, nombres d'entrées par type, de fragments
  `text`/`thinking`/`tool_reference` — dont dans la réflexion —/`reference`/autres, de contenus
  en chaîne, de recherches, de pages distinctes citables — `consulted_pages` — dont tirées des
  résultats — `search_result_pages` —, noms des clés de `info` assainis, et
  `execution_info_shape` : types, longueurs de listes et noms de champs de l'`info` de la
  première recherche, une clé qui n'est pas un identifiant simple devenant `?`), jamais un
  texte, un titre ou une URL ; `ai_calls.consulted_pages` garde le nombre de pages
  (0 sans recherche web comptée : le modèle n'a pas cherché ; 0 avec recherches : forme non lue,
  à lire dans `execution_info_shape`).
- 429 / 5xx : retry avec backoff ; ne jamais boucler sans limite.
- Mise en œuvre (job `analyze_idea`, `jobs/analysis.rs`, stockage `db/analyses.rs`) :
  - première analyse : `queued_analysis → analyzing → published` (slug tiré du titre, suffixe
    `-2`, `-3`… en cas de collision, jamais modifié ensuite) ; sortie encore invalide après les
    3 tentatives du client, erreur Mistral non transitoire (hors clé refusée, §6.4) ou dernière
    tentative du job : `analysis_failed` (motif `analysis_failed`) et job `dead` ;
  - ré-analyse d'une idée `published` ou `hidden` : entrées = analyse courante, contributions
    `accepted` (20 au plus par analyse, un doublon fusionné y figure avec le texte de l'idée
    fusionnée) et corrections des revues `update` non encore appliquées
    (`report_reviews.applied_in_analysis_id`) ; sans entrée nouvelle, le job ne fait rien (sauf
    déclencheur `admin`) ; l'idée garde son statut, son slug et son analyse jusqu'au
    remplacement ; un échec laisse l'analyse publiée en place ;
  - chaque analyse est une nouvelle ligne `analyses` (version + 1, historique conservé) avec
    `prompt_version` (`analysis.md`), `research_prompt_version`, `schema_version` (empreinte
    `sha256:` des 12 premiers chiffres hexadécimaux du schéma de sortie), `scoring_version`,
    modèle et nombre de recherches web ; les colonnes de publication sont recopiées dans
    `ideas`, les contributions passent `integrated` avec le résumé neutre de l'analyse,
    `ideas.contribution_count` compte les contributions intégrées hors doublons ;
  - le budget est vérifié avant chaque appel (l'idée reste `analyzing` jusqu'au lendemain) ;
    des entrées arrivées pendant l'analyse la relancent aussitôt (même clé de dédoublonnage) ;
  - une publication programme la régénération des exports (job `generate_exports`, clé
    `generate_exports`, différé de 2 min pour regrouper les changements) ; un masquage (quel
    qu'en soit le motif, escalade comprise), un retrait ou une suppression par l'auteur la
    demande **aussitôt**, sans regroupement (§7.4).

### 6.2 Contenu de l'analyse

| Champ | Contenu |
|---|---|
| `title`, `summary` | Reformulation **neutre et factuelle** (le texte original reste consultable) |
| `improved_proposal` | Version améliorée et précisée de l'idée |
| `admin_level` | `etat`, `securite_sociale`, `region`, `departement`, `intercommunalite`, `commune`, `multi` |
| `territory` | Facultatif (nom + code INSEE si une collectivité précise est visée) |
| `category` | Catégorie de `prompts/taxonomy.yaml` (missions budgétaires de l'État, fonctions des collectivités) |
| `kind` | `savings` (baisse de dépense), `revenue` (hausse de recette), `mixed` |
| `annual_gain_eur` | `{ low, median, high }` gain brut annuel en régime de croisière |
| `implementation_cost_eur` | `{ low, median, high }` coût ponctuel de mise en œuvre |
| `time_to_effect_months` | Délai avant les premiers effets |
| `legal_vehicle` | `loi`, `loi_de_finances`, `decret`, `arrete`, `deliberation`, `circulaire`, `organisation_interne`, `autre` |
| `feasibility` | 1 à 5 + obstacles |
| `confidence` | `low`, `medium`, `high` (qualité des données disponibles) |
| `assumptions` | Hypothèses de calcul, explicites |
| `pros`, `cons`, `risks` | Listes courtes et factuelles |
| `impacts` | Pour **chaque** public de `taxonomy.yaml` (ménages modestes, classes moyennes, ménages aisés, entreprises, agents publics, usagers des services publics, territoires ruraux, territoires urbains, environnement) : `positive` / `neutral` / `negative` / `uncertain` + explication |
| `sources` | Publiées `{ id, title, publisher, url, used_for }` ; au moins une source pour tout chiffre clé, citée `[S1]`, `[S2]`… dans les textes. **Citation par identifiant (INJ-3)** : les pages renvoyées par la recherche web de l'analyse (`tool_reference`, puis résultats des exécutions `web_search`, §6.1), puis les sources de l'analyse remplacée, sont numérotées `R1`, `R2`… dans `{{pages}}` d'`analysis.md` (« Rn — titre — site — adresse », une page par clé de comparaison : sans schéma, port par défaut, fragment, `/` final ni paramètres `utm_*` ; écartées : identifiants, port explicite, adresse IP, plus de 2 048 caractères, caractère de contrôle) ; la réponse du modèle ne porte que `{ id, ref, used_for }` (`ref` = `Rn`, restreint par un `enum` ajouté au schéma envoyé pour l'appel, `OutputSchema::constrained`) et le backend remplit `title` (titre de la page nettoyé, 500 caractères au plus, à défaut le site), `publisher` (site : nom d'hôte sans `www.`) et `url` depuis cette liste (`ai::outputs::ConsultedPages`, `AnalysisDraft::resolve`) : le modèle n'écrit jamais d'URL. Un `ref` inconnu ou un `[Sx]` cité sans source est refusé et la tentative suivante reçoit après le prompt un bloc `<retour_validation>` qui liste les identifiants valides (construit à partir d'identifiants validés seulement, jamais de texte du modèle). Les analyses publiées avant gardent leur format (mêmes champs), lu tel quel. Une recherche qui ne renvoie aucune page (le modèle décide lui-même de chercher) est redemandée une fois dans la même exécution ; toujours aucune page et rien à citer de l'analyse remplacée : le job est relancé par la file (erreur transitoire, tentative comptée, backoff), puis la dernière tentative échoue (`analysis_failed`, motif « the web research consulted no page… ») ; jamais d'appel de structuration ni de publication sans page à citer |
| `existing_situation` | Droit et pratique actuels, mesures proches déjà en vigueur (le gain chiffré est le gain **supplémentaire**) |
| `confidence_explanation` | Justification du niveau de confiance |
| `contributions_considered` | Pour chaque contribution intégrée : `{ contribution_id, summary, taken_into_account, response }` (résumé neutre affiché sur le portail, §5.5) |
| `changes_since_previous` | Ce qui change par rapport à l'analyse précédente (vide pour une première analyse) |

Les fourchettes `annual_gain_eur` et `implementation_cost_eur` portent aussi un champ
`calculation` (calcul détaillé avec références aux sources), et `feasibility` vaut
`{ score: 1..5, obstacles: [...] }`. `territory` vaut `{ name, insee_code }`, deux chaînes vides
si aucune collectivité précise n'est visée (le mode strict de Mistral exclut les champs
facultatifs).

Règles de prompt : ne jamais qualifier politiquement (gauche, droite, libéral, social…), ne
jamais nommer de parti ni d'élu, présenter pour et contre avec le même niveau de détail, préférer
les sources publiques (Cour des comptes, INSEE, PLF/PLFSS, DREES, rapports parlementaires,
comptes des collectivités).

### 6.3 Score composé

Implémenté **une seule fois** dans `backend/src/domain/scoring.rs`, testé, versionné
(`SCORING_VERSION`), recalculé à chaque analyse :

```
net_annual   = annual_gain.median − implementation_cost.median / 5   (coût amorti sur 5 ans)
feasibility  = feasibility / 5
confidence   = { low: 0.5, medium: 0.75, high: 1.0 }
delay        = 1 / (1 + time_to_effect_months / 24)
score        = net_annual × feasibility × confidence × delay
```

Le portail trie par score par défaut et permet de trier par gain médian, faisabilité, délai,
et de filtrer par niveau administratif, territoire, catégorie et type. Les composantes du score
sont affichées sur chaque idée. Aucun ajustement manuel du score d'une idée.

### 6.4 Budget

- Chaque appel IA est enregistré (`ai_calls` : usage, modèle, tokens, nombre de recherches web,
  coût estimé, version de prompt, idée, latence, statut, effort de raisonnement envoyé —
  `reasoning_effort`, `NULL` s'il est omis, refusé par le modèle (§6.1) ou sans objet ; pages
  citables d'une recherche web — `consulted_pages`, `NULL` pour les autres appels). Le tableau des coûts de l'admin
  ventile par usage, fournisseur et effort de raisonnement.
- Plafond journalier `MISTRAL_DAILY_BUDGET_EUR` (prix unitaires en variables d'environnement
  car ils changent). Plafond atteint : les jobs restent en file jusqu'au lendemain (minuit
  Europe/Paris) ; les seconds avis attendent aussi (l'auteur voit « en cours de vérification »).
- **Clé refusée** (HTTP 401 ou 403 de Mistral) : erreur de **configuration**, traitée comme une
  clé absente, jamais comme un échec (`jobs/guard.rs`, `db/provider_alerts.rs`). Le client
  enregistre l'alerte `mistral_auth` (table `provider_alerts` : début, dernier refus, statut
  HTTP, nombre de refus) et journalise **un seul** événement `ERROR` par heure (jamais un par
  job, jamais la clé) ; le job (analyse, dédoublonnage, second avis, revue de signalement) est
  replanifié sans consommer de tentative ni changer d'étape, 15 min plus tard puis jusqu'à 1 h
  (un quart de l'âge de l'alerte). Pendant l'alerte, un seul job par période appelle Mistral
  pour vérifier la clé, les autres sont replanifiés sans appel ; l'api ne tente plus le second
  avis en ligne (le job en file prend le relais). Le premier appel qui franchit
  l'authentification (réponse, sortie invalide, autre 4xx) clôt l'alerte. Le tableau de bord
  admin l'affiche (`alerts`, « Clé Mistral refusée depuis le … ») avec le bouton « Vérifier la
  clé Mistral maintenant » (`POST /api/admin/provider-alerts/mistral/check`, §13.2) : un appel
  `GET /v1/models`, qui ne facture rien, au lieu d'attendre jusqu'à 1 h le prochain job autorisé
  à vérifier ; acceptée, l'alerte est close et les jobs Mistral qui attendaient (échéance dans
  l'heure) deviennent dus aussitôt. Ce contrôle n'est pas enregistré dans `ai_calls` (aucun
  token, aucun coût, aucun usage du pipeline : la contrainte `purpose` n'a pas à s'élargir) ; il
  est journalisé dans `admin_actions` (`provider_alert.mistral_check`). Les idées passées en
  `analysis_failed` avant la correction se relancent en masse
  (`POST /api/admin/ideas/reanalyze-failed`, §13.2). Remplacer la clé (secret Docker immuable) :
  `docs/ops.md` §2.1.
- Priorité de la file : second avis > signalement > nouvelle idée > contribution > relance admin
  (au-dessus de tout : la régénération des exports après un masquage, un retrait ou une
  suppression, §7.4 ; en dessous : purge et autres régénérations).
- QA : plafond bien plus bas (dans `.env.qa`).

### 6.5 Versionnement et méthodologie publique

- Chaque fichier de `prompts/` porte une version (`version:` en tête). Chaque décision et chaque
  analyse stockent les versions de prompt, de grille et de score utilisées.
- La page publique « Méthodologie » affiche les prompts, la grille de filtrage, la taxonomie,
  la formule de score, les modèles Mistral et l'effort de raisonnement envoyé à chacun (§6.1)
  **tels qu'utilisés en production** (lus depuis l'API, pas recopiés).

---

## 7. Anonymat, anti-abus et données

### 7.1 Anonymat

- Aucune donnée personnelle collectée : pas de compte, pas d'email, pas d'IP, pas de user-agent
  stocké, pas de cookie.
- **Lien de suivi** : à la soumission, un jeton aléatoire de 256 bits (base64url) est renvoyé
  **une seule fois** ; seul son hash SHA-256 est stocké. L'interface invite l'auteur à le
  conserver. Le même mécanisme vaut pour contributions et signalements.
  - Le jeton est placé dans le **fragment** de l'URL (`/suivi#<jeton>`) et envoyé à l'API dans
    le **corps** d'un `POST` : il n'apparaît jamais dans une URL de requête, donc jamais dans les
    journaux de Traefik ou de nginx. La page de suivi envoie `Referrer-Policy: no-referrer`.
  - La page de suivi propose « Supprimer ma contribution » (effacement immédiat, ou retrait
    pour une idée publiée ou masquée ; retrait des exports suivants) et « Demander un réexamen
    humain » (file admin).
  - Un refus affiche l'exposé des motifs (DSA art. 17) : mesure prise, règle en cause (article
    de `/regles`), « décision prise automatiquement par une IA », date de purge, voies de recours
    (réexamen humain, contact, juge).
  - Jeton : format vérifié (43 caractères base64url) avant hachage, recherche par son SHA-256
    (clé primaire) ; jeton inconnu, expiré ou mal formé : même `404 tracking_not_found`. Les
    réponses ne contiennent aucun identifiant interne et portent `Cache-Control: no-store`.
  - **Retrait** d'une idée `published` ou `hidden` (`db::tracked`, décision D2), dans une
    transaction : `→ withdrawn` (§5.2) ; rien n'est effacé ; la demande de réexamen de l'idée,
    les jobs en file de l'idée et de ses contributions et signalements sont supprimés, les
    exports régénérés ; une escalade en attente reste dans la file admin (§7.5). Le jeton de
    l'auteur devient une pierre tombale (`status: deleted`, 30 jours) ; ceux des autres
    visiteurs (contributions, signalements, doublons fusionnés) répondent avec leur statut,
    `idea: null` et `idea_withdrawn: true` (« idée retirée par son auteur »), et un réexamen
    humain n'y est plus possible (`409 review_not_available`).
  - **Suppression** (`db::tracked`) de tout autre texte, immédiate et dans une transaction :
    idée non publiée → la ligne et tout ce qui s'y rattache (historique, analyses, embedding, contributions et signalements
    d'autres visiteurs, doublons fusionnés ; leurs jetons deviennent inconnus), un doublon
    fusionné quitte aussi l'idée parente (compteur de propositions décrémenté, résumé de son
    texte retiré de `contributions_considered` de toutes ses analyses, exports régénérés), une
    escalade en attente (cas défensif : une idée escaladée est masquée, donc retirée et non
    effacée) part avec l'idée mais laisse dans le journal admin l'action
    `escalation.deleted_by_author` (identifiants de l'idée et de l'escalade, date, aucun texte) ;
    contribution → la ligne, et son résumé est retiré de `contributions_considered` des
    analyses (exports régénérés si elle était intégrée) ; signalement → la ligne et les
    coordonnées du signalant. Dans tous les cas : demande de réexamen, jobs en file et
    explications de second avis effacés ; les scores du modèle de décision restent, sans
    texte. Le jeton de l'auteur devient une pierre tombale (`status: deleted`) pendant 30 jours.
  - **Réexamen humain** : possible une fois par texte, seulement contre une décision
    automatique (refus ou masquage) ; message facultatif (1 000 caractères, masqué), table
    `review_requests` (`pending` puis `done` par l'admin). Tant qu'il est en attente, le texte
    refusé n'est pas purgé (`purge_on` vaut `null`). L'admin le clôt avec `resolution` :
    `confirmed` (la décision reste, la purge reprend) ou `overturned` : un texte refusé passe
    `rejected → accepted` (champs de refus effacés, dernière décision de filtrage étiquetée
    `accept` pour la calibration si elle ne l'était pas) et reprend le pipeline (dédoublonnage,
    ré-analyse ou revue de signalement, `moderation::overturn_rejection`) ; une idée masquée est
    republiée (exports régénérés). Note facultative de l'admin (`note`), jamais montrée à
    l'auteur.
- Logs applicatifs : jamais d'IP, de jeton, d'en-têtes d'identification ni de texte d'idée
  (seulement des identifiants internes).

### 7.2 Anti-abus

- **ALTCHA** (proof-of-work auto-hébergé, clé `ALTCHA_HMAC_KEY`) sur dépôt, contribution et
  signalement. Aucun captcha tiers. Protocole ALTCHA v1 (widget `altcha` 2.x) : défi
  `GET /api/altcha/challenge` (SHA-256, `maxnumber` 100 000, valable 20 min, expiration signée
  dans le sel) ; la solution (base64 du widget) n'est acceptée qu'une fois, les signatures
  utilisées restant en mémoire jusqu'à leur expiration.
- **Rate limit** applicatif par `HMAC(sel_du_jour, ip)` : le sel est aléatoire, gardé **en
  mémoire uniquement** (jamais en base, sur disque, dans un log ou une sauvegarde), renouvelé
  toutes les 24 h puis détruit ; les compteurs vivent aussi en mémoire et expirent en 24 h au
  plus. L'api tourne en une réplique ; avec plusieurs répliques, la limite s'appliquerait par
  réplique (acceptable). Les tables `rate_limit_salts` / `rate_limit_counters` du schéma
  initial ne sont pas utilisées. L'IP en clair n'est jamais persistée. L'IP cliente provient de
  `X-Forwarded-For` posé par Traefik (ports publiés en mode `host` : Traefik voit l'IP réelle).
- Aucune empreinte HMAC n'est stockée avec une idée, une contribution ou un signalement.
- Limites par défaut (configurables) : 5 idées / heure et 20 / jour, 20 contributions / jour,
  20 signalements / jour par empreinte. Fenêtres fixes ouvertes par la première requête
  comptée ; une IPv6 est comptée par préfixe /64 ; un refus répond `429` (`rate_limited`,
  message en français, `Retry-After`) et n'est pas compté ; une requête de formulaire dont
  l'ALTCHA est invalide n'est pas comptée non plus (`RateLimiter::peek` avant l'ALTCHA,
  `check` après). Mémoire bornée par action (`MAX_CLIENTS_PER_ACTION`, 50 000 empreintes) :
  table pleine → compteurs expirés retirés puis éviction par lots des plus anciens ; un nouveau
  visiteur n'est jamais refusé à cause des autres, et le suivi ne peut pas évincer les
  compteurs des formulaires.
- Suivi (`/api/track*`, toutes routes confondues) : 120 requêtes / heure et 600 / jour par
  empreinte (constantes `TRACKING_PER_HOUR` / `TRACKING_PER_DAY` de `antispam/rate_limit.rs`).
- Le WAF et le rate limit global de Traefik (`global@file`) s'appliquent en plus.

### 7.3 Rétention

- Idée refusée : texte et jeton purgés **30 jours** après le refus ; seules des statistiques
  agrégées sont conservées (date, motif, niveau, scores du modèle de décision pour la calibration
  sans le texte).
- Idées publiées : conservées avec leur texte original (version masquée, voir §7.5).
- Contenus masqués (`hidden`) : texte purgé après 12 mois (`hidden → purged`, sauf réexamen
  humain en attente) avec tout ce qui en dérive : titre, résumé, slug, territoire, contenu
  textuel des analyses (montants, catégorie et scores gardés pour les statistiques),
  explications des revues de signalement, embedding, jetons, droits de réponse, précision,
  coordonnées et jetons de ses signalements quel que soit leur statut ; ses contributions sont supprimées
  et les doublons qui y ont été fusionnés purgés (`merged → purged`). L'exposé des motifs du
  masquage donne la date de purge.
- Idées retirées par leur auteur (`withdrawn`, §5.2) : même purge que les contenus masqués,
  **12 mois** après le retrait (`withdrawn → purged`) ; `withdrawn_at` reste (une idée retirée
  n'est jamais recomptée dans les statistiques publiques).
- Signalements : nom et e-mail facultatifs du signalant effacés 6 mois après le signalement ;
  précision et jeton d'un signalement traité (`reviewed`) effacés 6 mois après la revue ; ceux d'un
  signalement resté sans décision (`submitted`, `screening`, `second_opinion`, `accepted` : job
  mort) effacés 6 mois après le signalement, son statut restant inchangé.
- Contribution supprimée par son auteur : effacement immédiat (§7.1) ; idée publiée ou masquée
  supprimée par son auteur : retrait (ci-dessus).
- Demandes de réexamen humain : supprimées avec leur texte, et un an après leur traitement.
- Copies scellées des contenus escaladés (§7.5, décision D3) : supprimées **un an après
  l'escalade** par la purge quotidienne (`escalation_seals.purge_after`), et jamais avant, même
  si l'idée a été retirée ou supprimée par son auteur ou purgée entre-temps.
- Job de purge quotidien dans le worker, testé.
- Sauvegardes de la base : **30 jours** au plus (mémo juridique) ; aucune n'est automatisée à ce
  jour, procédure et script dans `docs/ops.md` §5.3. Journaux d'accès de Traefik : IP tronquée,
  retirée ou 7 jours au plus (§7.5, `docs/ops.md` §5.1).

### 7.4 Export

Export JSON et CSV des idées publiées et de leurs analyses (téléchargement depuis le portail).
Pas d'API publique documentée au MVP.

- **Licence : Licence Ouverte 2.0 (Etalab)**, SPDX `etalab-2.0`, compatible CC BY 4.0, « dans
  la mesure où des droits existent » ; contenus de tiers cités exclus. Concédant : l'éditeur
  d'iapolitique.fr (les contributeurs lui concèdent leurs droits via les CGU, §7.5).
- Chaque export contient un fichier `LICENSE` (texte LO 2.0) et des métadonnées : licence,
  producteur, `generated_at`, version du schéma d'export, avertissement IA, exclusion des
  contenus de tiers, mention « données personnelles masquées », avertissement d'irrévocabilité
  (une copie déjà téléchargée ne peut pas être retirée) et invitation à se resynchroniser.
- Par idée : texte original (`source_type=user`, `attribution="Contribution anonyme –
  iapolitique.fr"`), champs générés marqués (`ai_generated=true`, `ai_model`, `ai_provider`,
  `generated_at`, `analysis_version`, `human_reviewed=false`), `date_derniere_mise_a_jour` et
  URL canonique. Sources : titre, éditeur, URL, date, sans extrait long.
- **Exclus** : idées refusées, masquées, retirées par leur auteur ou en attente, jetons et hash,
  journaux de filtrage,
  signalements, coordonnées, horodatages fins (dates au jour près).
- Régénéré chaque nuit, après toute publication et **aussitôt** après tout masquage, retrait
  ou suppression : une idée masquée ou retirée ne reste jamais téléchargeable ; fichiers datés
  et versionnés.
- Mise en œuvre (`jobs/exports.rs`, `db/exports.rs`) : job `generate_exports` chaque nuit à
  04:15 (Europe/Paris) et 2 min après toute publication ou ré-analyse (clé `generate_exports`,
  `pipeline::exports_job` : les changements arrivés entre-temps sont regroupés). Un masquage
  (admin, revue de signalement, escalade), un retrait ou une suppression par l'auteur appelle
  `pipeline::regenerate_exports_now` dans sa transaction : une génération en file devient due
  aussitôt avec la priorité la plus haute (`priority::URGENT_EXPORTS`), sinon un job urgent sans
  clé est créé (une génération déjà en cours a pu lire l'idée avant le changement). Avant
  d'écrire ses fichiers, une génération verrouille (`FOR SHARE`) les idées qu'elle exporte et
  vérifie qu'elles sont toutes encore publiées (`db::exports::still_published`) ; sinon elle
  recommence (3 fois, puis le job repasse 10 s plus tard sans consommer de tentative) : aucune
  génération enregistrée ne contient une idée qui n'était plus publiée à ce moment. Une
  génération écrit quatre lignes dans `exports` (même
  `generation_id`, `export_schema_version` = `EXPORT_SCHEMA_VERSION`, actuellement `1.0`) :
  `json` (`ideas.json` : `metadata`, `licence_text`, `ideas`), `csv` (`ideas.csv`, colonnes
  fixes, listes jointes par « | »), `license` (texte officiel Etalab embarqué depuis
  `backend/assets/licence-ouverte-2.0.md`) et `metadata` (`metadata.json`). Par idée :
  `slug`, `url` (`https://{TRAEFIK_HOST}/idees/{slug}`, `iapolitique.fr` par défaut),
  `date_publication`, `date_derniere_mise_a_jour`, `proposal_count`, `contribution_count`,
  `original` (`text` masqué, `source_type`, `attribution`, `personal_data_masked`), `analysis`
  (champs du §6.2 sauf les identifiants de contributions, marquage IA, versions ; sources
  réduites à `id`, `title`, `publisher`, `url`, `consulted_on`), `score` (composantes,
  « score calculé par algorithme »), `contributions` (résumés neutres intégrés). Aucun
  identifiant interne. Les exports sont servis depuis la base (dernière génération par
  format) ; la purge garde 7 jours d'exports remplacés.
- Page `/licence` : licence, compatibilité CC BY 4.0, mention d'attribution prête à copier
  (« Source : iapolitique.fr – Propositions citoyennes et analyses générées par IA (Mistral AI),
  mise à jour du AAAA-MM-JJ – Licence Ouverte 2.0 »), avertissement IA.

### 7.5 Conformité juridique

Référence : [docs/juridique/memo.md](docs/juridique/memo.md) (décisions du 2026-10-18). Ce
n'est pas un avis juridique : ne pas trancher les points de sa section 5 sans l'humain.

- **Éditeur** : personne physique, identité publiée (LCEN art. 1-1). Ses coordonnées ne sont
  **jamais** dans git (dépôt public) : variables `LEGAL_PUBLISHER_NAME`,
  `LEGAL_PUBLISHER_ADDRESS`, `LEGAL_PUBLISHER_PHONE`, `LEGAL_CONTACT_EMAIL`,
  `LEGAL_PRIVACY_EMAIL`, servies par `GET /api/legal` et affichées par les pages légales.
  Site auto-hébergé : l'hébergeur est l'éditeur lui-même.
- **Pages** (liées dans le pied de page de toutes les pages) : `/mentions-legales`,
  `/confidentialite`, `/regles` (CGU : critères numérotés d'acceptation et de rejet, modération
  algorithmique d3-mini + Mistral sans relecture humaine systématique, mesures, durées, recours,
  paramètres du score, clause de licence, interdits), `/licence`. Modèles de texte : sections 1 à
  3 du mémo. Chaque page affiche sa date de mise à jour.
- **Formulaires publiables** (idée, contribution) : deux cases **non pré-cochées**, envoi
  bloqué sans elles : acceptation de la clause de licence des CGU, et « je comprends que mon
  texte sera publié s'il est accepté ». Avertissement « Ne signez pas, ne nommez aucun agent ni
  particulier » et mention du traitement par IA (modèle local puis Mistral AI). On stocke
  `cgu_version` (version courante : `domain::rules::CGU_VERSION`, à incrémenter avec la page
  `/regles` quand ses clauses changent : 1.1 le 2026-10-19 pour le retrait par l'auteur D2, la
  copie scellée D3 et l'interdiction explicite de nommer un parti ou un élu D1) et `cgu_accepted_at` (date Europe/Paris) avec chaque idée et contribution, rien
  d'autre. Sans les deux cases : `400 consent_required`.
- **Données personnelles dans les textes** : avant tout appel à Mistral et avant publication,
  masquage déterministe (e-mails, téléphones, IBAN, NIR, adresses postales, URL nominatives →
  `[donnée masquée]`), appliqué dès la réception : le texte non masqué n'est **jamais**
  conservé ni transmis, le filtrage d3-mini porte lui aussi sur la version masquée ; et critères de filtrage `personal_data` et `contact_data` (§5.3) qui refusent un texte
  désignant un particulier identifiable ou portant un nom ou une coordonnée que le masquage a
  manqués (auteur compris), avec invitation à reformuler. Les élus et ministres
  agissant dans leur fonction ne sont pas des particuliers, mais le critère `targeted` refuse
  tout texte qui cite un parti, un candidat ou un élu, même de façon neutre (décision D1), avec
  invitation à reformuler ; les fonctions et institutions (« le Gouvernement », « les élus
  locaux ») restent admises. Le texte « original »
  affiché est la version masquée, avec la mention « données personnelles masquées ».
- **Transparence IA (AI Act art. 50, applicable depuis le 2026-08-02)** : avant chaque bloc
  généré, l'étiquette visible et accessible « Analyse générée automatiquement par intelligence
  artificielle (Mistral AI), sans relecture humaine. Elle peut contenir des erreurs ; vérifiez
  les sources citées. Ce n'est pas une position officielle. » ; marquage lisible par machine
  (`data-ai-generated="true"` et JSON-LD) dans le HTML et les exports. Le texte de l'internaute
  est séparé visuellement sous « Contribution d'un internaute (texte original, données
  personnelles masquées) ». Le score est présenté comme « score calculé par algorithme », jamais
  comme un sondage ou une mesure d'opinion (vigilance renforcée du 2027-01-01 au second tour de
  la présidentielle).
- **Site non officiel** : mention persistante « Site citoyen indépendant et non officiel, sans
  lien avec l'État ni aucune administration ». Interdits : Marianne, bloc-marque, DSFR, codes
  visuels des sites .gouv.fr, vocabulaire administratif trompeur (« dossier », « validé par »).
- **Signalement** (DSA « léger », LCEN art. 6) : motif, précision, nom et e-mail **facultatifs**,
  déclaration de bonne foi, rappel de la sanction des signalements abusifs (LCEN art. 6 VII),
  accusé de réception sur la page de suivi, mention du traitement automatisé. Sans déclaration
  de bonne foi : `400 good_faith_required`. Le nom et l'e-mail ne sont stockés qu'avec le
  signalement (`reports.reporter_name`, `reporter_email`), jamais publiés, exportés, journalisés
  ni transmis à une IA ; la précision est masquée comme les autres textes.
- **Escalade humaine** : la revue de signalement marque `escalate` les contenus graves
  (diffamation, menace, haine, terrorisme, pédocriminalité) : masquage immédiat et file admin
  « escalade » (transmission manuelle à PHAROS, cible 24 h) ; l'admin la clôt avec `outcome`
  (`transmitted` ou `dismissed`) et une note facultative ; l'idée reste masquée (republication
  séparée). Si l'auteur supprime l'idée avant la clôture, elle est retirée (§5.2) et l'entrée
  reste dans la file admin jusqu'à sa clôture.
- **Copie scellée** (décision du propriétaire D3, 2026-10-19) : à la création de l'escalade, dans
  la même transaction (`db::escalation_seals::seal`), une copie de ce qui est escaladé est
  scellée dans `escalation_seals` : texte original masqué, territoire, titre, résumé, slug,
  dates de dépôt, de publication et de masquage, contenu de l'analyse courante, revue
  (`outcome`, `escalate`, explication, modèle, version de prompt) et signalements qui l'ont
  déclenchée (motif et précision masquée ; **jamais** les coordonnées du signalant). Une copie par
  escalade, sans clé étrangère : elle survit au retrait ou à la suppression de l'idée par son
  auteur et à la purge de 12 mois, et est supprimée un an après l'escalade (§7.3). Base légale
  (à mentionner dans `/confidentialite`) : RGPD art. 17(3)(e) (constatation, exercice ou défense
  de droits en justice) et obligations de la LCEN. Réservée à l'admin : lue par
  `GET /api/admin/escalations/{id}/seal`, chaque lecture journalisée dans `admin_actions`
  (`escalation.seal_read`) ; jamais publique, jamais exportée. La migration `0016` a scellé les
  escalades de l'année précédente à partir de ce qui restait (`backfilled: true`).
- **Droit de réponse** (LCEN art. 1-1 III) : l'admin peut publier une réponse attachée à une
  idée (insertion sous 3 jours) : table `rights_of_reply` (`author_label`, `body` en texte
  brut, `requested_on`), affichée par `GET /api/ideas/{slug}`, retirée du site avec l'idée
  (retrait par l'auteur) et effacée avec elle (purge 12 mois après un masquage ou un retrait).
- **Mistral AI** : sous-traitant (art. 28 RGPD). Option « Anonymous improvement data »
  **désactivée** sur le compte (fait le 2026-10-18). Ne jamais utiliser de modèle « Labs » ou
  « Preview ». Ne jamais transmettre IP, empreinte ou jeton. Le point d'accès UE
  (`MISTRAL_API_BASE_URL`) est à confirmer ; la recherche web sort de la garantie régionale
  (mentionné dans la politique de confidentialité).
- **Journaux** : l'application ne collecte jamais l'IP en clair ; nginx sans IP. Les journaux
  d'accès Traefik (infrastructure partagée) doivent tronquer l'IP ou être gardés 7 jours au plus
  (voir `docs/ops.md`).
- **Stockage local navigateur** : uniquement le thème et, si l'utilisateur le demande, ses liens
  de suivi ; décrit dans `/confidentialite`. Aucun cookie, donc pas de bandeau.

---

## 8. Administration (LAN uniquement)

- Host dédié (`ADMIN_HOST`), routé par Traefik avec `dashboard-allowlist@file` vers le port
  admin 8081 de l'api. Ce port n'est **jamais** routé depuis Internet. Défense en profondeur :
  en-tête `Authorization: Bearer` avec `ADMIN_API_TOKEN`.
- Le SPA est le même sur les deux hosts : les pages `/admin` sondent d'abord
  `GET /api/admin/session` **sans** jeton et n'affichent le formulaire que si l'API admin répond
  (`401`, ou `429` si verrouillée). Sur le host public, le port public répond `404` : aucun
  formulaire, aucun jeton envoyé, un jeton déjà stocké est effacé, et la page indique que
  l'administration n'est disponible que sur l'adresse du réseau local
  (`frontend/src/hooks/use-admin-session.ts`). En amont, Traefik redirige (302) `/admin` et
  `/admin/…` du host public vers `https://ADMIN_HOST/admin…` (routeur
  `iapolitique-admin-redirect`, priorité 50, `global@file`, `noop@internal`) : les pages admin ne
  s'exécutent pas sur l'origine publique.
- Fonctions : tableau de bord (file de jobs, taux d'acceptation, coûts IA du jour/mois, alertes
  de configuration comme une clé Mistral refusée — avec vérification immédiate de la clé — ou un
  modèle qui refuse `reasoning_effort`), idées masquées et signalements,
  masquer/republier, relancer une analyse ou toutes les analyses échouées, consulter les copies
  scellées des escalades (`/admin/copies-scellees` ; la copie n'est demandée à l'API qu'après un
  clic explicite, jamais rechargée en arrière-plan, puisque chaque lecture est journalisée),
  idées retirées par leur auteur en lecture seule, consulter les scores bruts
  du modèle de décision, **étiqueter** des décisions (accepter/refuser à raison) pour calibrer
  les seuils, réglage des seuils de doublon et de filtrage.
- Toute action admin est journalisée (`admin_actions`).

---

## 9. Qualité et tests

À exécuter avant **chaque** commit (et par le service `test` de la CI) :

| Partie | Commandes |
|---|---|
| Backend | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` |
| Frontend | `npm run lint` (ESLint), `npm run typecheck` (`tsc --noEmit`), `npm test` (Vitest), `npm run build` |

- Tests unitaires obligatoires pour : machine à états, scoring, parsing/validation des réponses
  IA, seuils et zone grise, rate limit, purge.
- Tests d'intégration backend contre un PostgreSQL éphémère (conteneur `pgvector/pgvector:pg18`
  dans le service `test`) ; les appels decision service et Mistral sont **mockés** (jamais
  d'appel réel ni payant en test).
- Tout bug corrigé arrive avec un test qui le reproduit, dans le même commit.
- Front : tests de composants avec Testing Library sur les parcours clés (dépôt, suivi,
  exploration, contribution, signalement).

---

## 10. Déploiement (PulsarCD / Docker Swarm)

Référence : `C:\Repos\PulsarCD\docs\PROMPT_PROJECTS.md`. Le déploiement passe **uniquement**
par PulsarCD (MCP `pulsarcd-actions` / `pulsarcd-read`), jamais par un `docker stack deploy`
manuel. PulsarCD construit et teste chaque commit de `main` ; la promotion QA → production est
décidée par l'humain.

### 10.1 `devops/docker-compose.swarm.yml`

- Images : `registry.methodinfo.fr/iapolitique-api`, `registry.methodinfo.fr/iapolitique-frontend`
  (le worker réutilise l'image api avec `command: ["worker"]`). Sections `build:` avec
  `context: ..`.
- Services : `api`, `worker`, `frontend`, et `test` (`replicas: 0`, exécuté par PulsarCD via
  `docker compose run --rm test`, avec le PostgreSQL éphémère `test-db`, voir [§13.5](#135-tests-en-ci)).
- Réseaux externes : `proxy` (api, frontend), `postgresqlcluster_internal` (api, worker), et le
  réseau LLM `${LLM_NETWORK:-llm-net}` (api, worker). En QA, PulsarCD préfixe lui-même le nom des
  réseaux externes (`llm-net` devient `qa-llm-net`) : ne **pas** définir `LLM_NETWORK` dans
  `.env.qa` (on obtiendrait `qa-qa-llm-net`).
- Traefik : routeurs HTTPS avec `global@file` **obligatoire** sur tout ce qui est public ;
  redirection HTTP→HTTPS ; `PathPrefix(/api)` vers l'api **sans** strip prefix (le backend
  sert ses routes sous `/api`, voir [§13](#13-contrats-techniques)) ; routeur admin sur `ADMIN_HOST`
  avec `dashboard-allowlist@file` ; `/admin` du host public redirigé vers `ADMIN_HOST` (§8).
  Hosts lus depuis `TRAEFIK_HOST` et `ADMIN_HOST` (le préfixe
  `qa.` est ajouté automatiquement en QA).
- Domaine : `TRAEFIK_HOST=iapolitique.fr`, `ADMIN_HOST=admin.iapolitique.fr`. `www.iapolitique.fr`
  redirige (301) vers `iapolitique.fr`. QA : `qa.iapolitique.fr` et `qa.admin.iapolitique.fr`.
  Tous ces noms doivent avoir un enregistrement DNS A vers l'IP publique du cluster pour le
  challenge Let's Encrypt HTTP-01 ; l'admin reste filtré par `dashboard-allowlist@file`.
- Placement : `node.labels.gpu == none`. Healthchecks : api `/api/health` (port 8080), worker
  `/health` (port 8082), tous deux via `iapolitique healthcheck` ; frontend `/health` (port 80,
  `wget`). `DEPLOY_HEALTHCHECK_PATH=/api/health` pour la vérification post-déploiement.
- Migrations : appliquées au démarrage de l'api et du worker (`sqlx migrate`, verrou
  consultatif : plusieurs répliques peuvent démarrer ensemble), jamais destructives
  ([§2.2](#22-autres-règles)).

### 10.2 Environnements

| | Production | QA |
|---|---|---|
| Stack | `iapolitique` | `qa-iapolitique` |
| Host | `iapolitique.fr` | `qa.iapolitique.fr` |
| Base | `iapolitique_prod` | `iapolitique_qa` (via `devops/.env.qa`) |
| Réseau LLM | `llm-net` | `qa-llm-net` (préfixe automatique de PulsarCD) |
| Budget Mistral | `MISTRAL_DAILY_BUDGET_EUR` de prod | plafond réduit |

### 10.3 Variables et secrets

- `devops/.env` et `devops/.env.qa` ne sont **jamais** committés ; `devops/.env.example` liste
  toutes les variables avec des valeurs factices et est tenu à jour à chaque nouvelle variable.
- Les variables sensibles portent un suffixe reconnu par PulsarCD pour devenir des secrets
  Docker : `DATABASE_CONNECTION_STRING`, `MISTRAL_API_KEY`, `ALTCHA_HMAC_KEY`,
  `ADMIN_API_TOKEN`. Ne jamais utiliser `DATABASE_URL` (serait déployé en clair).
- Coordonnées légales (non secrètes mais hors git) : `LEGAL_PUBLISHER_NAME`,
  `LEGAL_PUBLISHER_ADDRESS`, `LEGAL_PUBLISHER_PHONE`, `LEGAL_CONTACT_EMAIL`, `LEGAL_PRIVACY_EMAIL`.
  `LEGAL_CONTACT_EMAIL` et `TRAEFIK_HOST` sont aussi transmises au frontend, qui en tire
  `/.well-known/security.txt` (§14.2).
- Variables non sensibles : `TRAEFIK_HOST`, `ADMIN_HOST`, `LLM_NETWORK`, `DECISION_SERVICE_URL`,
  `DECISION_SERVICE_TIMEOUT_SECS`, `DECISION_SERVICE_JOB_TIMEOUT_SECS`, `MISTRAL_API_BASE_URL`,
  `MISTRAL_ANALYSIS_MODEL`, `MISTRAL_REVIEW_MODEL`, `MISTRAL_EMBED_MODEL`,
  `MISTRAL_ANALYSIS_REASONING_EFFORT`, `MISTRAL_REVIEW_REASONING_EFFORT`,
  `MISTRAL_DAILY_BUDGET_EUR`, prix unitaires Mistral, `DEDUP_SIMILARITY_THRESHOLD`, limites de
  rate limit, `RUST_LOG`, `DATABASE_MAX_CONNECTIONS`, `WORKER_CONCURRENCY`, `TRUSTED_PROXY_HOPS`,
  limites et délais facultatifs (`DATABASE_STATEMENT_TIMEOUT_SECS`,
  `DATABASE_JOB_STATEMENT_TIMEOUT_SECS`, `DATABASE_ACQUIRE_TIMEOUT_SECS`,
  `HTTP_REQUEST_TIMEOUT_SECS`, `HTTP_BODY_LIMIT_BYTES`, `WORKER_POLL_INTERVAL_MS`,
  `JOB_LOCK_TIMEOUT_SECS`, vides = défauts du §13.1),
  et pour PulsarCD seul `DEPLOY_HEALTHCHECK_PATH`, `DEPLOY_CONVERGE_TIMEOUT`. Toute variable
  active de `.env.example` lue par le backend est transmise par `x-backend-env` (api et worker)
  ou par l'environnement de l'api dans `docker-compose.swarm.yml`. Y figurent seulement en
  commentaire : les réglages liés à l'image et aux sondes (`BIND_ADDRESS`, ports, `PROMPTS_DIR`,
  `LOG_FORMAT`), et `ALLOWED_DEV_ORIGINS`, réservée au développement local et que le compose ne
  transmet jamais (`check_compose_security.py` le vérifie).

---

## 11. Hors périmètre et décisions ouvertes

**Hors MVP** (prévoir le champ, pas la fonctionnalité) : suivi du devenir réel des idées
(statut externe : déjà appliquée, en débat, reprise…). **Exclus** : comptes utilisateurs, votes,
commentaires libres, ré-analyse périodique, API publique documentée, multi-pays, autres langues.

**Décidé** : licence Licence Ouverte 2.0 (§7.4) ; éditeur personne physique avec identité
publiée hors dépôt (§7.5) ; aucune IP en clair (§7.2, §7.5).

**À décider** (ne pas trancher sans l'humain) :
- valeurs des seuils de filtrage et de doublon en production (défauts calibrés sur des exemples
  écrits à la main : `docs/calibration/screening_report.md` §4, `DEDUP_SIMILARITY_THRESHOLD`) ;
- prix unitaires Mistral et plafonds journaliers réels ;
- points juridiques de la section 5 de `docs/juridique/memo.md` (à faire valider par un juriste).

---

## 12. Tenir ce fichier à jour

Toute décision structurante (nouveau service, nouvelle variable, changement de règle métier,
de pipeline, de formule) se reflète ici **dans le même commit** que le code concerné. Un agent
qui constate un écart entre ce fichier et le code le signale et corrige l'un ou l'autre dans un
commit dédié (`docs: …`).

---

## 13. Contrats techniques

Contrats partagés par tous les modules. Les modifier = mettre à jour cette section dans le même
commit.

### 13.1 Processus et ports

| Processus | Commande | Ports |
|---|---|---|
| api | `iapolitique api` | 8080 API publique, 8081 API admin |
| worker | `iapolitique worker` | 8082 `/health` interne |
| migrations | `iapolitique migrate` (aussi exécutées au démarrage de l'api) | — |
| openapi | `iapolitique openapi [--output FICHIER] [--public]` : écrit la spec OpenAPI (complète, ou publique seule avec `--public`) sur stdout ou dans `FICHIER` | — |
| healthcheck | `iapolitique healthcheck [--url URL] [--timeout-secs N]` : GET HTTP, code de sortie 0 si 2xx (URL par défaut `http://127.0.0.1:8080/api/health` ; worker : `--url http://127.0.0.1:8082/health`). L'image runtime (distroless, §14.7) n'a ni shell, ni curl, ni wget : sondes en forme exec (`CMD`, jamais `CMD-SHELL` ni `docker run --health-cmd`). | — |
| frontend | nginx (image `nginx:alpine`, workers non root) ; variables `LEGAL_CONTACT_EMAIL`, `TRAEFIK_HOST` lues au démarrage pour `/.well-known/security.txt` | 80 (`/health` : sonde de vivacité) |

Variables de test : `DATABASE_URL` (utilisée **uniquement** par `#[sqlx::test]` en test). En
déploiement, la base est lue depuis `DATABASE_CONNECTION_STRING`.

Variables lues par le backend (`backend/src/config.rs`, valeur vide = non définie ; une erreur de
configuration liste tous les problèmes et empêche le démarrage) :

| Variable | Défaut | Rôle |
|---|---|---|
| `DATABASE_CONNECTION_STRING` | **requis** | URL `postgres://` (secret). Une liste d'hôtes `h1,h2` est réduite au premier. |
| `ADMIN_API_TOKEN`, `ALTCHA_HMAC_KEY` | **requis pour l'api** | secrets, 32 caractères minimum |
| `MISTRAL_API_KEY` | absent | secret ; sans clé, le worker laisse les jobs IA en file (avertissement) ; un handler IA appelé sans client replanifie son job 1 h plus tard (`jobs/guard.rs`), sans le faire échouer ; une clé refusée (401/403) est traitée de même, avec alerte au tableau de bord (§6.4) |
| `BIND_ADDRESS`, `PUBLIC_PORT`, `ADMIN_PORT`, `WORKER_HEALTH_PORT` | `0.0.0.0`, 8080, 8081, 8082 | écoute |
| `HTTP_REQUEST_TIMEOUT_SECS`, `HTTP_BODY_LIMIT_BYTES` | 30, 65536 | limites HTTP |
| `TRUSTED_PROXY_HOPS` | 1 | nombre de proxys (Traefik) qui ajoutent à `X-Forwarded-For` |
| `DATABASE_MAX_CONNECTIONS`, `DATABASE_ACQUIRE_TIMEOUT_SECS` | 10, 5 | pool PostgreSQL |
| `DATABASE_STATEMENT_TIMEOUT_SECS`, `DATABASE_JOB_STATEMENT_TIMEOUT_SECS` | 10, 120 | `statement_timeout` des connexions de l'api et du worker (§14.5) ; une requête annulée (`57014`) répond `503 database_unavailable` ; les migrations s'exécutent sans limite |
| `WORKER_CONCURRENCY`, `WORKER_POLL_INTERVAL_MS`, `JOB_LOCK_TIMEOUT_SECS` | 2, 2000, 300 | file de jobs |
| `DECISION_SERVICE_URL`, `DECISION_SERVICE_TIMEOUT_SECS` | `http://llamacpp-decision-service:8000/v1/systemone`, 5 | modèle de décision ; délai du filtrage synchrone de l'api (1 à 60 s) |
| `DECISION_SERVICE_JOB_TIMEOUT_SECS` | 30 | délai du filtrage par les jobs `screen_*` du worker (1 à 120 s, au moins `DECISION_SERVICE_TIMEOUT_SECS`) |
| `MISTRAL_API_BASE_URL` | `https://api.mistral.ai` | surchargeable pour les tests |
| `MISTRAL_ANALYSIS_MODEL`, `MISTRAL_REVIEW_MODEL`, `MISTRAL_EMBED_MODEL` | `mistral-large-latest`, = analyse, `mistral-embed` | modèles |
| `MISTRAL_ANALYSIS_REASONING_EFFORT`, `MISTRAL_REVIEW_REASONING_EFFORT` | `high`, `none` | `reasoning_effort` envoyé à Mistral (§6.1) : analyse (recherche et structuration) ; second avis et revue de signalement. `high` ou `none` (casse indifférente), toute autre valeur est une erreur de configuration. **Exception à la règle du vide** : variable absente = défaut, variable définie mais vide = paramètre omis (défaut de Mistral) ; le compose les passe donc avec `${VAR-défaut}` (sans `:`) |
| `MISTRAL_DAILY_BUDGET_EUR` | 5 | plafond journalier (décimal, `.` ou `,`) |
| `MISTRAL_ANALYSIS_INPUT_EUR_PER_MTOK`, `MISTRAL_ANALYSIS_OUTPUT_EUR_PER_MTOK` | 2, 6 | prix par million de tokens |
| `MISTRAL_REVIEW_INPUT_EUR_PER_MTOK`, `MISTRAL_REVIEW_OUTPUT_EUR_PER_MTOK` | = analyse | idem pour second avis et revue |
| `MISTRAL_EMBED_EUR_PER_MTOK`, `MISTRAL_WEB_SEARCH_EUR_PER_CALL` | 0.1, 0.03 | embeddings, recherche web |
| `DEDUP_SIMILARITY_THRESHOLD` | 0.95 | similarité cosinus de fusion (valeur prudente, à calibrer ; le réglage `dedup.similarity_threshold`, §13.6, prévaut) |
| `RATE_LIMIT_IDEAS_PER_HOUR`, `RATE_LIMIT_IDEAS_PER_DAY` | 5, 20 | par empreinte |
| `RATE_LIMIT_CONTRIBUTIONS_PER_DAY`, `RATE_LIMIT_REPORTS_PER_DAY` | 20, 20 | par empreinte |
| `PROMPTS_DIR` | `/app/prompts` | prompts versionnés |
| `RUST_LOG`, `LOG_FORMAT` | `info,sqlx=warn`, `json` | logs (`text` en dev) |
| `TRAEFIK_HOST`, `ADMIN_HOST` | absents | origines `https://…` acceptées par l'anti-CSRF (§14.3) ; `TRAEFIK_HOST` sert aussi aux URL canoniques des exports (`iapolitique.fr` si absent) |
| `ALLOWED_DEV_ORIGINS` | vide | origines supplémentaires acceptées par l'anti-CSRF (§14.3) sur les deux ports, séparées par des virgules, de la forme `http(s)://hôte[:port]` (en dev : `http://localhost:5173`) ; jamais en production |
| `LEGAL_PUBLISHER_NAME`, `LEGAL_PUBLISHER_ADDRESS`, `LEGAL_PUBLISHER_PHONE` | absents | identité de l'éditeur (§7.5), une ligne de 200, 300 et 40 caractères au plus ; servis par `GET /api/legal` (`null` si absent, avertissement au démarrage de l'api) |
| `LEGAL_CONTACT_EMAIL`, `LEGAL_PRIVACY_EMAIL` | absents | adresses de contact et de demandes RGPD (adresse électronique valide, sinon erreur de configuration) |

### 13.2 Routes HTTP

Toutes les routes sont servies sous `/api` (pas de strip prefix chez Traefik).

API publique (port 8080) :

| Méthode | Route | Rôle |
|---|---|---|
| GET | `/api/health`, `/api/ready` | vivacité, base joignable |
| GET | `/api/altcha/challenge` | défi proof-of-work |
| POST | `/api/ideas` | dépôt d'une idée `{ text, admin_level?, territory?, accept_licence, accept_publication, altcha }` → `201 { status, tracking_token, reason_code, reason_message, statement }` (`statement` : exposé des motifs d'un refus `{ measure, reason_code, reason_message, rule: { article, title, url }, automated, second_opinion, decided_on, purge_on }`) ; erreurs `consent_required`, `altcha_*` (400), `text_*`, `field_too_long`, `invalid_territory`, `invalid_body` (422), `rate_limited` (429) |
| POST | `/api/track` | suivi d'une idée, contribution ou signalement (corps `{ "token" }`, jamais dans l'URL) → `200 { kind, status, submitted_on, idea: { slug, title }?, decision: exposé des motifs?, report_outcome?, review_request: { requested_on, status }?, idea_withdrawn }` (`status` : statut du §5.2 ou `deleted` ; `idea_withdrawn` : l'idée de la contribution, du signalement ou du doublon fusionné a été retirée par son auteur, `idea` vaut alors `null` ; toujours présent, facultatif dans la spec pour les anciens clients) |
| POST | `/api/track/delete` | suppression par l'auteur (corps `{ "token" }`) → `200 { status: "deleted" }`, idempotent ; une idée publiée ou masquée est retirée (`withdrawn`, §5.2), tout autre texte effacé (§7.1) |
| POST | `/api/track/review-request` | demande de réexamen humain (corps `{ "token", "message"? }`) → `201 { requested_on, status: "pending" }` ; `409 review_not_available` / `review_already_requested` |
| GET | `/api/legal` | `{ publisher: { name, address, phone }, contact_email, privacy_email, cgu_version, pages: { mentions_legales, confidentialite, regles, licence } }` : coordonnées de l'éditeur (`LEGAL_*`, `null` si non configurées), version des CGU (`domain::rules::CGU_VERSION`) et date de mise à jour de chaque page légale (`domain::rules::LEGAL_PAGES_UPDATED_ON`, à modifier avec le texte de la page, en même temps que son miroir `LEGAL_TEXTS_DATES` de `frontend/src/lib/legal.ts`, dates affichées si l'API ne répond pas) |
| GET | `/api/ideas` | classement des idées `published` uniquement : `sort` (`score` par défaut, `gain`, `feasibility`, `delay`, `recent`), filtres `admin_level`, `category` (clé de la taxonomie), `kind`, `territory` (nom, partie du nom sans accents ni casse, ou code INSEE exact), `q` (200 caractères : recherche plein texte française sans accents `websearch_to_tsquery('french_unaccent')` sur titre et résumé, puis, si rien ne correspond, similarité trigramme `word_similarity ≥ 0,3`), `page` (1 à 10 000) / `per_page` (1 à 50, 20 par défaut) → `200 { items: [{ slug, title, summary, admin_level, territory: { name, insee_code }?, category: { key, label }, kind, annual_gain_eur: { low, median, high }, feasibility, time_to_effect_months, confidence, score, proposal_count, contribution_count, updated_at (minuit Europe/Paris du jour de l'analyse courante : aucune heure fine n'est publique, elle daterait le dépôt) }], page, per_page, total, total_pages, search: exact\|approximate\|null }` ; paramètre invalide ou inconnu : `400 invalid_query` |
| GET | `/api/ideas/{slug}` | page d'une idée `published` : `{ slug, title, summary, admin_level, territory?, category, kind, original: { text (masqué), personal_data_masked, attribution, notice }, analysis (champs du §6.2 sans identifiants, `impacts` en liste ordonnée `{ public, label, effect, explanation }`), score: { label, scoring_version, net_annual_eur, feasibility_factor, confidence_factor, delay_factor, score }, contributions: [{ kind, summary, taken_into_account, response, integrated_on }] (résumés neutres, jamais le texte), rights_of_reply: [{ author_label, body, published_at, updated_at }], ai: { ai_generated, provider, model, generated_at (minuit Europe/Paris, comme `updated_at`), analysis_version, prompt_version, research_prompt_version, schema_version, scoring_version, web_search_count, human_reviewed, notice }, proposal_count, contribution_count, published_on, updated_at, canonical_url }` ; idée inconnue, masquée ou purgée : `404 idea_not_found` ; idée fusionnée : `301` vers `/api/ideas/{slug parent}` (corps `{ slug }`) |
| POST | `/api/ideas/{slug}/contributions` | amélioration ou objection `{ kind: amelioration\|objection, text, accept_licence, accept_publication, altcha }` → `201` comme `/api/ideas` ; `404 idea_not_found` si l'idée n'est pas publiée |
| POST | `/api/ideas/{slug}/reports` | signalement `{ reason, details? (alias precision), reporter_name?, reporter_email?, good_faith, altcha }` → `201` comme `/api/ideas` ; erreurs `good_faith_required`, `invalid_email`, `idea_not_found` |
| GET | `/api/methodology` | méthodologie en vigueur (§6.5) : `{ screening_version, taxonomy_version, prompt_versions, analysis_schema_version, export_schema_version, cgu_version, models: { analysis: { model, reasoning_effort, configured_reasoning_effort }, review: { … }, embedding } (configuration en vigueur, §6.1 ; `reasoning_effort` : effort réellement envoyé, `high`, `none` ou `null` si omis par configuration ou refusé par le modèle ; `configured_reasoning_effort` : effort configuré, facultatif dans la spec), scoring: { version, formula (FORMULA_FR), amortization_years, max_feasibility, delay_half_life_months, confidence_factors }, screening: { version, sections: [{ kind, label, criteria: [{ key, question_type, instructions, answers?, levels, pass_when?, pass_from_level?, reject_below, accept_above (seuils appliqués, réglage §13.6 compris), thresholds_adjusted, reason_code, reason_message, rule? }] }] }, files: [{ file, kind, version?, content }] }` (texte intégral des fichiers de `prompts/`, à afficher en texte brut) |
| GET | `/api/taxonomy` | `{ version, admin_levels, categories: [{ key, label, description?, group, admin_levels }], impact_publics, impact_effects, idea_kinds, confidence_levels, legal_vehicles, report_reasons, contribution_kinds, rejection_reasons: [{ code, message, rule? }] }` |
| GET | `/api/stats` | compteurs publics agrégés : `{ ideas_received, ideas_published, ideas_in_progress, ideas_rejected, duplicates_merged, contributions_integrated, published_by_admin_level, published_by_kind, last_published_on, computed_on }` (aucun texte, identifiant ni horodatage fin ; une idée retirée par son auteur, ses contributions et les doublons qui y ont été fusionnés ne sont pas comptés) |
| GET | `/api/exports/ideas.json`, `/api/exports/ideas.csv`, `/api/exports/LICENSE`, `/api/exports/metadata.json` | exports open data : dernière génération de chaque format de la table `exports` (`json`, `csv`, `license`, `metadata`, §7.4), `Content-Type` de `ExportFormat::content_type`, `Content-Disposition: attachment` avec un nom fixe daté du jour de génération (`iapolitique-ideas-AAAA-MM-JJ.json`, `….csv`, `iapolitique-LICENSE-AAAA-MM-JJ.txt`, `iapolitique-metadata-AAAA-MM-JJ.json`), `ETag` (SHA-256 du contenu, `If-None-Match` → `304`), `Last-Modified` ; avant la première génération : `503 exports_unavailable` avec `Retry-After: 300` |
| GET | `/api/openapi.json` | spec OpenAPI |

API admin (port 8081, `Authorization: Bearer $ADMIN_API_TOKEN`), sous `/api/admin/…`
(`http/admin/`). Toute réponse porte `Cache-Control: no-store` et
`X-Robots-Tag: noindex, nofollow` ; toute mutation est journalisée dans `admin_actions` (nom
d'action pointé, cible, détails sans texte de visiteur) dans la transaction qui l'applique, de
même que toute lecture d'une copie scellée (§7.5) ;
les listes sont paginées (`page`, `per_page` ≤ 50 → `{ items, page, per_page, total,
total_pages }`) et refusent les paramètres inconnus (`400 invalid_query`).

| Méthode | Route | Rôle |
|---|---|---|
| GET | `/api/admin/session` | vérifie le jeton (`200 { "authenticated": true }` ou `401` ; `429 admin_locked` pendant un verrouillage, §14.4) |
| GET | `/api/admin/dashboard` | `{ alerts: [{ code (`mistral_auth` : clé refusée, « Clé Mistral refusée depuis le JJ/MM/AAAA à HH:MI (HTTP 401) : … » ; `mistral_reasoning_unsupported` : un modèle configuré avec un effort refuse `reasoning_effort`, §6.1, « Le modèle X refuse le paramètre reasoning_effort depuis le … : … définir MISTRAL_ANALYSIS_MODEL (ou MISTRAL_REVIEW_MODEL) sur un modèle qui le prend en charge, par exemple mistral-large-4-0 » ; un refus d'un modèle qui n'est plus configuré avec un effort n'est pas une alerte), since, last_seen_at, http_status, failures, message (français) }] (toujours présent, vide sans alerte ; facultatif dans la spec pour les anciens clients), jobs: { queued, running, succeeded, dead, by_kind: [{ kind, status, count }] }, screening_window_days (30), screening: [{ kind, total, accepted, rejected, gray_zone, errors, acceptance_rate, rejection_rate, gray_zone_rate, second_opinion_accept, second_opinion_reject, labeled }], ai_costs: { today_eur, month_eur, daily_budget_eur, remaining_today_eur, budget_reached, next_reset, by_purpose: [{ purpose, provider, calls_today, calls_month, today_eur, month_eur }] }, queues: { escalations_pending, review_requests_pending, ideas_analysis_failed, texts_second_opinion, texts_submitted_stale, ideas_hidden, reports_awaiting_review, jobs_dead } }` (jour et mois Europe/Paris) |
| POST | `/api/admin/provider-alerts/mistral/check` | sans corps : « Vérifier la clé Mistral maintenant » (§6.4), un `GET /v1/models` de Mistral (rien de facturé, pas de ligne `ai_calls`, délai 15 s) → `200 { result: ok\|refused\|unavailable, http_status (statut de Mistral, `null` sans réponse), alert_cleared, jobs_due, message (français) }` : `ok` (2xx) clôt l'alerte `mistral_auth` et rend dus aussitôt les jobs Mistral en file dont l'échéance tombe dans l'heure (`AUTH_RETRY_MAX`) ; `refused` (401/403) ouvre ou prolonge l'alerte comme un appel refusé ; `unavailable` (délai, réseau, autre statut) ne change rien ; `409 mistral_key_missing` si l'api n'a pas de `MISTRAL_API_KEY` ; action `provider_alert.mistral_check` (`result`, `http_status`, `alert_cleared`, `jobs_due`) |
| GET | `/api/admin/jobs` | jobs, filtres `status`, `kind` ; plus récents d'abord |
| POST | `/api/admin/jobs/{id}/retry` | remet en file un job `dead` (tentatives remises à zéro) → `200` job ; pour un job `analyze_idea` dont l'idée est restée `analysis_failed` (que le job ignore), l'idée repasse aussi `queued_analysis` (acteur admin, motif `admin_reanalysis`, comme « Relancer l'analyse » de l'idée ; détails `idea_id`, `idea_requeued`) ; `404 job_not_found`, `409 job_not_dead` / `job_already_queued` (même clé de dédoublonnage en file) ; action `job.retry` |
| GET | `/api/admin/ideas` | idées de tout statut, filtre `status`, recherche `q` (identifiant, ou partie du slug, du titre, du résumé ou du texte, sans accents ni casse) ; plus récemment changées d'abord ; extrait du texte masqué, motifs, signalements en attente, réexamen et escalade en attente, `withdrawn_at` (idée retirée par son auteur, statut `withdrawn` ; facultatif dans la spec) |
| POST | `/api/admin/ideas/{id}/hide` | `{ reason }` (1 000 caractères) : `published → hidden` (`hidden_reason = admin`), exports régénérés → `200` idée ; `409 idea_not_published` ; action `idea.hide` |
| POST | `/api/admin/ideas/{id}/republish` | `{ reason }` : `hidden → published` (champs de masquage effacés), exports régénérés ; `409 idea_not_hidden`, ou `409 idea_withdrawn` pour une idée retirée par son auteur (jamais republiée) ; action `idea.republish` |
| POST | `/api/admin/ideas/{id}/reanalyze` | analyse déclencheur `admin` d'une idée `published`/`hidden`, ou `analysis_failed → queued_analysis` → `202 { queued, status }` (`queued: false` si une analyse était déjà en file) ; `409 reanalysis_not_available` ; action `idea.reanalyze` |
| POST | `/api/admin/ideas/reanalyze-failed` | sans corps : toutes les idées `analysis_failed` (500 au plus par appel, plus anciennes d'abord) passent `queued_analysis` avec un job d'analyse (déclencheur `admin`) → `202 { requeued, remaining }` (`remaining` > 0 : rappeler) ; action `idea.reanalyze_failed` (`count`, `remaining`, `idea_ids`), journalisée même à vide |
| GET | `/api/admin/reports` | signalements (filtres `status`, `reason`, `idea_id`) avec idée, précision masquée, coordonnées facultatives du signalant et revue (`outcome`, `escalate`, `explanation`, `corrections`) |
| GET | `/api/admin/escalations` | file d'escalade (filtre `status` `pending`/`done`, en attente d'abord, plus anciennes d'abord) avec `due_at` (cible 24 h) et explication de la revue |
| POST | `/api/admin/escalations/{id}/handle` | `{ outcome: transmitted\|dismissed, note? }` (2 000 caractères) → `200` ; `404 escalation_not_found`, `409 escalation_already_handled` ; action `escalation.handle` |
| GET | `/api/admin/escalations/{id}/seal` | copie scellée de l'escalade (§7.5, D3) → `200 { escalation_id, idea_id, escalated_at, purge_after, sealed_at, content: { idea: { status, slug, title, summary, original_text, submitted_territory, submitted_admin_level, submitted_at, published_at, hidden_at, hidden_reason }, analysis, review: { outcome, escalate, explanation, model, prompt_version, reviewed_at }, reports: [{ reason, details, reported_at }], backfilled? } }` (texte brut, jamais rendu en HTML) ; `404 seal_not_found` (aucune copie, ou purgée un an après l'escalade) ; **chaque lecture** est journalisée (action `escalation.seal_read`, cible l'escalade, détails `seal_id`, `idea_id`) dans la transaction qui lit |
| GET | `/api/admin/escalation-seals` | copies scellées encore conservées, sans leur contenu (pas de journalisation), escalade la plus récente d'abord : `{ escalation_id, idea_id, escalated_at, purge_after, sealed_at, escalation_status (pending\|done\|null si l'escalade a disparu), idea_status (null si l'idée a disparu) }` |
| GET | `/api/admin/review-requests` | demandes de réexamen (filtre `status`, en attente d'abord) avec texte masqué concerné, message, décision en vigueur (`{ measure: rejected\|hidden, reason_code }`) |
| POST | `/api/admin/review-requests/{id}/resolve` | `{ resolution: confirmed\|overturned, note? }` (§7.1) → `200` ; `404 review_request_not_found`, `409 review_request_already_resolved` / `decision_not_overturnable` (texte ni refusé ni masqué) ; action `review_request.resolve` |
| GET | `/api/admin/screenings` | décisions de filtrage (filtres `kind`, `outcome`, `labeled` (`true` : étiquetées, `false` : décidées et non étiquetées), `reason_code`), plus récentes d'abord : version de grille, issue, motif, erreur, latence, `criteria: [{ key, probability, reject_below, accept_above, verdict }]` dans l'ordre de la grille, second avis, décision finale, étiquette `{ label: correct\|incorrect, expected: accept\|reject, note, labeled_at }`, début du texte masqué (`null` après purge) |
| PUT, DELETE | `/api/admin/screenings/{id}/label` | étiquette de calibration `{ label: correct\|incorrect, expected?: accept\|reject, note? }` : `expected` se déduit de `label` et de la décision atteinte, il est requis sans décision (erreur du service, second avis en attente) ; incohérence : `422 label_inconsistent` ; stockée dans `screenings.human_label` (décision attendue) ; `DELETE` la retire ; actions `screening.label` / `screening.unlabel` |
| GET | `/api/admin/screenings/export.jsonl` | export JSON Lines pour `docs/calibration/bench_screening.py` (`application/x-ndjson`, pièce jointe) : `part=results` (défaut : `{ id, section, questions_hash, latency_ms, input_tokens, answers, grid_version }` des décisions avec réponses brutes ; `questions_hash` calculé comme le banc pour la grille en vigueur, `grid-<version>` pour une autre version, que le banc ignore) ou `part=samples` (décisions étiquetées : `{ id, section, text (masqué, vide après purge), expected, fails, gray_ok, split: tune, tags }` + `parent` et `kind` ou `reason_label`) ; filtres `kind`, `labeled_only`, `limit` (5 000 par défaut, 20 000 au plus) |
| GET, POST | `/api/admin/ideas/{id}/replies` | droits de réponse d'une idée ; `POST { author_label (une ligne, 200 caractères), body (8 000 caractères, texte brut), requested_on (pas dans le futur) }` → `201 { id, idea_id, author_label, body, requested_on, due_on (requested_on + 3 jours), published_at, updated_at }`, publié aussitôt sous une idée `published` ou `hidden` (`409 idea_not_published` sinon, `404 idea_not_found`, `422 invalid_field`) ; action `reply.create` |
| PUT, DELETE | `/api/admin/replies/{id}` | modifie (même corps) ou retire (`204`) un droit de réponse ; `404 reply_not_found` ; actions `reply.update` / `reply.delete` |
| GET | `/api/admin/actions` | journal `admin_actions` (filtres `action`, `target_id`), plus récentes d'abord |
| GET | `/api/admin/settings` | seuils en vigueur par section et critère (`reject_below`, `accept_above`, valeurs du fichier, `adjusted`) et seuil de doublon (`value`, `default`, `adjusted`, `minimum`) |
| PUT | `/api/admin/settings/screening-thresholds` | remplace le réglage `screening.thresholds` (§13.6) : `{ idea?, contribution?, report? : { "<clé>": { reject_below?, accept_above? } } }`, critères vérifiés contre `screening.yaml`, seuils dans [0, 1] et `reject_below ≤ accept_above` après fusion avec le fichier (`422 invalid_thresholds`) ; `{}` revient aux seuils du fichier ; action `settings.screening_thresholds` (avant/après) |
| PUT | `/api/admin/settings/dedup-threshold` | `{ value: nombre entre 0,5 et 1 \| null }` (`null` : retour à `DEDUP_SIMILARITY_THRESHOLD`) ; `422 invalid_threshold` ; action `settings.dedup_threshold` |

Le port admin sert aussi, sans jeton, `/api/health`, `/api/ready` et `/api/openapi.json` (spec
complète ; celle du port public omet les routes admin et les schémas qu'elles seules
utilisent). Le worker répond sur `/health` (et `/api/health`)
du port 8082 : `503` si sa boucle est bloquée (2 min sans tour ; quand toutes les places sont
prises par des jobs longs, la boucle continue de tourner à chaque intervalle de sondage : ce n'est
pas un blocage, sinon Swarm tuerait le worker en pleine analyse).

Erreurs : toute réponse 4xx/5xx a le corps JSON du §4.1, y compris 404, 405, 413 (corps au-delà
de `HTTP_BODY_LIMIT_BYTES`), 403 et 415 (anti-CSRF, §14.3), 504 (`timeout`, au-delà de `HTTP_REQUEST_TIMEOUT_SECS`) et 500.
Chaque réponse porte un en-tête `x-request-id` généré par le serveur.
Quand une réponse HTTP/1 laisse le corps de la requête non lu (route inconnue, 403, 405, 413,
415…), l'api lit et jette le reste du corps avant de répondre (1 Mio et 2 s au plus,
`http::middleware::drain_unread_body`), et pose `Connection: close` au-delà : sinon hyper ferme la
connexion sans le dire, et Traefik, qui partage ses connexions vers l'api entre tous les clients,
pouvait y envoyer la requête d'un autre visiteur et lui répondre `500` (revue ZAP-2).

Le frontend appelle toujours des chemins relatifs `/api/...`. En dev, Vite proxifie `/api` vers
`localhost:8080` (et vers `8081` pour `/api/admin`). En production, Traefik fait de même sur
`ADMIN_HOST` (`/api/admin` → 8081, reste de `/api` → 8080, les deux filtrés par
`dashboard-allowlist@file`) ; sur `TRAEFIK_HOST`, seul le port 8080 est routé.

### 13.3 Types d'API partagés

`backend` génère la spec (`utoipa`) ; elle est committée dans `frontend/src/api/openapi.json`
et les types TS sont générés par `npm run gen:api` (`openapi-typescript`) vers
`frontend/src/api/schema.d.ts`. Un test backend échoue si la spec committée diffère de la spec
générée : toute modification d'API régénère les deux fichiers dans le même commit.

- Les `paths` de la spec sont les chemins **servis**, préfixe `/api` inclus (`/api/ideas`, pas
  `/ideas` avec un `servers: [{ url: "/api" }]`) : le client front (`openapi-fetch`) les résout
  sur l'origine courante.
- Sans Node local, régénérer via Docker :
  `docker run --rm -v "<repo>/frontend:/app" -v iapolitique_node_modules:/app/node_modules -w /app node:22-alpine sh -c "npm ci && npm run gen:api"`.
- `npm run lint` échoue si `schema.d.ts` n'est pas à jour par rapport à `openapi.json`.
- Les erreurs sont normalisées côté front en `ApiError { status, code, message }` à partir du
  corps `{ "error": { "code", "message" } }` : le `message` est affiché tel quel à l'utilisateur
  (donc en français, sans détail technique).

### 13.4 Fichiers `prompts/`

Embarqués dans l'image (`/app/prompts`, chemin configurable `PROMPTS_DIR`), chargés et validés
au démarrage (un fichier invalide empêche le démarrage).

| Fichier | Format |
|---|---|
| `screening.yaml` | `version` + trois sections `idea`, `contribution`, `report`. Chaque section : liste ordonnée de critères `{ key, type: noul\|score, instructions, criteria?, pass_when (true/false pour noul, niveau minimal pour score), reject_below, accept_above, reason_code, reason_message }`. Les seuils portent sur la probabilité de **réussir** le critère (`noul` ou `1 − noul` selon `pass_when` ; pour `score`, somme des probabilités des niveaux d'indice ≥ `pass_when`) ; entre `reject_below` et `accept_above` = zone grise. `state` envoyé : texte brut (idea, précédé de `Territoire indiqué par l'auteur : <territoire>` et d'une ligne vide si l'auteur en a indiqué un), objet `{ idee, type, contribution }` (contribution), objet `{ idee, motif, precision }` (report), où `idee` est le résumé neutre de l'idée publiée. Calibration : `docs/calibration/`. |
| `analysis_research.md`, `analysis.md`, `second_opinion.md`, `report_review.md` | En-tête YAML entre deux lignes `---` : `version`, `model_hint`, `tools` (`[web_search]` ou `[]`), `output_schema` (fichier de schéma ou `null`), `variables` (map nom → description ; toutes doivent être substituées ; une valeur absente ou vide prend la valeur citée dans la description après « ou » — « (aucune) », « non précisé »… —, une chaîne vide si la description dit « chaîne vide », « (aucun) » sinon). Corps en français découpé par la ligne `<!-- user -->` : avant = instructions système (seule variable admise : `{{today}}`, pour garder le cache de prompt), après = message utilisateur avec les variables `{{nom}}`. Les textes de visiteurs sont insérés entre balises (`<idee_originale>`…) : le backend met chaque valeur substituée en forme NFKC (`＜`, `﹤` deviennent `<`, lettres et accents combinants recomposés) puis remplace tout chevron (`<`, `>`, `〈`, `〉`, `⟨`, `⟩`, `❮`, `❯`…) par `‹` ou `›` : ni balise à attributs, ni balise auto-fermante, ni variante Unicode ne peut fermer un cadre (`PromptTemplate::neutralize`). |
| Analyse en deux étapes | Étape 1 `analysis_research.md` (Conversations API avec `web_search`) → mémo sourcé en texte ; étape 2 `analysis.md` (`response_format` json_schema strict, sans outil) avec `{{research}}` = le mémo. Si la recherche web et la sortie structurée sont compatibles dans un même appel, `analysis.md` seul suffit avec `{{research}}` vide et l'outil `web_search` activé. Les versions des deux fichiers sont stockées avec l'analyse. |
| `analysis.schema.json` | JSON Schema (draft 2020-12, sous-ensemble compatible avec le mode strict de Mistral : `type`, `properties`, `required` = toutes les propriétés, `additionalProperties: false`, `items`, `enum`, `description`, `title`) de la sortie d'analyse (§6.2), utilisé tel quel en `response_format` (`{"type": "json_schema", "json_schema": {"name": "analysis", "schema": …, "strict": true}}`) et pour valider la réponse. Le backend vérifie en plus : low ≤ median ≤ high, montants et délai ≥ 0, catégorie compatible avec `admin_level`, références `[Sx]` présentes dans `sources`, au moins une source, `ref` de chaque source parmi les identifiants `Rn` des pages consultées par la recherche web ou des sources de l'analyse remplacée (§6.2, `ai::outputs::ConsultedPages`, avec retour au modèle), et pour toute chaîne générée (analyse, second avis, revue de signalement) : aucun caractère de contrôle hors `
` et `	`, aucun contrôle bidi ni caractère invisible, longueur bornée (titre 200, résumé 1 500, textes longs 6 000, éléments de liste 1 500, champs de source 500, URL 2 048 caractères ; `ai::outputs::check_ai_text`). Une valeur que PostgreSQL refuse au stockage (`22P05`, `22021`, `22001`) fait échouer l'analyse (`analysis_failed`) sans relancer les appels payants. |
| `second_opinion.schema.json`, `report_review.schema.json` | Idem pour le second avis (`decision: accept\|reject`, `reason_code` = `none` ou un `reason_code` de `screening.yaml`, `explanation`) et la revue de signalement (`outcome: maintain\|update\|hide`, `escalate` : booléen, `true` pour un contenu grave — diffamation, menace, haine, terrorisme, pédocriminalité — avec alors `outcome = hide` : le backend masque immédiatement et place l'idée dans la file d'escalade, §7.5 ; `explanation`, `corrections` : consignes passées en `{{report}}` à la ré-analyse si `update`, vide sinon). |
| Contrôle | `python docs/calibration/check_prompts.py` vérifie formats, variables, mode strict et synchronisation des énumérations avec `taxonomy.yaml` et `screening.yaml` (à lancer après toute modification de `prompts/`). |
| `taxonomy.yaml` | `version`, `admin_levels` (`{ key, label, description, category_groups }`), `categories` (map groupe → liste `{ key, label, description }` ; groupes `etat`, `securite_sociale`, `collectivites`, `transverse` ; clé de catégorie unique dans tout le fichier ; un niveau n'accepte que les catégories de ses `category_groups`), `impact_publics`, `impact_effects`, `idea_kinds`, `confidence_levels`, `legal_vehicles`, `report_reasons`, `contribution_kinds` (listes `{ key, label, description? }`), libellés français. |

### 13.5 Tests en CI

Le service `test` du compose construit `devops/Dockerfile.test` (contexte `..`) :
1. stages `frontend-audit` et `frontend-check` (node 22), sur le même `npm ci` : d'un côté
   `npm audit --omit=dev --audit-level=high`, de l'autre lint, typecheck, Vitest, build, puis `npm run check:dist` (`scripts/check-dist-urls.mjs` : le build ne nomme aucune origine tierce
   hors d'une liste justifiée d'identifiants et de liens de documentation ; aussi exécuté par
   `frontend/Dockerfile`) ;
2. stage `frontend-headers` (même image nginx que le runtime, même `apk upgrade`) : le `dist/` de `frontend-check`
   servi par `frontend/nginx.conf`, contrôlé par `devops/check_frontend_headers.sh` (en-têtes,
   nonce CSP, cache, méthodes, fichiers cachés, `security.txt` avec et sans adresse, §14.2) ;
3. stage `devops-check` (alpine, python3 + PyYAML du paquet `py3-yaml`) :
   `devops/check_compose_security.py` (invariants du compose, §14.4 et §14.7),
   `devops/check_image_pins.py` (images de base épinglées par digest, §14.7) et leurs tests ;
4. stage `backend-test` (rust) : `cargo fmt --check`, `cargo clippy -D warnings`, compilation des
   tests (`cargo test --no-run --message-format=json-render-diagnostics`), dont seuls les
   exécutables (messages `compiler-artifact` à `profile.test`) sont copiés dans l'image, sous
   `/src/backend/test-bin/` (`lib-iapolitique`, `bin-iapolitique`…), puis `cargo deny`
   (advisories, bans, sources ; §14.7) ; il copie un fichier témoin produit par chacun des stages
   précédents (sinon BuildKit les saute) ;
5. à l'exécution (`devops/run-tests.sh`) : attente du port de `test-db`, puis chaque exécutable
   de `test-bin/` par ordre alphabétique, depuis `/src/backend` et avec `CARGO_MANIFEST_DIR`
   comme sous `cargo test` (ni cargo ni compilation ; les arguments de
   `docker compose run --rm test …` vont à chacun, premier `--` retiré). Tous
   tournent même après un échec (comme `--no-fail-fast`) et le code de sortie est non nul si l'un
   d'eux échoue. `DATABASE_URL` pointe vers le service
   `test-db` (`pgvector/pgvector:pg18` en mémoire, sans mot de passe, réseau interne au compose).
   `test-db` n'est **pas** en `replicas: 0` (compose ne démarrerait pas la dépendance) mais en
   `mode: global` contraint sur une étiquette qu'aucun nœud ne porte : zéro tâche sur le swarm.
   Le service `test` lui-même est en `replicas: 0`.

Caches BuildKit : npm, et pour cargo le registre et `target/` montés directement aux chemins
qu'utilise cargo (`/usr/local/cargo/registry`, `/src/backend/target`), jamais recopiés : ni l'un
ni l'autre n'est dans l'image (les tests lisent à l'exécution `../prompts`,
`../frontend/src/api/openapi.json` et `src/`, d'où la même arborescence sous `/src`). Les id sont
propres à chaque build, pour que `docker compose build api frontend test`, qui les lance en
parallèle, n'attende pas un verrou : `iapolitique-cargo-registry-test` et
`iapolitique-cargo-target-test` (stage `backend-test`, et `cargo deny` pour le registre),
`iapolitique-cargo-registry-release` et `iapolitique-cargo-target-release` (`backend/Dockerfile`).
Les deux builds touchent `src/main.rs`, `src/lib.rs` et `build.rs` : le crate est toujours
recompilé (jamais un `target/` périmé), les dépendances jamais. Il n'y a pas de doctest (les blocs
de code des commentaires sont en `text`) : en ajouter un demande d'ajouter `cargo test --doc` au
stage `backend-test`. La version de Rust de `Dockerfile.test` (`RUST_VERSION`) suit celle de
`backend/Dockerfile`.

En local, sans Node installé : `docker run --rm -v "<repo>/frontend:/app" -v iapolitique_node_modules:/app/node_modules -w /app node:22-alpine npm …`.
Postgres local de test : `docker run -d --name iapolitique-test-pg -e POSTGRES_PASSWORD=postgres -p 55432:5432 pgvector/pgvector:pg18`
puis `DATABASE_URL=postgres://postgres:postgres@localhost:55432/postgres cargo test`.

### 13.6 Réglages d'exécution (table `settings`)

Valeurs JSON modifiables depuis l'admin, prioritaires sur les défauts de `prompts/` et de
l'environnement. Une valeur illisible ou incohérente est ignorée (avertissement dans les logs) et
le défaut s'applique.

| Clé | Valeur | Rôle |
|---|---|---|
| `screening.thresholds` | `{ "<section>": { "<clé du critère>": { "reject_below"?: nombre, "accept_above"?: nombre } } }`, sections `idea`, `contribution`, `report` | Surcharge, critère par critère, des seuils de `screening.yaml` (§5.3). Un seuil omis garde la valeur du fichier ; il faut `0 ≤ reject_below ≤ accept_above ≤ 1`. Les seuils appliqués sont enregistrés avec chaque décision (`screenings.thresholds`). |
| `dedup.similarity_threshold` | nombre entre 0 et 1 (l'admin n'accepte que 0,5 à 1) | Similarité cosinus à partir de laquelle une idée acceptée est fusionnée dans l'idée publiée la plus proche (§5.4) ; surcharge `DEDUP_SIMILARITY_THRESHOLD`. |

---

## 14. Sécurité

Exigences **bloquantes** : tout code nouveau ou modifié les respecte, et chacune est couverte par
un test automatisé. En cas de conflit avec une autre section, la plus restrictive s'applique.
Défense en profondeur : le WAF Coraza/OWASP CRS et le rate limit de Traefik (`global@file`) ne
dispensent d'aucune des mesures ci-dessous.

### 14.1 Injections

- **SQL** : uniquement des requêtes paramétrées (`bind`). Jamais de `format!` ni de concaténation
  pour construire du SQL. Les parties dynamiques (tri, filtres) passent par des `enum` Rust
  convertis en fragments SQL **constants** (liste blanche). `q` (recherche) passe par
  `websearch_to_tsquery` / paramètres, jamais interpolé ; recherche de sous-chaîne (territoire,
  recherche admin) par `strpos`, sans métacaractère, jamais par `LIKE` sur une entrée (`unaccent()`
  transforme `％`, `＿`, `＼` en `%`, `_`, `\`).
- **Prompt injection** : le texte des visiteurs n'est jamais placé dans la partie système d'un
  prompt ; il est encadré de balises, et tout chevron des valeurs substituées est neutralisé
  après normalisation NFKC (§13.4), le critère `injection` du modèle de décision s'applique à tout texte entrant, la
  sortie de Mistral est validée par schéma strict puis par des contrôles sémantiques, et elle
  n'est **jamais** exécutée ni interprétée comme instruction : seul l'outil `web_search` est
  autorisé, aucun autre outil ni appel de fonction. Les sources d'une analyse ne sont que des
  pages réellement consultées par sa recherche web (fragments `tool_reference` et résultats
  des exécutions `web_search`, écrits par l'outil de Mistral, §6.1) ou les sources
  de l'analyse remplacée, citées par identifiant `Rn` : titre, site et URL publiés viennent de
  cette liste, jamais du texte du modèle, d'une idée, d'une contribution, du mémo ni du
  contenu d'une page (INJ-3, §6.2). Un texte qui tente de modifier les règles
  de l'analyse est refusé, et une analyse dont le contenu contredit le schéma est rejetée.
- **XSS** : React échappe tout ; `dangerouslySetInnerHTML` est interdit (règle ESLint
  `react/no-danger` en erreur, avec `react/no-danger-with-children`, `react/jsx-no-script-url`,
  `react/jsx-no-target-blank` et `no-restricted-properties` sur `innerHTML`, `outerHTML`,
  `insertAdjacentHTML`, `document.write`). Seule exception admise : le JSON-LD, sérialisé par une
  fonction unique (`aiJsonLdScript`) qui échappe `<`, `>`, `&`, U+2028 et U+2029 ; il est
  aujourd'hui rendu comme texte enfant de `<script>`, sans `dangerouslySetInnerHTML`. Aucun rendu
  Markdown ou HTML de contenu venant des visiteurs ou de l'IA : texte brut uniquement (les prompts
  de la page Méthodologie sont affichés en `<pre>`). Les URL de sources ne sont rendues en lien
  que si leur schéma est `https:` (ou `http:`), avec `rel="noopener noreferrer nofollow ugc"` et
  `target="_blank"` ; `javascript:`, `data:` et autres sont affichées en texte. Une adresse
  e-mail n'est rendue en lien `mailto:` que si elle respecte le motif strict de
  `frontend/src/lib/email.ts` (dot-atom sans `?`, `&`, `%` ni `#`, domaine en libellés DNS), aussi
  appliqué par le formulaire de signalement ; sinon elle est affichée en texte, pour qu'aucun champ
  d'en-tête (`cc`, `bcc`, `body`, RFC 6068) ne s'ajoute à la réponse de l'éditeur. Le test
  `src/test/xss-guards.test.ts` vérifie en plus, dans les sources : aucun puits HTML ni code
  dynamique, aucune règle XSS désactivée, aucune dépendance de rendu Markdown/HTML, et le
  stockage navigateur limité au thème, aux liens de suivi (`localStorage`) et au jeton admin
  (`sessionStorage`).
- **CSV / formules** : dans les exports CSV, toute cellule commençant par `=`, `+`, `-`, `@`,
  tabulation ou retour chariot est préfixée par `'` ; encodage UTF-8 avec BOM, séparateur
  documenté, guillemets RFC 4180.
- **En-têtes et chemins** : aucune valeur venant d'un visiteur dans un en-tête de réponse (y
  compris `Content-Disposition`, construit à partir de noms fixes), aucun chemin de fichier
  construit à partir d'une entrée.
- **SSRF** : le backend n'appelle que les URL configurées (`DECISION_SERVICE_URL`,
  `MISTRAL_API_BASE_URL`) ; il ne télécharge jamais une URL fournie par un visiteur ou par l'IA.
- **Désérialisation** : structures d'entrée avec `#[serde(deny_unknown_fields)]`, longueur
  maximale sur **chaque** chaîne, énumérations fermées, bornes sur les nombres et la pagination
  (`per_page ≤ 50`), format du jeton vérifié (base64url, longueur exacte) avant hachage, slugs
  validés par `is_valid_slug`.

### 14.2 En-têtes HTTP

Posés par nginx (frontend) **et** par le backend, testés automatiquement. Côté backend, une couche
unique (`http/security.rs`) les pose sur **toute** réponse des ports public, admin et santé du
worker (erreurs 404, 405, 413, 415, 504 et 500 comprises), retire `Server`, `X-Powered-By` et tout
`Access-Control-*` ; une réponse sans `Cache-Control` explicite reçoit `no-store` :

| En-tête | Valeur |
|---|---|
| `Content-Security-Policy` (frontend) | `default-src 'none'; script-src 'self'; style-src 'self' 'nonce-<par réponse>' <3 empreintes sha256>; style-src-attr 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; worker-src 'self'; manifest-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'; object-src 'none'; upgrade-insecure-requests` (valeur exacte : `frontend/nginx.conf`). Aucun `'unsafe-inline'` ni `'unsafe-eval'` pour les scripts ni pour les éléments `<style>`. `worker-src 'self'` suffit : le worker ALTCHA est un fichier de même origine (`altcha/worker`), sans `blob:`. Les `<style>` injectés par des bibliothèques sont autorisés un par un : par **empreinte** pour les contenus fixes (feuille de sonner, insérée vide puis remplie, d'où l'empreinte de la chaîne vide ; style du viewport de Radix Select), vérifiées contre `node_modules` par `src/test/csp.test.tsx` (à mettre à jour avec la bibliothèque) ; par **nonce** (`$request_id`, 128 bits, nouveau à chaque réponse) pour le style de compensation de barre de défilement de react-remove-scroll (dialogues et listes Radix), dont le contenu varie : nginx pose le nonce sur la balise de `theme-init.js` (`sub_filter`) et ce script le copie dans `window.__webpack_nonce__`, lu par react-style-singleton. `style-src-attr 'unsafe-inline'` : attributs `style` seulement (positionnement Radix, widget ALTCHA). |
| `Content-Security-Policy` (api) | `default-src 'none'; frame-ancestors 'none'; base-uri 'none'` |
| `Strict-Transport-Security` | `max-age=63072000; includeSubDomains`, sans `preload` (difficile à annuler : décision de l'éditeur). Derrière Traefik, le middleware `security-headers` de `global@file` (infrastructure partagée) remplace cette valeur par la sienne, aujourd'hui `max-age=31536000; includeSubDomains; preload` : voir `docs/ops.md` |
| `X-Content-Type-Options` | `nosniff` |
| `X-Frame-Options` | `DENY` |
| `Referrer-Policy` | `no-referrer` |
| `Permissions-Policy` | tout désactivé (`camera=(), microphone=(), geolocation=(), payment=(), usb=(), browsing-topics=()`, etc. : liste dans `frontend/nginx.conf`) ; seulement des fonctionnalités reconnues par les navigateurs (Chromium signale en console `bluetooth` ; `interest-cohort`, FLoC abandonné, est remplacé par `browsing-topics`) |
| `Cross-Origin-Opener-Policy` | `same-origin` |
| `Cross-Origin-Resource-Policy` | `same-origin` |
| `Cross-Origin-Embedder-Policy` | `require-corp` (frontend) : compatible, toutes les ressources étant de même origine ; Chromium et Firefox voient les pages isolées (`crossOriginIsolated`), vérifié par `devops/dev/browser_check.sh` |
| `Cache-Control` | `no-store` sur toute réponse contenant un jeton, sur `/api/track*` et sur l'API admin ; `max-age` court sur les GET publics (`public, max-age=60` : classement, idée, statistiques ; `public, max-age=300` : méthodologie, taxonomie, mentions légales, exports ; `http/cache.rs`) |
| `X-Robots-Tag` | `noindex, nofollow` sur l'admin, `/suivi` et l'API (nginx : chemins `/suivi*` et `/admin*` sans tenir compte de la casse, et tout chemin encodé en `%`) |
| `Server` / `X-Powered-By` | absents ou neutres |

Autres règles de nginx (`frontend/nginx.conf`) : `Cache-Control: no-store` sur `/suivi*` et
`/admin*` (mêmes chemins que `X-Robots-Tag`), `no-cache` sur le reste sauf `/assets/` (immuable) ;
`index.html` porte un nonce nouveau à chaque réponse, donc sans `ETag` ; `server_tokens off` ;
seules les méthodes `GET` et `HEAD` existent (`405` avec `Allow: GET, HEAD` sinon) ; aucun fichier
ou dossier commençant par un point n'est servi (`404`), sauf `/.well-known/security.txt`. Contrôlé
en CI par `devops/check_frontend_headers.sh` (§13.5) et dans un vrai navigateur par
`devops/dev/browser_check.sh`.

`/.well-known/security.txt` (RFC 9116) : généré au démarrage du conteneur frontend par le
mécanisme de gabarits `envsubst` de l'image nginx (`frontend/nginx/.well-known/security.txt.template`,
`frontend/nginx/16-security-txt.envsh`) à partir de `LEGAL_CONTACT_EMAIL` (`Contact: mailto:`) et de
`TRAEFIK_HOST` (`Canonical: https://<hôte>/.well-known/security.txt`), avec `Preferred-Languages: fr`
et `Expires` à 180 jours (renouvelé à chaque démarrage). Sans adresse valide : `404`, jamais un
fichier incomplet.

### 14.3 CORS et CSRF

- **Aucun CORS** : l'API est same-origin uniquement. Elle n'émet jamais
  `Access-Control-Allow-*` ; une pré-requête `OPTIONS` est refusée. Pas de couche CORS
  permissive, même en dev (le proxy Vite rend tout same-origin).
- **Anti-CSRF** sur toute requête non `GET`/`HEAD` (`http/csrf.rs`) : `Content-Type:
  application/json` exigé (`415 unsupported_media_type` sinon : bloque les formulaires
  cross-site) ; seule une requête **sans corps** peut l'omettre (`openapi-fetch` n'en envoie pas
  pour un `POST`/`DELETE` sans corps). En-tête `Origin` vérifié : sur le port public, seules
  `https://TRAEFIK_HOST` et `https://ADMIN_HOST` (Traefik y route aussi `/api`) sont acceptées ;
  sur le port admin, seule `https://ADMIN_HOST` ; plus, sur les deux, `ALLOWED_DEV_ORIGINS`
  (§13.1). Sans `Origin`, `Sec-Fetch-Site` doit valoir `same-origin` ou `none` ;
  `Sec-Fetch-Site: cross-site` ou `same-site` est refusé même avec un `Origin` admis. Refus :
  `403 origin_not_allowed` (message français). Une requête **sans** `Origin` **ni**
  `Sec-Fetch-Site` est acceptée : elle ne vient pas d'un navigateur (curl, scripts, scénario de
  bout en bout), or un CSRF suppose un navigateur, qui envoie toujours `Origin` sur un `POST`
  cross-site. En dev derrière le proxy Vite, définir `ALLOWED_DEV_ORIGINS=http://localhost:5173`.
- `OPTIONS` (pré-requête CORS ou non) : `405 method_not_allowed` sur les deux ports.
- Aucun cookie n'existe ; s'il en apparaissait un, il serait `Secure; HttpOnly; SameSite=Strict`.

### 14.4 Authentification et autorisation (admin)

- Port 8081 jamais routé depuis Internet (routeur admin uniquement sur `ADMIN_HOST`, filtré par
  `dashboard-allowlist@file`) ; un test de configuration vérifie les labels du compose.
- Jeton `ADMIN_API_TOKEN` ≥ 32 caractères, comparaison en temps constant, jamais dans une URL ni
  un log ; échecs comptés et limités (`http/admin/mod.rs`) : après 10 jetons **erronés** en
  5 minutes (fenêtre glissante, compteur en mémoire commun à tout le port, aucune IP stockée), toute
  requête `/api/admin/…` est refusée 5 minutes, bon jeton compris, par `429 admin_locked` avec
  `Retry-After` ; un en-tête absent répond `401` sans compter (sonde de session des pages admin) ;
  chaque échec est un événement de sécurité (§14.6) ; côté navigateur, conservé en mémoire ou en `sessionStorage`, jamais en
  `localStorage`, et oublié dès qu'une requête admin qui le portait reçoit `401` (jeton faux ou
  changé : middleware `adminRejectionMiddleware` de `frontend/src/api/client.ts`, la page redemande
  le jeton) ; pendant un verrouillage (`429 admin_locked`), la page affiche le message français de
  l'API et le délai de `Retry-After` (revue AUTH-3).
- Le port public ne connaît aucune route admin (404) ; test dédié.
- **Exposition résiduelle** (revue de sécurité NET-1, NET-4) : « jamais routé depuis Internet »
  vaut pour Traefik seulement. L'api écoute sur `0.0.0.0` et rejoint les réseaux partagés
  `proxy`, `postgresqlcluster_internal` et `llm-net` : tout conteneur de ces réseaux joint
  directement 8080 (sans WAF ni rate limit de Traefik, en choisissant son `X-Forwarded-For`) et
  8081 (sans `dashboard-allowlist@file`, protégé par le seul jeton, et capable de déclencher le
  verrouillage). Corrections attendues et procédure : `docs/ops.md` §5.4 (réseau overlay dédié
  partagé avec Traefik seul, à créer par le propriétaire de Traefik ; confiance en
  `X-Forwarded-For` limitée au pair Traefik côté backend). Correction **reportée** par le
  propriétaire (décision D4, 2026-10-19) : risque résiduel accepté, procédure pas à pas prête.

### 14.5 Abus et déni de service

ALTCHA, rate limits (§7.2), `HTTP_BODY_LIMIT_BYTES`, `HTTP_REQUEST_TIMEOUT_SECS`, sémaphore du
modèle de décision, budget Mistral (§6.4), `statement_timeout` PostgreSQL côté application
(`DATABASE_STATEMENT_TIMEOUT_SECS` pour l'api, `DATABASE_JOB_STATEMENT_TIMEOUT_SECS` pour le worker,
posé à chaque connexion du pool ; migrations sur une connexion dédiée sans limite),
concurrence du worker bornée, pagination bornée (toute liste servie a une limite : `per_page ≤ 50`,
contributions et droits de réponse d'une idée plafonnés), regex à temps linéaire uniquement (crate
`regex`), taille maximale des réponses IA acceptées (`ai::read_capped` : 2 Mio pour Mistral, réponse
refusée comme sortie invalide ; 256 Kio pour le modèle de décision), exports servis depuis la base (pas de
génération à la demande ; `304` sans lecture du contenu, contenu de la dernière génération de
chaque format gardé en mémoire par l'api et partagé entre téléchargements : `http::exports::ExportCache`). Limites de ressources Swarm (`deploy.resources`, vérifiées par
`devops/check_compose_security.py` et appliquées aussi par `devops/dev/e2e.sh`) : api et worker
`cpus: 2.0`, `memory: 512M`, `pids: 1024`, réservation `cpus: 0.25`, `memory: 128M` ; frontend
`cpus: 0.5`, `memory: 128M`, `pids: 256` (workers nginx), réservation `memory: 32M` : une api
saturée ne prend pas tous les cœurs d'un nœud partagé avec d'autres stacks, et une rafale de
processus est contenue.

### 14.6 Secrets, erreurs et journaux

Secrets uniquement via `/run/secrets` / environnement, type `Secret` à `Debug` masqué, jamais
dans une erreur, un log, une réponse ou un `ARG` Docker. Les erreurs 500 renvoient un message
générique (détail seulement dans les logs, sans donnée sensible). Les rejets de désérialisation
(`400 invalid_query`, `422 invalid_body`, `http/extract.rs`) ont un message français fixe : ni le
texte technique de serde, ni le nom ou la valeur reçus n'y sont repris (revue ZAP-1). Journaliser les événements de
sécurité (échec d'auth admin, CSRF refusé, rate limit atteint, ALTCHA invalide) **sans** IP, jeton
ni texte. Format : `http::security::security_event`, niveau `WARN`, cible `security`, message
`security event`, champs `event` (`admin_auth_failure`, `admin_auth_lockout`, `admin_auth_refused`,
`csrf_refused`, `rate_limited`, `altcha_refused`) et `reason`, deux libellés fixes (`&'static str`) :
aucune valeur de la requête ne peut y figurer. Une panique (api, worker) est journalisée par
`telemetry::log_panic` : un événement `error` de cible `panic` avec son seul emplacement, jamais
son message (qui peut citer un texte de visiteur), à la place du hook par défaut qui écrit sur
stderr.

### 14.7 Conteneurs et chaîne d'approvisionnement

- Images non root, système de fichiers en lecture seule là où Swarm le permet (sinon
  documenter), capacités minimales, aucun outil inutile dans l'image runtime, images de base
  épinglées.
- Épinglage : `backend/Dockerfile`, `frontend/Dockerfile` et `devops/Dockerfile.test` fixent
  chaque image de base par version **et** digest de l'index multi-architecture (`ARG *_DIGEST`,
  procédure de mise à jour en tête de fichier). Runtime du backend :
  `gcr.io/distroless/cc-debian13` (glibc, libgcc, certificats CA, tzdata ; ni shell, ni
  gestionnaire de paquets, ni curl/wget : la sonde est la sous-commande `healthcheck`), même
  version de Debian (trixie) que l'image `rust` de compilation, uid 10001 (revue IMG-1). Toute
  image du compose qui n'est pas construite ici (`registry.methodinfo.fr`) est épinglée par
  `@sha256:` (`test-db`). `devops/check_image_pins.py` (CI) le vérifie : chaque `FROM` nomme un
  stage ou une image `@${*_DIGEST}` / `@sha256:`, et un `ARG` présent dans plusieurs Dockerfiles
  (`RUST_VERSION`, `RUST_DIGEST`, `NGINX_DIGEST`…) a la même valeur partout ; aucun Dockerfile
  n'est plus en attente (`PENDING` vide) : tous sont contrôlés de façon bloquante.
- Dans `devops/docker-compose.swarm.yml`, api, worker et frontend ont `read_only: true`,
  `init: true` et `cap_drop: [ALL]` (tous pris en charge par `docker stack deploy`, vérifié sur
  Docker 29) ; les seuls chemins inscriptibles sont des volumes `type: tmpfs` (la clé courte
  `tmpfs:` est ignorée par Swarm) : `/tmp` pour api et worker ; `/run` (pid, `security.txt`
  généré) et `/var/cache/nginx` pour le frontend, qui garde seulement `CHOWN`, `SETGID`,
  `SETUID` (maître nginx root, workers `nginx`) et `NET_BIND_SERVICE` (port 80). `security_opt`
  (`no-new-privileges`) est ignoré par Swarm : les processus applicatifs tournent sans privilège
  (uid 10001 pour api et worker, utilisateur `nginx` pour les workers nginx). Aucun port publié :
  tout passe par Traefik. `devops/dev/e2e.sh` lance les conteneurs avec les mêmes options.
- `devops/check_compose_security.py` (CI, stage `devops-check`) vérifie ces réglages, les
  réseaux (worker hors `proxy`, liste fermée de réseaux pour api, worker et frontend, services de
  CI sur leur réseau interne), et les routeurs Traefik :
  chacun nomme son service, son point d'entrée et une règle de la forme `Host(…)`, éventuellement
  `&& PathPrefix(…)` ou `&& PathRegexp(…)`, sur un host connu (ni `||`, ni `!`, ni parenthèses, ni
  autre matcher : la structure est vérifiée, pas une sous-chaîne) ; `global@file` sur tout
  routeur HTTPS public, `dashboard-allowlist@file` sur `ADMIN_HOST` ; les routeurs HTTP ne font que
  rediriger ; le port admin 8081 n'est servi que par `Host(ADMIN_HOST) && PathPrefix(/api/admin)`,
  derrière `dashboard-allowlist@file` (§14.4) ; `/admin` du host public redirige vers `ADMIN_HOST`.
- CI (`devops/Dockerfile.test`) : `cargo deny check advisories bans` (ou `cargo audit`) et
  `npm audit --omit=dev --audit-level=high` bloquants ; `Cargo.lock` et `package-lock.json`
  committés ; `npm ci` uniquement, sans script de cycle de vie des dépendances (`ignore-scripts=true`
  dans `frontend/.npmrc` ; un paquet qui en aurait besoin passe par un `npm rebuild <paquet>`
  explicite et une entrée justifiée de `src/test/supply-chain.test.ts`, qui vérifie le réglage et
  qu'aucun paquet verrouillé n'a de script d'installation hors de cette liste). Réalisation : `npm audit` dans le stage `frontend-audit` ;
  `cargo deny --locked --config ../devops/deny.toml check advisories bans sources` à la fin du
  stage `backend-test` (binaire officiel épinglé par version et SHA-256). `devops/deny.toml` :
  toute alerte RustSec (vulnérabilité, crate non maintenue ou unsound) et toute version retirée
  bloquent, une exception se note dans `ignore` avec sa raison et une date de réexamen ; crates
  interdites `openssl`, `openssl-sys`, `native-tls`, `aws-lc-sys` (TLS = rustls + ring) ;
  crates.io comme seule source. Ces deux contrôles interrogent leur base à chaque exécution de
  leur étape, relancée quand les sources du frontend, resp. du backend, changent, **et** quand
  le flux d'alertes change : `Dockerfile.test` lie à chaque étape (`RUN --mount=type=bind`) un
  fichier téléchargé par `ADD` à chaque build (flux Atom des commits de
  `github/advisory-database` et de `rustsec/advisory-db`), qui invalide le cache de BuildKit dès
  qu'une alerte paraît, sans rien laisser dans l'image (revue SUP-2).
- Images nginx (runtime du frontend et stage `frontend-headers`) : `apk upgrade --no-cache` après
  le `FROM` épinglé, relancé de la même façon à chaque mise à jour du flux de sécurité d'Alpine
  (`secdb.alpinelinux.org/v<NGINX_ALPINE_RELEASE>/main.json`) : les paquets corrigés sont pris
  sans attendre un nouveau digest. `NGINX_ALPINE_RELEASE` suit `/etc/alpine-release` de l'image
  (le build échoue sinon). Scan de vulnérabilités des images runtime (revue SUP-2) :
  `devops/dev/security/trivy.sh` (Trivy épinglé par version et digest, archive `docker save`
  scannée hors ligne), lancé par la campagne d'avant production (§14.8) : toute vulnérabilité
  `HIGH`/`CRITICAL` qui a un correctif fait échouer ; une exception se note dans
  `devops/dev/security/trivy-ignore.yaml` avec sa raison et une date d'expiration. Reste à faire :
  mise à jour automatique des digests (Renovate).
- Le worker n'est pas sur le réseau `proxy` ; seuls api et frontend y sont. `proxy` est partagé
  par toutes les stacks du cluster (`attachable`, non `internal`) : tant qu'un réseau dédié à
  Traefik et à iapolitique n'existe pas (`docs/ops.md` §5.4), les ports 8080 et 8081 de l'api
  sont joignables par les autres services de ce réseau (exposition résiduelle, §14.4).

### 14.8 Vérification

Tests d'intégration de sécurité obligatoires : en-têtes (§14.2) sur toutes les familles de
réponses, absence de CORS, refus CSRF (Origin étranger, mauvais Content-Type), charges
d'injection SQL dans `q`, filtres, tri et slug (aucune erreur 500, aucun résultat anormal),
XSS stocké rendu échappé (test front), injection de formule CSV, corps trop gros, JSON avec
champs inconnus, jeton mal formé, routes admin absentes du port public, auth admin. Avant chaque
mise en production, la campagne `devops/dev/security/run.sh` est rejouée sur la pile locale
(jamais sur la QA ni la production) : charges hostiles à travers toute la chaîne (faux Mistral en
mode hostile, marqueur `sondexss`) et aller-retour CSV, sondes manuelles (CORS, CSRF, `Origin`,
`Host`, tailles, isolement de l'admin, réutilisation des connexions, `X-Forwarded-For`), XSS
stocké dans Chromium et Firefox, OWASP ZAP (baseline, API scan, scan actif ciblé) ; toute alerte
ZAP de risque faible ou plus doit être corrigée ou justifiée dans `zap-rules.tsv`, puis scan Trivy
des images runtime (`trivy.sh`, §14.7). Modèle de
menace, couverture des exigences, résultats et risques résiduels : `docs/securite.md`.
