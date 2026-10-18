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
│   └── src/
│       ├── main.rs         # sous-commandes: api | worker | migrate | openapi | healthcheck
│       ├── config.rs       # lecture env + /run/secrets
│       ├── db/             # requêtes par agrégat
│       ├── domain/         # types métier, machine à états, scoring (sans I/O)
│       ├── http/           # routes publiques, routes admin, erreurs
│       ├── ai/             # clients decision service et Mistral, prompts chargés depuis /prompts
│       ├── jobs/           # file de jobs et handlers
│       └── antispam/       # ALTCHA, rate limit
├── frontend/
│   ├── Dockerfile          # build Vite puis nginx
│   ├── nginx.conf
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
├── docs/                   # documentation fonctionnelle (méthodologie, calibration…)
│   └── calibration/        # banc de calibration du filtrage, contrôle des fichiers prompts/
└── devops/
    ├── docker-compose.swarm.yml
    ├── Dockerfile.test     # image CI (§13.5)
    ├── run-tests.sh        # point d'entrée de l'image CI
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
facultatif. Protégé par ALTCHA et rate limit ([§7](#7-anonymat-anti-abus-et-données)).

### 5.2 Machine à états

```
submitted
  └─▶ screening (modèle de décision, synchrone)
        ├─▶ rejected ─────────────▶ purged (texte supprimé à J+30)
        ├─▶ second_opinion (Mistral, zone grise) ─▶ rejected | accepted
        └─▶ accepted
              └─▶ dedup (embedding + pgvector)
                    ├─▶ merged (devient une contribution de l'idée existante)
                    └─▶ queued_analysis ─▶ analyzing ─▶ published
                                                  └─▶ analysis_failed (retries puis admin)
published ─▶ hidden (après revue d'un signalement ou action admin) ─▶ published
```

- Les transitions sont définies **uniquement** dans `backend/src/domain/` et testées. Toute
  transition est horodatée dans une table d'historique.
- Réponse immédiate à l'auteur : `accepted` (« Votre idée est conservée, elle va être améliorée
  et chiffrée par IA »), `rejected` + motif lisible + invitation à reformuler, ou
  `second_opinion` (« en cours de vérification ») si l'avis Mistral n'est pas rendu dans le
  délai (10 s).
- Lien de suivi : voir [§7.1](#71-anonymat).

### 5.3 Filtrage par le modèle de décision

Service `llamacpp-decision-service`, `POST http://llamacpp-decision-service:8000/v1/systemone`
(référence : `C:\Repos\AIFabric\DECISION_SERVICE.md`). Ce n'est **pas** un LLM de chat :
n'appeler que `/v1/systemone`.

- **Une seule requête** par idée, avec toutes les questions de `prompts/screening.yaml` :
  - `injection` (`noul`) : tentative de manipulation d'une IA ;
  - `respectful` (`noul`) : pas d'insulte, de haine, d'incitation à la violence ;
  - `targeted` (`noul`) : vise nommément un parti, un élu, une personne ou un groupe
    (religion, origine, orientation…), ou promeut ou dénigre un candidat ou un parti ;
  - `personal_data` (`noul`) : désigne un particulier identifiable (nom, fonction précise dans
    un service nommé…) ou lui impute des faits (§7.5) ;
  - `on_topic` (`noul`) : propose une mesure qui réduit les dépenses ou augmente les recettes
    d'une administration publique française ;
  - `lawful` (`noul`) : compatible avec les droits fondamentaux ;
  - `realism` (`score`, 4 niveaux : irréaliste → simple à mettre en œuvre).
- Chaque critère a deux seuils configurables (rejet franc / acceptation franche). Entre les
  deux : **zone grise** → second avis Mistral avec la même grille (`prompts/second_opinion.md`).
- Motif de refus = premier critère en échec selon l'ordre ci-dessus ; libellés en français
  définis dans `screening.yaml`.
- `state` limité au texte de l'idée (< 2 000 tokens). Timeout 5 s ; si le service est
  indisponible, l'idée reste en `submitted`, un job réessaie, et l'auteur est informé via son lien
  de suivi.
- Les scores bruts sont stockés pour **calibrer les seuils** (l'admin peut étiqueter des
  décisions, voir [§8](#8-administration-lan-uniquement)). Les probabilités ne sont pas
  calibrées d'origine : ne jamais coder de seuil en dur.
- Contributions et signalements passent par le même service avec leurs propres questions
  (pertinence par rapport à l'idée, respect, injection).

### 5.4 Doublons

- Après acceptation : embedding du texte (`mistral-embed`) stocké en `vector`, recherche du plus
  proche voisin parmi les idées publiées (distance cosinus, index HNSW).
- Similarité ≥ `DEDUP_SIMILARITY_THRESHOLD` → l'idée devient une **contribution** de l'idée
  existante (`merged`), le compteur « proposée N fois » augmente et l'idée parente est
  ré-analysée. Le lien de suivi de l'auteur pointe vers l'idée parente.
- Le seuil se calibre via l'admin ; en cas de doute, ne pas fusionner.

### 5.5 Contributions (améliorations et objections)

Pas de commentaires libres. Sur une idée publiée, un visiteur peut soumettre une amélioration ou
une objection (même protection anti-abus que le dépôt). Elle est filtrée, puis, si acceptée,
déclenche une **ré-analyse** de l'idée qui l'intègre. Le portail affiche le nombre de
contributions et le résumé neutre de celles qui ont été prises en compte, jamais le texte brut.

### 5.6 Signalements

Bouton « Signaler » sur chaque idée publiée : motif (erreur de chiffrage, source erronée,
contenu problématique, doublon, autre) + précision facultative. Le signalement est filtré, puis
traité **par IA** : Mistral réévalue l'idée avec le signalement (`prompts/report_review.md`) et
conclut `maintain`, `update` (nouvelle analyse) ou `hide` (masquée, visible dans l'admin), avec
un drapeau `escalate` pour les contenus graves (masquage immédiat et file d'escalade humaine,
§7.5). Les signalements d'une même idée sont regroupés : au plus une réévaluation par idée et
par 24 h, sauf `contenu_problematique`, traité sans délai. Formulaire et obligations : §7.5.

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
- **Recherche web obligatoire** pour le chiffrage (outil `web_search` de l'API Agents /
  Conversations de Mistral). Vérifier la documentation Mistral en vigueur au moment de
  l'implémentation (noms d'outils, sortie structurée).
- Sortie **JSON validée contre un schéma** côté Rust. Si la recherche web et la sortie
  structurée ne sont pas compatibles dans un même appel : étape 1 recherche (texte + sources),
  étape 2 structuration via `response_format` JSON schema. Toute sortie invalide est rejetée et
  réessayée (3 tentatives max, backoff exponentiel), puis `analysis_failed`.
- 429 / 5xx : retry avec backoff ; ne jamais boucler sans limite.

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
| `sources` | `{ id, title, publisher, url, used_for }` ; au moins une source pour tout chiffre clé, citée `[S1]`, `[S2]`… dans les textes |
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
  coût estimé, version de prompt, idée, latence, statut).
- Plafond journalier `MISTRAL_DAILY_BUDGET_EUR` (prix unitaires en variables d'environnement
  car ils changent). Plafond atteint : les jobs restent en file jusqu'au lendemain (minuit
  Europe/Paris) ; les seconds avis attendent aussi (l'auteur voit « en cours de vérification »).
- Priorité de la file : second avis > signalement > nouvelle idée > contribution > relance admin.
- QA : plafond bien plus bas (dans `.env.qa`).

### 6.5 Versionnement et méthodologie publique

- Chaque fichier de `prompts/` porte une version (`version:` en tête). Chaque décision et chaque
  analyse stockent les versions de prompt, de grille et de score utilisées.
- La page publique « Méthodologie » affiche les prompts, la grille de filtrage, la taxonomie et
  la formule de score **tels qu'utilisés en production** (lus depuis l'API, pas recopiés).

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
  - La page de suivi propose « Supprimer ma contribution » (effacement immédiat, retrait des
    exports suivants) et « Demander un réexamen humain » (file admin).
  - Un refus affiche l'exposé des motifs (DSA art. 17) : mesure prise, règle en cause (article
    de `/regles`), « décision prise automatiquement par une IA », date de purge, voies de recours
    (réexamen humain, contact, juge).
- Logs applicatifs : jamais d'IP, de jeton, d'en-têtes d'identification ni de texte d'idée
  (seulement des identifiants internes).

### 7.2 Anti-abus

- **ALTCHA** (proof-of-work auto-hébergé, clé `ALTCHA_HMAC_KEY`) sur dépôt, contribution et
  signalement. Aucun captcha tiers.
- **Rate limit** applicatif par `HMAC(sel_du_jour, ip)` : le sel est aléatoire, gardé **en
  mémoire uniquement** (jamais en base, sur disque, dans un log ou une sauvegarde), renouvelé
  toutes les 24 h puis détruit ; les compteurs vivent aussi en mémoire et expirent en 24 h au
  plus. L'api tourne en une réplique ; avec plusieurs répliques, la limite s'appliquerait par
  réplique (acceptable). Les tables `rate_limit_salts` / `rate_limit_counters` du schéma
  initial ne sont pas utilisées. L'IP en clair n'est jamais persistée. L'IP cliente provient de
  `X-Forwarded-For` posé par Traefik (ports publiés en mode `host` : Traefik voit l'IP réelle).
- Aucune empreinte HMAC n'est stockée avec une idée, une contribution ou un signalement.
- Limites par défaut (configurables) : 5 idées / heure et 20 / jour, 20 contributions / jour,
  20 signalements / jour par empreinte.
- Le WAF et le rate limit global de Traefik (`global@file`) s'appliquent en plus.

### 7.3 Rétention

- Idée refusée : texte et jeton purgés **30 jours** après le refus ; seules des statistiques
  agrégées sont conservées (date, motif, niveau, scores du modèle de décision pour la calibration
  sans le texte).
- Idées publiées : conservées avec leur texte original (version masquée, voir §7.5).
- Contenus masqués (`hidden`) : texte purgé après 12 mois.
- Signalements traités : purgés après 6 mois (coordonnées facultatives du signalant comprises).
- Contribution supprimée par son auteur : effacement immédiat.
- Job de purge quotidien dans le worker, testé.

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
- **Exclus** : idées refusées, masquées ou en attente, jetons et hash, journaux de filtrage,
  signalements, coordonnées, horodatages fins (dates au jour près).
- Régénéré chaque nuit et après tout masquage ou suppression ; fichiers datés et versionnés.
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
  `cgu_version` et `cgu_accepted_at` (au jour près) avec chaque contribution, rien d'autre.
- **Données personnelles dans les textes** : avant tout appel à Mistral et avant publication,
  masquage déterministe (e-mails, téléphones, IBAN, NIR, adresses postales, URL nominatives →
  `[donnée masquée]`) ; et critère de filtrage `personal_data` (§5.3) qui refuse un texte
  désignant un particulier identifiable, avec invitation à reformuler. Les élus et ministres
  agissant dans leur fonction ne sont pas des particuliers, mais le critère `targeted` refuse
  toute attaque ou promotion d'un parti, d'un candidat ou d'un élu. Le texte « original »
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
  accusé de réception sur la page de suivi, mention du traitement automatisé.
- **Escalade humaine** : la revue de signalement marque `escalate` les contenus graves
  (diffamation, menace, haine, terrorisme, pédocriminalité) : masquage immédiat et file admin
  « escalade » (transmission manuelle à PHAROS, cible 24 h).
- **Droit de réponse** (LCEN art. 1-1 III) : l'admin peut publier une réponse attachée à une
  idée (insertion sous 3 jours).
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
- Fonctions : tableau de bord (file de jobs, taux d'acceptation, coûts IA du jour/mois), idées
  masquées et signalements, masquer/republier, relancer une analyse, consulter les scores bruts
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
  avec `dashboard-allowlist@file`. Hosts lus depuis `TRAEFIK_HOST` et `ADMIN_HOST` (le préfixe
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
- Variables non sensibles : `TRAEFIK_HOST`, `ADMIN_HOST`, `LLM_NETWORK`, `DECISION_SERVICE_URL`,
  `MISTRAL_ANALYSIS_MODEL`, `MISTRAL_REVIEW_MODEL`, `MISTRAL_EMBED_MODEL`,
  `MISTRAL_DAILY_BUDGET_EUR`, prix unitaires Mistral, `DEDUP_SIMILARITY_THRESHOLD`, limites de
  rate limit, `RUST_LOG`.

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
| healthcheck | `iapolitique healthcheck [--url URL] [--timeout-secs N]` : GET HTTP, code de sortie 0 si 2xx (URL par défaut `http://127.0.0.1:8080/api/health` ; worker : `--url http://127.0.0.1:8082/health`). L'image runtime n'a ni curl ni wget. | — |
| frontend | nginx (image `nginx:alpine`, workers non root) | 80 (`/health` : sonde de vivacité) |

Variables de test : `DATABASE_URL` (utilisée **uniquement** par `#[sqlx::test]` en test). En
déploiement, la base est lue depuis `DATABASE_CONNECTION_STRING`.

Variables lues par le backend (`backend/src/config.rs`, valeur vide = non définie ; une erreur de
configuration liste tous les problèmes et empêche le démarrage) :

| Variable | Défaut | Rôle |
|---|---|---|
| `DATABASE_CONNECTION_STRING` | **requis** | URL `postgres://` (secret). Une liste d'hôtes `h1,h2` est réduite au premier. |
| `ADMIN_API_TOKEN`, `ALTCHA_HMAC_KEY` | **requis pour l'api** | secrets, 32 caractères minimum |
| `MISTRAL_API_KEY` | absent | secret ; sans clé, le worker laisse les jobs IA en file (avertissement) |
| `BIND_ADDRESS`, `PUBLIC_PORT`, `ADMIN_PORT`, `WORKER_HEALTH_PORT` | `0.0.0.0`, 8080, 8081, 8082 | écoute |
| `HTTP_REQUEST_TIMEOUT_SECS`, `HTTP_BODY_LIMIT_BYTES` | 30, 65536 | limites HTTP |
| `TRUSTED_PROXY_HOPS` | 1 | nombre de proxys (Traefik) qui ajoutent à `X-Forwarded-For` |
| `DATABASE_MAX_CONNECTIONS`, `DATABASE_ACQUIRE_TIMEOUT_SECS` | 10, 5 | pool PostgreSQL |
| `WORKER_CONCURRENCY`, `WORKER_POLL_INTERVAL_MS`, `JOB_LOCK_TIMEOUT_SECS` | 2, 2000, 300 | file de jobs |
| `DECISION_SERVICE_URL`, `DECISION_SERVICE_TIMEOUT_SECS` | `http://llamacpp-decision-service:8000/v1/systemone`, 5 | modèle de décision |
| `MISTRAL_API_BASE_URL` | `https://api.mistral.ai` | surchargeable pour les tests |
| `MISTRAL_ANALYSIS_MODEL`, `MISTRAL_REVIEW_MODEL`, `MISTRAL_EMBED_MODEL` | `mistral-large-latest`, = analyse, `mistral-embed` | modèles |
| `MISTRAL_DAILY_BUDGET_EUR` | 5 | plafond journalier (décimal, `.` ou `,`) |
| `MISTRAL_ANALYSIS_INPUT_EUR_PER_MTOK`, `MISTRAL_ANALYSIS_OUTPUT_EUR_PER_MTOK` | 2, 6 | prix par million de tokens |
| `MISTRAL_REVIEW_INPUT_EUR_PER_MTOK`, `MISTRAL_REVIEW_OUTPUT_EUR_PER_MTOK` | = analyse | idem pour second avis et revue |
| `MISTRAL_EMBED_EUR_PER_MTOK`, `MISTRAL_WEB_SEARCH_EUR_PER_CALL` | 0.1, 0.03 | embeddings, recherche web |
| `DEDUP_SIMILARITY_THRESHOLD` | 0.95 | similarité cosinus de fusion (valeur prudente, à calibrer) |
| `RATE_LIMIT_IDEAS_PER_HOUR`, `RATE_LIMIT_IDEAS_PER_DAY` | 5, 20 | par empreinte |
| `RATE_LIMIT_CONTRIBUTIONS_PER_DAY`, `RATE_LIMIT_REPORTS_PER_DAY` | 20, 20 | par empreinte |
| `PROMPTS_DIR` | `/app/prompts` | prompts versionnés |
| `RUST_LOG`, `LOG_FORMAT` | `info,sqlx=warn`, `json` | logs (`text` en dev) |
| `TRAEFIK_HOST`, `ADMIN_HOST` | absents | informatifs |

### 13.2 Routes HTTP

Toutes les routes sont servies sous `/api` (pas de strip prefix chez Traefik).

API publique (port 8080) :

| Méthode | Route | Rôle |
|---|---|---|
| GET | `/api/health`, `/api/ready` | vivacité, base joignable |
| GET | `/api/altcha/challenge` | défi proof-of-work |
| POST | `/api/ideas` | dépôt d'une idée → statut du filtrage + jeton de suivi |
| POST | `/api/track` | suivi d'une idée, contribution ou signalement (corps `{ "token" }`, jamais dans l'URL) |
| POST | `/api/track/delete` | suppression par l'auteur (corps `{ "token" }`) |
| POST | `/api/track/review-request` | demande de réexamen humain (corps `{ "token", "message"? }`) |
| GET | `/api/legal` | coordonnées de l'éditeur (`LEGAL_*`) et versions/dates des pages légales et des CGU |
| GET | `/api/ideas` | liste publiée : `sort` (`score`, `gain`, `feasibility`, `delay`, `recent`), filtres `admin_level`, `category`, `kind`, `territory`, `q`, pagination `page`/`per_page` |
| GET | `/api/ideas/{slug}` | détail : idée, analyse courante, contributions intégrées, compteur de propositions |
| POST | `/api/ideas/{slug}/contributions` | amélioration ou objection |
| POST | `/api/ideas/{slug}/reports` | signalement |
| GET | `/api/methodology` | prompts, grilles, taxonomie, formule de score et leurs versions |
| GET | `/api/taxonomy` | niveaux, catégories, publics, motifs |
| GET | `/api/stats` | compteurs publics |
| GET | `/api/exports/ideas.json`, `/api/exports/ideas.csv` | exports open data |
| GET | `/api/openapi.json` | spec OpenAPI |

API admin (port 8081, `Authorization: Bearer $ADMIN_API_TOKEN`), sous `/api/admin/…` :
tableau de bord, file de jobs, coûts IA, idées masquées et signalements, masquer/republier,
relancer une analyse, décisions de filtrage avec étiquetage (et export JSONL pour
`docs/calibration/bench_screening.py`), réglages des seuils, file d'escalade, demandes de
réexamen humain, publication d'un droit de réponse.
`GET /api/admin/session` vérifie le jeton (`200 { "authenticated": true }` ou `401`). Le port
admin sert aussi, sans jeton, `/api/health`, `/api/ready` et `/api/openapi.json` (spec complète ;
celle du port public omet les routes admin). Le worker répond sur `/health` (et `/api/health`)
du port 8082 : `503` si sa boucle est bloquée.

Erreurs : toute réponse 4xx/5xx a le corps JSON du §4.1, y compris 404, 405, 413 (corps au-delà
de `HTTP_BODY_LIMIT_BYTES`), 504 (`timeout`, au-delà de `HTTP_REQUEST_TIMEOUT_SECS`) et 500.
Chaque réponse porte un en-tête `x-request-id` généré par le serveur.

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
| `screening.yaml` | `version` + trois sections `idea`, `contribution`, `report`. Chaque section : liste ordonnée de critères `{ key, type: noul\|score, instructions, criteria?, pass_when (true/false pour noul, niveau minimal pour score), reject_below, accept_above, reason_code, reason_message }`. Les seuils portent sur la probabilité de **réussir** le critère (`noul` ou `1 − noul` selon `pass_when` ; pour `score`, somme des probabilités des niveaux d'indice ≥ `pass_when`) ; entre `reject_below` et `accept_above` = zone grise. `state` envoyé : texte brut (idea), objet `{ idee, type, contribution }` (contribution), objet `{ idee, motif, precision }` (report), où `idee` est le résumé neutre de l'idée publiée. Calibration : `docs/calibration/`. |
| `analysis_research.md`, `analysis.md`, `second_opinion.md`, `report_review.md` | En-tête YAML entre deux lignes `---` : `version`, `model_hint`, `tools` (`[web_search]` ou `[]`), `output_schema` (fichier de schéma ou `null`), `variables` (map nom → description ; toutes doivent être substituées, une valeur absente devient « (aucun) » ou « (aucune) » selon la description). Corps en français découpé par la ligne `<!-- user -->` : avant = instructions système (seule variable admise : `{{today}}`, pour garder le cache de prompt), après = message utilisateur avec les variables `{{nom}}`. Les textes de visiteurs sont insérés entre balises (`<idee_originale>`…) : le backend neutralise toute balise fermante identique présente dans le texte. |
| Analyse en deux étapes | Étape 1 `analysis_research.md` (Conversations API avec `web_search`) → mémo sourcé en texte ; étape 2 `analysis.md` (`response_format` json_schema strict, sans outil) avec `{{research}}` = le mémo. Si la recherche web et la sortie structurée sont compatibles dans un même appel, `analysis.md` seul suffit avec `{{research}}` vide et l'outil `web_search` activé. Les versions des deux fichiers sont stockées avec l'analyse. |
| `analysis.schema.json` | JSON Schema (draft 2020-12, sous-ensemble compatible avec le mode strict de Mistral : `type`, `properties`, `required` = toutes les propriétés, `additionalProperties: false`, `items`, `enum`, `description`, `title`) de la sortie d'analyse (§6.2), utilisé tel quel en `response_format` (`{"type": "json_schema", "json_schema": {"name": "analysis", "schema": …, "strict": true}}`) et pour valider la réponse. Le backend vérifie en plus : low ≤ median ≤ high, montants et délai ≥ 0, catégorie compatible avec `admin_level`, références `[Sx]` présentes dans `sources`, au moins une source. |
| `second_opinion.schema.json`, `report_review.schema.json` | Idem pour le second avis (`decision: accept\|reject`, `reason_code` = `none` ou un `reason_code` de `screening.yaml`, `explanation`) et la revue de signalement (`outcome: maintain\|update\|hide`, `explanation`, `corrections` : consignes passées en `{{report}}` à la ré-analyse si `update`, vide sinon). |
| Contrôle | `python docs/calibration/check_prompts.py` vérifie formats, variables, mode strict et synchronisation des énumérations avec `taxonomy.yaml` et `screening.yaml` (à lancer après toute modification de `prompts/`). |
| `taxonomy.yaml` | `version`, `admin_levels` (`{ key, label, description, category_groups }`), `categories` (map groupe → liste `{ key, label, description }` ; groupes `etat`, `securite_sociale`, `collectivites`, `transverse` ; clé de catégorie unique dans tout le fichier ; un niveau n'accepte que les catégories de ses `category_groups`), `impact_publics`, `impact_effects`, `idea_kinds`, `confidence_levels`, `legal_vehicles`, `report_reasons`, `contribution_kinds` (listes `{ key, label, description? }`), libellés français. |

### 13.5 Tests en CI

Le service `test` du compose construit `devops/Dockerfile.test` (contexte `..`) :
1. stage `frontend-check` (node 22) : `npm ci`, lint, typecheck, Vitest, build ;
2. stage `backend-test` (rust) : `cargo fmt --check`, `cargo clippy -D warnings`, compilation des
   tests ; il copie un fichier témoin produit par `frontend-check` (sinon BuildKit saute ce
   stage) ;
3. à l'exécution (`devops/run-tests.sh`) : attente du port de `test-db`, puis
   `cargo test --locked --offline` (rien n'est recompilé) avec `DATABASE_URL` vers le service
   `test-db` (`pgvector/pgvector:pg18` en mémoire, sans mot de passe, réseau interne au compose).
   `test-db` n'est **pas** en `replicas: 0` (compose ne démarrerait pas la dépendance) mais en
   `mode: global` contraint sur une étiquette qu'aucun nœud ne porte : zéro tâche sur le swarm.
   Le service `test` lui-même est en `replicas: 0`.

Les deux stages utilisent des cache mounts BuildKit (npm, registre cargo, `target/`) ; le
`target/` est recopié dans l'image pour que `cargo test` n'ait rien à recompiler. La version de
Rust de `Dockerfile.test` (`RUST_VERSION`) suit celle de `backend/Dockerfile`.

En local, sans Node installé : `docker run --rm -v "<repo>/frontend:/app" -v iapolitique_node_modules:/app/node_modules -w /app node:22-alpine npm …`.
Postgres local de test : `docker run -d --name iapolitique-test-pg -e POSTGRES_PASSWORD=postgres -p 55432:5432 pgvector/pgvector:pg18`
puis `DATABASE_URL=postgres://postgres:postgres@localhost:55432/postgres cargo test`.
