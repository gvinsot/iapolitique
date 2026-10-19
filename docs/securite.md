# Sécurité d'iapolitique

Synthèse de la sécurité de la plateforme au 2026-10-19 : modèle de menace, contrôles en place et
tests qui les couvrent (exigences de [AGENTS.md §14](../AGENTS.md#14-sécurité)), résultats de
l'audit et de la campagne dynamique (OWASP ZAP, sondes manuelles, XSS stocké en navigateur),
risques résiduels, actions d'infrastructure hors dépôt, et comment rejouer les contrôles.

AGENTS.md reste la référence contractuelle : en cas d'écart, il prévaut et ce document doit être
corrigé. Les opérations (secrets, sauvegardes, Traefik) sont décrites dans [ops.md](ops.md).

## 1. Modèle de menace

### 1.1 Biens à protéger

| Bien | Pourquoi | Où |
|---|---|---|
| Anonymat des auteurs | Principe non négociable (§1, §7.1) : aucune IP, aucun compte, aucune donnée permettant de relier un texte à une personne | api (empreinte HMAC en mémoire), journaux, Traefik |
| Textes des visiteurs avant masquage, coordonnées des signalants | Données personnelles (RGPD), possibles propos illicites | PostgreSQL, purge §7.3 |
| Jetons de suivi | Seul moyen d'agir sur sa contribution (suivi, suppression, réexamen) | fragment d'URL, corps `POST`, SHA-256 en base |
| Intégrité du contenu publié | Analyses IA, score, classement, exports open data : responsabilité de l'éditeur (diffamation, désinformation) | api, worker, exports |
| Jeton d'administration | Masquage, republication, droits de réponse, réglages du filtrage | secret Swarm, `sessionStorage` du navigateur admin |
| Secrets techniques | Base, clé Mistral, clé HMAC ALTCHA | secrets Swarm (`/run/secrets`) |
| Budget Mistral | Coût réel par appel | table `ai_calls`, plafond journalier §6.4 |
| Disponibilité | Dépôt, consultation, modération | Swarm partagé avec d'autres stacks |

### 1.2 Acteurs

- **Visiteur légitime**, anonyme, depuis un navigateur.
- **Visiteur malveillant** : spam, propos illicites, injection de prompt directe (dans l'idée, la
  contribution, le signalement, le territoire), XSS stocké, injection de formule CSV, abus des
  signalements, tentative de désanonymisation d'un auteur.
- **Bot scripté** : résout ALTCHA nativement (quelques millisecondes), fait tourner ses IP,
  cherche à épuiser le budget Mistral ou les limites des autres visiteurs.
- **Site tiers** : CSRF, clickjacking, lecture cross-origin (CORS).
- **Contenu web hostile** : pages consultées par la recherche web de Mistral, qui peuvent porter
  une injection indirecte ou des URL piégées.
- **Fournisseur IA** : réponse malformée, démesurée ou contraire au schéma.
- **Administrateur** : de confiance, mais sur le LAN seulement ; son navigateur est une cible
  (XSS dans les pages admin, jeton).
- **Conteneur voisin du cluster** : services d'autres stacks sur les réseaux overlay partagés
  (`proxy`, `postgresqlcluster_internal`, `llm-net`), voir NET-1.
- **Chaîne d'approvisionnement** : paquets npm, crates, images de base, CI.

### 1.3 Frontières de confiance

```
Internet ──TLS──▶ Traefik (WAF Coraza/CRS, rate limit, global@file)
                    ├──▶ nginx (frontend, port 80)            CSP stricte, GET/HEAD seuls
                    └──▶ api :8080 (/api)                      anti-CSRF, ALTCHA, rate limits
LAN ──TLS──▶ Traefik (ADMIN_HOST, dashboard-allowlist@file)
                    └──▶ api :8081 (/api/admin)                jeton Bearer, verrouillage
api / worker ──▶ PostgreSQL (overlay postgresqlcluster_internal)
api / worker ──▶ modèle de décision (overlay llm-net)          sémaphore, délai, 256 Kio max
worker ──HTTPS──▶ Mistral (Internet) ──▶ pages web consultées  schéma strict, sources vérifiées
navigateur ◀── contenu des visiteurs et de l'IA                texte brut, React, CSP
réseaux overlay partagés ──▶ api :8080 / :8081 (sans Traefik)  exposition résiduelle NET-1
GitHub ──▶ PulsarCD (build, tests, QA) ──▶ registre ──▶ Swarm   CI bloquante (§13.5)
```

Chaque frontière suppose l'autre côté hostile : le WAF et le rate limit de Traefik ne dispensent
d'aucun contrôle applicatif (§14), la sortie de l'IA est traitée comme une entrée de visiteur, et
le navigateur ne fait confiance qu'à son origine.

## 2. Contrôles et tests, par exigence de AGENTS.md §14

| § | Contrôle | Tests automatisés (CI sauf mention) |
|---|---|---|
| 14.1 SQL | Requêtes `&'static str` paramétrées, tri par `enum` vers fragments constants, `strpos` au lieu de `LIKE`, recherche par `websearch_to_tsquery` | `source_audit_tests.rs` (interdit `format!` SQL, `AssertSqlSafe`, `QueryBuilder`, `raw_sql`), `security_tests::sql_injection_payloads_are_inert`, `input_tests.rs` ; local : `probes.py` (famille `limits`), ZAP (règles 40018 à 40024, 90018…) |
| 14.1 Prompts | Texte des visiteurs hors de la partie système, encadré de balises ; NFKC puis neutralisation de tout chevron ; critère `injection` du modèle de décision ; sortie Mistral validée par schéma strict, contrôles sémantiques, `check_ai_text` (contrôles, bidi, longueurs) ; sources limitées aux pages consultées, citées par identifiant `Rn` et remplies par le backend (`ConsultedPages`) ; seul outil `web_search` | `ai/prompts.rs`, `ai/outputs.rs`, `jobs/analysis.rs` (tests unitaires et `sqlx::test`) ; local : `seed.py` (des URL consultées porteuses de charges sont publiées comme données ; le modèle ne peut plus écrire d'URL) |
| 14.1 XSS | React, aucun puits HTML ; ESLint `react/no-danger` & co ; liens de sources `http(s)` seulement, `rel="noopener noreferrer nofollow ugc"` ; JSON-LD échappé (`aiJsonLdScript`) ; `mailto:` seulement pour une adresse stricte | `src/test/xss-guards.test.ts`, tests des composants ; local : `xss_check.mjs` (Chromium et Firefox) |
| 14.1 CSV | Préfixe `'` des cellules `= + - @ TAB CR`, BOM, RFC 4180 | tests de `exports` ; local : `seed.py` (aller-retour d'un titre `=HYPERLINK(…)` produit par l'IA) |
| 14.1 Désérialisation | `deny_unknown_fields`, longueur de chaque chaîne, pagination bornée, jeton vérifié avant hachage ; message de rejet fixe, sans écho (ZAP-1) | `source_audit_tests.rs`, `input_tests.rs`, `http/extract.rs` (`rejections_do_not_echo_the_request`) |
| 14.2 En-têtes | Couche unique `http/security.rs` (api) ; `frontend/nginx.conf` (CSP à nonce et empreintes, COEP, HSTS…) | `security_tests::public_responses_carry_the_security_headers`, `admin_responses_…` ; stage CI `frontend-headers` (`check_frontend_headers.sh`) ; `src/test/csp.test.tsx` ; local : `browser_check.sh`, `probes.py` (famille `headers`), ZAP baseline |
| 14.3 CORS / CSRF | Aucun `Access-Control-*`, `OPTIONS` → 405 ; `Content-Type: application/json` exigé ; `Origin` et `Sec-Fetch-Site` vérifiés par port | `http/csrf.rs`, `security_tests::cors_preflights_are_refused_and_never_answered`, `cross_site_writes_are_refused` ; local : `probes.py` (familles `cors`, `csrf`, `methods`) |
| 14.4 Admin | Port 8081 routé seulement sur `ADMIN_HOST` derrière `dashboard-allowlist@file` ; jeton ≥ 32 caractères, temps constant, verrouillage 10 échecs / 5 min ; port public sans route admin ; jeton en `sessionStorage` | `security_tests::admin_authentication_locks_after_repeated_failures`, `the_public_port_knows_no_admin_route` (toutes les opérations de la spec), `check_compose_security.py` (stage `devops-check`) ; local : `probes.py` (famille `admin`) |
| 14.5 Abus / DoS | ALTCHA, rate limits par empreinte, limite de corps, délai de requête, `statement_timeout`, sémaphore du modèle de décision, budget Mistral, réponses IA plafonnées, pagination bornée, limites Swarm ; corps non lus vidés avant la réponse (ZAP-2) | tests de `antispam/`, `ai/`, `http/middleware.rs` (`bodies_over_the_limit_are_rejected`, `unread_request_bodies_are_drained`, `rejected_requests_keep_the_connection_usable_on_a_real_server`, `a_missing_body_closes_the_connection_on_a_real_server`), `check_compose_security.py` ; local : `probes.py` (familles `limits`, `connection`, `xff`) |
| 14.6 Secrets / journaux | `Secret` à `Debug` masqué, erreurs 500 génériques, `security_event` à libellés fixes, paniques sans message | `security_tests::csrf_refusals_are_logged_without_ip_nor_content`, `abuse_refusals_are_security_events_without_ip_nor_text`, tests de `telemetry` ; local : `e2e.sh` (aucune IP, jeton, secret ni texte dans les journaux des conteneurs) |
| 14.7 Conteneurs / supply chain | Images non root, lecture seule, `cap_drop: ALL`, `init`, tmpfs, limites ; images épinglées par digest ; `cargo deny`, `npm audit`, `ignore-scripts` | `check_compose_security.py`, `check_image_pins.py` et leurs tests, stages `frontend-audit` et `backend-test` (cargo deny), `src/test/supply-chain.test.ts`, `scripts/check-dist-urls.mjs` ; local : `trivy.sh` (vulnérabilités des images runtime, §4.5) |
| 14.8 Vérification | Campagne dynamique locale avant chaque mise en production | `devops/dev/security/run.sh` (§7) |

## 3. Synthèse de l'audit

L'audit du 2026-10-19 s'est fait en trois temps : mise en conformité avec §14 (11 commits backend,
8 commits frontend et devops), revue de sécurité par des relecteurs indépendants (injections, web,
authentification, abus, données personnelles, chaîne d'approvisionnement), puis la campagne
dynamique décrite au §4. Chaque correction est un commit `fix(security)` avec son test de
non-régression et l'identifiant du constat dans le message.

Sévérités : celles consignées par la revue quand elle les a données ; les autres (marquées \*)
sont évaluées pour cette synthèse, selon l'impact et la facilité d'exploitation.

### 3.1 Constats corrigés

| Id | Sévérité | Constat | Correction |
|---|---|---|---|
| ZAP-2 | moyenne\* | Une requête rejetée avant lecture de son corps (route inconnue, 403/415 anti-CSRF, 405, 413) faisait fermer la connexion par hyper sans `Connection: close` ; Traefik, qui partage ses connexions vers l'api entre clients, y envoyait la requête suivante d'un autre visiteur et répondait `500` sans rejouer un `POST` | L'api lit et jette le reste d'un corps non lu avant de répondre (1 Mio, 2 s au plus), `Connection: close` au-delà (`http::middleware::drain_unread_body`) ; un premier correctif qui fermait toujours la connexion transformait les `413` en `502` derrière Traefik et a été remplacé |
| PRIV-3 | moyenne | Le filtrage ne refusait pas un auteur qui se nomme ou laisse ses coordonnées | Critère `contact_data` (grille 1.2.0), calibré |
| BUD-1 | moyenne\* | Un appel Mistral payé pour une idée supprimée entre-temps n'était pas compté dans le budget | Coût enregistré quoi qu'il arrive, arrêt avant la seconde étape payante |
| PRIV-1, PRIV-2 | moyenne\* | Formes de téléphone, e-mail, IBAN, RIB, NIR, adresse, URL de profil qui échappaient au masquage | Masquage étendu, caractères pleine chasse repliés |
| INJ-1 | moyenne\* | Territoire non filtré et inséré tel quel dans les prompts | Filtré avec l'idée, caractères de nom de lieu seulement (`422 invalid_territory`) |
| INJ-2 | moyenne\* | Un texte de visiteur pouvait fermer sa balise de cadre dans le prompt | NFKC puis remplacement de tout chevron des valeurs substituées |
| INJ-3 | moyenne\* | Une analyse pouvait citer des URL plantées par un visiteur ou une page web | Sources limitées aux pages consultées par la recherche ou à l'analyse remplacée. Depuis le 2026-10-19 (deux échecs en production : URL réécrites par le modèle refusées, puis recherche sans page lue), le modèle n'écrit plus d'URL : les pages (`tool_reference`) sont numérotées `R1`, `R2`… dans le prompt, la réponse les cite par identifiant (`enum` du schéma envoyé), et titre, site et URL publiés sont recopiés de la liste par le backend (`ai::outputs::ConsultedPages`) ; un identifiant inconnu est redemandé avec la liste des identifiants valides ; une recherche sans page est redemandée, puis le job est relancé par la file, puis `analysis_failed` : jamais de publication sans page consultée. Tests : `ai::outputs` (`a_published_url_is_always_a_consulted_page`), `jobs::analysis` (URL plantée dans l'idée, le mémo et la réflexion) |
| RL-1 | moyenne\* | Table de limitation pleine : refus de tout nouveau visiteur ; comptage avant ALTCHA | Comptage après ALTCHA valide, une table par action, éviction des plus anciens |
| EXP-2 | moyenne | Une idée masquée (signalement `contenu_problematique`, escalade, admin) ou retirée par son auteur restait téléchargeable dans les exports jusqu'à leur régénération, différée de 2 minutes pour regrouper les changements, voire au-delà si une génération en cours l'avait déjà lue | Corrigé le 2026-10-19 : régénération immédiate et prioritaire après tout masquage, retrait ou suppression (`pipeline::regenerate_exports_now`), sans regroupement ; une génération ne s'enregistre que si toutes ses idées sont encore publiées (`db::exports::still_published`, verrou `FOR SHARE`) ; `e2e.sh` vérifie la sortie des exports en quelques secondes |
| EXP-1 | moyenne\* | Chaque téléchargement d'export chargeait une copie, et un `304` relisait le fichier : quelques dizaines de lecteurs lents dépassaient la mémoire de l'api | `304` sans lecture, une copie partagée en mémoire |
| ZAP-1 | faible\* | Les erreurs `400 invalid_query` / `422 invalid_body` renvoyaient le texte anglais de serde, qui cite la valeur reçue (`unknown variant <script>…`) | Message français fixe, sans écho |
| INJ-4, INJ-5, INJ-6 | faible\* | Caractères invisibles dans les textes, métacaractères `LIKE` après `unaccent`, caractères de contrôle et longueurs dans les sorties IA | Normalisation, `strpos`, `check_ai_text` et échec définitif sans relancer les appels payants |
| INJ-7 | faible\* | Un `mailto:` construit sur une adresse pouvait porter des champs d'en-tête (`cc`, `body`) | Motif strict `frontend/src/lib/email.ts` |
| PRIV-5, PRIV-6, PRIV-7 | faible\* | Dates d'analyse à la seconde, détails de signalements non revus jamais purgés, résumé d'un doublon supprimé resté dans l'analyse parente | Dates au jour, purge, retrait du résumé |
| AUTHZ-1 (phase 1) | faible\* | Suppression par l'auteur d'une idée en escalade sans trace | Événement `escalation.deleted_by_author` au journal admin |
| ADM-1, FE-5 | faible\* | Formulaire de jeton admin affiché sur l'hôte public ; `/admin` public servi | Pas de formulaire hors `ADMIN_HOST`, redirection Traefik |
| DEVOPS-7 | faible\* | Le contrôle du compose cherchait des sous-chaînes dans les règles Traefik | Analyse de la structure des règles |
| HDR-2, LOG-6 | faible\* | `interest-cohort` dans la Permissions-Policy de l'api ; message des paniques journalisé | Liste de nginx ; emplacement seul |
| IMG-1, SUP-1 (backend) | faible\* | `backend/Dockerfile` non épinglé par digest, runtime Debian complet (shell, gestionnaire de paquets) | Corrigé le 2026-10-19 (commit `32c3b20`) : images de build et de runtime épinglées par version et digest, runtime `gcr.io/distroless/cc-debian13` (uid 10001, sans shell ni curl), contrôle `check_image_pins.py` bloquant pour tous les Dockerfiles |
| SUP-2 (scan) | faible\* | Aucun scan de vulnérabilités des images construites | Corrigé le 2026-10-19 : `devops/dev/security/trivy.sh` (Trivy épinglé par digest) dans la campagne d'avant production, HIGH et CRITICAL corrigeables bloquants, exceptions justifiées et datées dans `trivy-ignore.yaml` ; résultats au §4.5 |
| DOS-1, SUP-1, SUP-2, SUP-3 | faible\* | Pas de limites Swarm ; image de CI non épinglée ; paquets Alpine et alertes figés en cache ; scripts d'installation npm | Limites, digest, `apk upgrade` et audits relancés au fil des alertes, `ignore-scripts` |

### 3.2 Constats ouverts

| Id | Sévérité | Constat | État |
|---|---|---|---|
| NET-1 | moyenne | Tout conteneur des réseaux overlay partagés joint l'api 8080 (sans WAF ni rate limit, en choisissant son `X-Forwarded-For`) et 8081 (jeton seul, peut déclencher le verrouillage) | **Reporté par l'éditeur** (décision D4 du 2026-10-19) : risque résiduel accepté et documenté (AGENTS.md §14.4, §5 ci-dessous). Procédure prête, pas à pas : ops.md §5.4 (réseau dédié côté infrastructure, puis confiance en `X-Forwarded-For` limitée au pair Traefik). Reproduit localement : `probes.py`, famille `xff` |
| NET-4 | faible | Même cause, réseaux `postgresqlcluster_internal` et `llm-net` | Idem |
| HDR-1 | faible | Sur la QA, Traefik remplace HSTS (`preload`), pose `X-Robots-Tag: none` partout et `X-XSS-Protection` | Infrastructure (§6) ; `noindex` voulu en QA |
| TLS-1 | faible | Pas d'enregistrement CAA ; suites CBC-SHA1 offertes en TLS 1.2 | Infrastructure (§6) |
| AUTH-3 | faible\* | Jeton refusé (401) gardé en `sessionStorage` ; compteur d'échecs commun à tout le port | Partie frontend corrigée le 2026-10-19 : toute réponse `401` à une requête admin qui portait le jeton l'efface de `sessionStorage` et redemande le jeton ; un verrouillage (`429`) affiche le message de l'API et le délai (tests `client.test.tsx`, `admin-layout.test.tsx`). Partie compteur liée à NET-1, ouverte |
| SUP-2 (reste) | faible\* | Pas de mise à jour automatique des digests (Renovate) ; le scan Trivy est local, avant chaque mise en production, pas à chaque commit | À faire : Renovate (ou équivalent) sur les `ARG *_DIGEST` ; scan en CI à décider (base de vulnérabilités téléchargée à chaque exécution) |

### 3.3 Constats réfutés, requalifiés ou relevant d'une décision

- **PRIV-4** (journaux de Traefik avec IP complète) : la rétention « illimitée » est réfutée pour
  OpenSearch (fenêtre glissante de 7 jours observée sur tous les services) ; l'IP complète en clair
  pendant ces 7 jours et la rétention du pilote de journaux Docker restent à trancher (ops.md §5.1).
- **HDR-1** : « la production viole la spécification » n'est pas démontré (production non
  déployée) ; sur la QA, `noindex` est souhaitable.
- **INJ-8** (aucun contrôle de contenu de l'analyse avant publication), **AUTHZ-2** (réexamen
  demandé contre un masquage admin), **ESC-1** (suppression d'une idée escaladée), **COST-1**
  (une empreinte peut consommer tout le budget du jour), **ALT-1** (ALTCHA peu coûteux pour un
  script, CGNAT sans IPv6) : comportements conformes à la spécification actuelle ; chaque remède
  change une règle produit ou juridique et relève de l'éditeur (§5).
- **INFRA-8** (jeton en clair dans la configuration statique de Traefik) et **VER-1** (IP vue par
  Traefik, stratégie de `dashboard-allowlist`, chiffrement des overlays et TLS PostgreSQL) : hors
  dépôt, non vérifiables sans accès à l'infrastructure.
- Faux positifs de la campagne ZAP : §4.2.

## 4. Campagne dynamique (2026-10-19)

Pile locale complète : images runtime de l'api, du worker et du frontend (commit de la campagne),
base neuve, modèle de décision réel, faux Mistral en mode hostile, Traefik v3.7.6 qui route comme
la production (sans le WAF ni `global@file`, pour tester l'application seule). Jamais contre la
QA ni la production : seules des vérifications passives d'en-têtes y sont faites (§4.4).

### 4.1 Charges à travers toute la chaîne

`seed.py` publie trois idées ordinaires et une idée « hostile » (marqueur `sondexss`) : le faux
Mistral remplit alors chaque texte libre de l'analyse, les résumés de contributions et les sources
de charges XSS (`<img onerror>`, `</script><script>`, `<svg onload>`, `<iframe src=javascript:>`,
attributs cassés), de formules CSV (`=HYPERLINK(…)`, `+SUM`, `-`, `@`) et d'URL `https://` porteuses
de charges, citées par la recherche web pour passer le contrôle INJ-3. S'y ajoutent une idée dont
le texte du visiteur contient du HTML (acceptée par le modèle de décision, donc publiée), une
contribution, un signalement (nom du signalant `<script>…`) et un droit de réponse admin. Depuis
la décision D3, une seconde idée hostile est signalée en `contenu_problematique` : masquée et
escaladée par la revue, sa copie scellée (titre, résumé, analyse porteurs de charges) est ouverte
par `xss_check.mjs`, avec la liste des copies scellées, après le clic « Afficher la copie
scellée » que la page exige (lecture journalisée).

Résultats :

- 42 contrôles passent (campagne rejouée sur le commit `7d9a6a7`). Les charges sont stockées et
  servies **comme données** (titre, résumé, listes, sources) ; les deux URL `https://evil.example/…` porteuses de charges sont acceptées
  parce que la recherche les a consultées, et une première version du faux Mistral qui citait des
  URL **non** consultées a vu l'analyse échouer (`URL not consulted by the web research`) :
  INJ-3 tient.
- Aller-retour CSV : aucune cellule de l'export ne commence par `=`, `+`, `-`, `@`, tabulation
  ou retour chariot ; `=HYPERLINK(…)` et `+SUM(…)` produits par l'IA sortent en `'=HYPERLINK(…)` et
  `'+SUM(…)`, le JSON les garde tels quels (données, sans interprétation possible).
- XSS stocké (`xss_check.mjs`) : 40 pages dans Chromium et Firefox (page de l'idée hostile, page
  de l'idée au texte HTML, classement, recherche et slug piégés, suivi, méthodologie, toutes les
  pages admin dont signalements, droits de réponse, liste des copies scellées et copie scellée
  avant puis après le clic) : aucune charge exécutée (piège `window.__xss`, dialogues), aucun élément injecté, aucune violation de CSP, JSON-LD valide sans
  `</script` littéral, liens de sources en `https:` avec `target=_blank` et
  `rel="noopener noreferrer nofollow ugc"`, et chaque charge visible **en texte**.

### 4.2 OWASP ZAP

Trois scans, dans le réseau du Traefik local (`https://localhost` comme un navigateur) :

| Scan | Périmètre | URL vues | Alertes de risque faible ou plus |
|---|---|---|---|
| `zap-baseline.py` | frontend, spider + spider AJAX, règles passives | 50 | 1 (10096, faux positif) |
| `zap-api-scan.py` | spec committée (58 opérations, routes admin comprises : toutes en 404 sur l'hôte public), passif + actif | 479 | 2 (100001 et 10096, triées) |
| `zap-api-scan.py -a` ciblé | routes publiques seules, paramètres réels (slug de l'idée hostile, filtres valides), règles alpha | 270 | 2 (100001 et 10096, triées) |

Aucune alerte moyenne ni haute après correction. Le **premier** passage de l'API scan levait
« SQL Injection » (40018, haute, confiance faible), « Format String Error » (30002) et 18 réponses
`500`/`502` : l'analyse a montré que ces `500` venaient de Traefik
(`http: server closed idle connection`) et non de l'api, d'où **ZAP-2** (§3.1) ; ZAP prenait ces
`500` pour des erreurs SQL. Après correction, plus aucune réponse `5xx` sur les 750 URL des
deux scans d'API.

Triage (`devops/dev/security/zap-rules.tsv`) :

| Règle | Verdict | Justification |
|---|---|---|
| 10096 Timestamp Disclosure | faux positif | le sel du défi ALTCHA porte sa date d'expiration par construction (signé, à usage unique) |
| 100001 Unexpected Content-Type | voulu | exports `text/csv` et `text/plain` en pièce jointe avec `nosniff` ; hors `/api`, c'est nginx (coquille SPA, 404 HTML des fichiers cachés, 405 HTML des écritures) |
| 10015, 10049 Cache-Control | voulu | `no-cache` sur `index.html` (nonce CSP neuf à chaque réponse), `no-store` sur le suivi, l'admin et les erreurs |
| 10027 Suspicious Comments | faux positif | mots comme `admin` ou `select` dans le bundle minifié et dans les prompts publiés par `/api/methodology` |
| 10109 Modern Web Application | informatif | application monopage |
| 120000 Browser Storage | informatif | `localStorage` : thème ; `sessionStorage` : positions de défilement de React Router (aucun texte, aucun jeton hors de l'admin) |
| 90005 Sec-Fetch-* absents, 10104, 10094 | informatif | requêtes de ZAP lui-même, ou base64 du défi ALTCHA |

Les rapports HTML complets sont produits à chaque exécution (§7.2) ; ils ne sont pas versionnés.

### 4.3 Sondes manuelles

151 sondes, aucune en échec, deux risques résiduels documentés :

| Famille | Sondes | Résultat |
|---|---|---|
| `headers` | 32 | chaque famille de réponses porte l'ensemble du §14.2 : nginx (`/`, script, `/suivi`, `/admin`, 404 d'asset et de fichier caché, `security.txt`, route profonde, 405) et api (200, 304, 400, 403, 404, 405, 413, 415, 422, ALTCHA, exports, OpenAPI, admin 200/401/404) ; corps d'erreur JSON, `no-store` sur les erreurs, `x-request-id`, aucun `Server`, `Set-Cookie` ni `Access-Control-*` |
| `cors` | 4 | pré-requêtes depuis `https://evil.example` : `405` sans `Access-Control-*` (api publique, admin, nginx) ; aucun `Access-Control-Allow-Origin` sur un GET |
| `csrf` | 25 | `text/plain`, formulaire et multipart : `415` ; `Origin: null`, suffixe, préfixe, schéma `http`, autre port, liste, chemin, `Sec-Fetch-Site: cross-site`/`same-site` : `403 origin_not_allowed` ; mutations admin depuis l'hôte public ou un tiers : `403` |
| `methods` | 8 | `TRACE`, `PUT`, `PATCH`, `DELETE`, `PROPFIND`, `CONNECT` refusés ; `X-HTTP-Method-Override` ignoré |
| `host` | 8 | `Host` inconnu : 404 de Traefik ; `canonical_url`, `security.txt` et `index.html` ne reprennent jamais `Host` ni `X-Forwarded-*` ; `X-Original-URL` / `X-Rewrite-URL` sans effet. Info : une URI absolue vers `admin.localhost` est routée comme son hôte (normal ; en production, `dashboard-allowlist@file` filtre l'adresse source) |
| `traversal` | 9 | `../`, `%2f`, `%2e%2e`, `.git`, `.env`, `.htaccess`, `nginx.conf` : rien ne fuit |
| `limits` | 33 | corps de 70 ko (avec et sans `chunked`) : `413` ; JSON imbriqué sur 20 000 niveaux, UTF-8 invalide, clé dupliquée, champ inconnu : `400`/`422` ; jetons mal formés : `404` ; pagination, tri et filtres hors domaine : `400 invalid_query` ; métacaractères SQL : `200` normal ; slugs piégés : `404` sans écho ; en-tête de 70 ko accepté (borné), de 2 Mio refusé (`431`) |
| `admin` | 26 | 13 variantes de chemin admin (`//`, `%2F`, casse, `./`, `../`, `;`, `%61`) avec un **bon** jeton sur l'hôte public : `404` ; port public avec `Host: admin.localhost` : `404` ; spec publique sans route admin ; port admin sans route publique ; jetons faux ou mal formés : `401` ; jeton en paramètre d'URL ignoré |
| `connection` | 2 | 240 `POST` acceptés et rejetés entrelacés par 6 clients à travers Traefik : aucune `5xx` (ZAP-2) ; corps annoncé jamais envoyé : réponse après 2 s avec `Connection: close` |
| `xff` | 3 | à travers Traefik, un `X-Forwarded-For` tournant n'échappe pas à la limite de suivi (Traefik remplace l'en-tête) ; en direct, une adresse est limitée à la 121ᵉ requête. **Résiduel NET-1** : en direct sur 8080, des adresses tournantes ne sont jamais limitées |
| `reflection` | 1 | une valeur de tri invalide n'est plus reprise dans l'erreur (ZAP-1) |

### 4.4 QA (vérification passive)

Le 2026-10-19, `GET` passifs sur `https://qa.iapolitique.fr` (`/`, `/api/health`,
`/api/ideas?per_page=1`, `/.well-known/security.txt`) : CSP (nginx à nonce et empreintes, api
`default-src 'none'`), `nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer`,
Permissions-Policy, COOP, CORP, COEP (frontend), `Cache-Control` (`no-cache` pour la page,
`no-store` pour la santé, `public, max-age=60` pour la liste) conformes ; aucun `Server`,
`X-Powered-By`, `Set-Cookie` ni `Access-Control-*` ; `security.txt` valide (`Contact`, `Expires` à
180 jours, `Canonical`). Écarts, tous dus à Traefik (HDR-1, §6) : HSTS
`max-age=31536000; includeSubDomains; preload`, `X-Robots-Tag: none` partout (souhaitable en QA),
`X-Xss-Protection: 1; mode=block`.

### 4.5 Scan des images runtime (Trivy, 2026-10-19)

`devops/dev/security/trivy.sh` (Trivy 0.75.0, base de vulnérabilités du jour) sur les images
construites depuis `main` (commit `b5ecaac`) : `docker save` puis scan hors ligne de l'archive,
sévérités `HIGH` et `CRITICAL`, vulnérabilités sans correctif ignorées (`--ignore-unfixed`).

| Image | Système | HIGH / CRITICAL corrigeables | Sans correctif publié (information) |
|---|---|---|---|
| api et worker (`iapolitique-api`) | Debian 13.7 (distroless `cc-debian13`) | 0 | 23 MEDIUM, 8 LOW (paquets de base de Debian, aucun HIGH ni CRITICAL) |
| frontend (`iapolitique-frontend`) | Alpine 3.24.2 (nginx, `apk upgrade`) | 0 | 0 |

Rien à corriger : les images de base sont à leurs derniers digests (IMG-1, même jour) et l'image
nginx prend les correctifs Alpine à chaque build. Le script échoue bien sur une image vulnérable
(vérifié sur une ancienne image de Trivy : 37 et 79 HIGH/CRITICAL corrigeables). Limites : Trivy
ne voit pas les crates du binaire Rust (compilé sans `cargo auditable`) ni les paquets npm du
bundle (pas de `package-lock.json` dans l'image) ; ils sont couverts en CI par `cargo deny` et
`npm audit` (§2, 14.7).

## 5. Risques résiduels

- **Réseaux overlay partagés (NET-1, NET-4)** : un service voisin compromis joint l'api sans
  Traefik, choisit son empreinte de limitation (`X-Forwarded-For`) et peut tenir l'administration
  verrouillée. Mesuré localement : en direct sur 8080, des adresses tournantes échappent à la
  limite de suivi. Correction reportée par l'éditeur (décision D4, 2026-10-19), procédure prête
  (ops.md §5.4).
- **Bots qui résolvent ALTCHA et font tourner leurs IP (ALT-1, COST-1)** : ALTCHA ralentit les
  navigateurs, pas un script ; les limites par empreinte et le plafond journalier bornent le coût,
  mais une campagne distribuée peut retarder les analyses légitimes jusqu'au lendemain.
- **Contenu généré par l'IA (INJ-8)** : publié après contrôles structurels et sémantiques, sans
  modération du fond ; une injection indirecte réussie produirait une analyse fausse ou
  diffamatoire jusqu'à un signalement. Les signalements `contenu_problematique` masquent aussitôt.
- **Texte du visiteur porteur de HTML** : accepté par le modèle de décision s'il est par ailleurs
  recevable ; il est publié comme texte brut, sans risque d'exécution (vérifié dans deux
  navigateurs), mais il reste visible tel quel.
- **Infrastructure partagée (HDR-1, TLS-1, PRIV-4, INFRA-8, VER-1)** : en-têtes réécrits par
  Traefik, absence de CAA, IP complètes 7 jours dans les journaux de Traefik, secrets de la
  configuration de Traefik, chiffrement des overlays non vérifié.
- **Chaîne d'approvisionnement** : digests des images de base mis à jour à la main (pas de
  Renovate) ; scan Trivy des images avant chaque mise en production seulement (§4.5) ; les audits
  npm et RustSec restent bloquants en CI.
- **Décisions juridiques ouvertes** : statut d'hébergeur ou d'éditeur du contenu IA, réexamen
  d'un masquage admin (AUTHZ-2). La conservation d'une idée escaladée (AUTHZ-1 phase 2, ESC-1)
  est tranchée : copie scellée un an, réservée à l'admin, chaque lecture journalisée (décision D3,
  AGENTS.md §7.5), et retrait plutôt qu'effacement d'une idée publiée supprimée par son auteur
  (décision D2).

## 6. Actions pour le propriétaire de l'infrastructure (hors dépôt)

| Priorité | Action | Constat | Détail |
|---|---|---|---|
| 1 | Créer un réseau overlay chiffré dédié à Traefik, l'api et le frontend (`iapolitique_edge`, `qa-iapolitique_edge`), l'ajouter au service Traefik de la stack `privatenetwork` ; puis, dans le dépôt, y passer api et frontend et retirer `proxy`. **Reporté** (décision D4 du 2026-10-19) | NET-1, NET-4 | ops.md §5.4 (procédure pas à pas) |
| 2 | Chaîne de middlewares sans `X-Robots-Tag` ni `preload` ni `X-XSS-Protection` pour les routeurs publics de production (`security-headers-indexable`, `global-indexable`), QA gardée en `noindex` ; ne pas soumettre le domaine à hstspreload.org sans décision de l'éditeur | HDR-1 | ops.md §5.2 |
| 3 | Zone DNS : `CAA 0 issue "letsencrypt.org"`, `CAA 0 issuewild ";"`, `CAA 0 iodef "mailto:<contact sécurité>"` ; Traefik : `tls.options.default` limité aux suites ECDHE-GCM/CHACHA20, `sniStrict: true` ; vérifier que `iapolitique.fr` et `www` obtiennent un certificat Let's Encrypt avant l'ouverture | TLS-1 | — |
| 4 | Journaux d'accès de Traefik : IP tronquée ou retirée pour les hôtes iapolitique, ou rétention ≤ 7 jours garantie aussi sur l'hôte Docker (`max-size`/`max-file`) | PRIV-4 | ops.md §5.1 |
| 5 | Sortir le jeton en clair de la configuration statique de Traefik (secret Swarm, rotation) | INFRA-8 | — |
| 6 | Vérifier et noter : mode de publication des ports de Traefik, `ipStrategy` de `dashboard-allowlist@file`, chiffrement des overlays, TLS entre l'api et PostgreSQL (`sslmode`) | VER-1 | — |
| 7 | Sauvegardes chiffrées de la base, 30 jours | §7.3 | ops.md §5.3 |

## 7. Rejouer les contrôles

### 7.1 À chaque commit (CI PulsarCD)

Identique à l'étape de test de PulsarCD, depuis `devops/`, avec les variables d'un `devops/.env`
(copie de `.env.example`), que PulsarCD charge de la même façon :

```bash
docker compose -p iapolitique-ci -f docker-compose.swarm.yml build test
docker compose -p iapolitique-ci -f docker-compose.swarm.yml run --rm test
docker compose -p iapolitique-ci -f docker-compose.swarm.yml down -v
```

Couvre : `npm audit`, lint (règles XSS), Vitest (CSP, XSS, chaîne d'approvisionnement), contrôle
des origines du build, en-têtes de nginx, invariants du compose et épinglage des images,
`cargo fmt`/`clippy`, `cargo deny`, puis tous les tests Rust contre PostgreSQL (en-têtes, CSRF,
CORS, admin, injections SQL, champs inconnus et trop longs, journaux).

### 7.2 Avant chaque mise en production (pile locale, AGENTS.md §14.8)

Prérequis : Docker, Python 3, le PostgreSQL de test (`docker start iapolitique-test-pg`), le
modèle de décision joignable, et les images runtime construites depuis `devops/` :

```bash
docker compose -f docker-compose.swarm.yml --env-file <copie de .env.example> build api frontend
devops/dev/e2e.sh                     # parcours fonctionnel complet + navigateurs + journaux
devops/dev/security/run.sh            # campagne de sécurité (15 à 20 minutes)
```

`e2e.sh` couvre aussi les décisions du 2026-10-19 : retrait par son auteur d'une idée publiée
portant la contribution d'un autre visiteur (D2 : 404, absente de la liste et des exports, suivi du
contributeur « retirée par son auteur », idée visible de l'admin), suppression d'une idée escaladée
dont la copie scellée reste lisible par l'admin avec lecture journalisée (D3), et clé Mistral
refusée (401) puis rétablie : jobs en attente sans tentative consommée, un seul appel de contrôle
par période, une seule ERROR journalisée, puis publication complète.

`run.sh` démarre la pile (`E2E_STACK_ONLY=1 devops/dev/e2e.sh`) et un Traefik local
(`devops/dev/traefik-dynamic.yml`), puis enchaîne :

| Script | Rôle |
|---|---|
| `seed.py` | idées publiées, charges hostiles à travers toute la chaîne, idée hostile escaladée (copie scellée), aller-retour CSV |
| `probes.py` | sondes manuelles (en-têtes de chaque famille de réponses, CORS, CSRF, `Origin` et `Host`, corps et en-têtes démesurés, isolement de l'admin, réutilisation des connexions, `X-Forwarded-For`) |
| `xss_check.mjs` | XSS stocké dans Chromium et Firefox, pages publiques et admin (copies scellées comprises : liste, puis copie affichée après le clic) : piège sur `window.__xss`, dialogues, violations de CSP, éléments injectés, JSON-LD, liens de sources |
| `zap.sh` | ZAP baseline (frontend), API scan (spec committée), scan actif ciblé (routes publiques, paramètres réels, règles alpha) ; triage par `zap-rules.tsv`, synthèse par `zap_summary.py` |
| `trivy.sh` | vulnérabilités `HIGH`/`CRITICAL` corrigeables des images runtime de l'api et du frontend (Trivy épinglé, archive `docker save` scannée hors ligne, base gardée dans le volume `iapolitique_trivy_cache`) ; exceptions dans `trivy-ignore.yaml` (raison et date d'expiration) ; lançable seul : `devops/dev/security/trivy.sh [répertoire]` |

Le script échoue sur toute sonde en échec, tout XSS, toute alerte ZAP de risque faible ou plus
qui n'est pas triée dans `zap-rules.tsv` (chaque exception y porte sa justification) et toute
vulnérabilité `HIGH`/`CRITICAL` corrigeable d'une image runtime. Rapports :
répertoire `SEC_WORK` (par défaut un dossier temporaire affiché à la fin) : `probes.json`,
`xss.log`, `zap/*.html`, `zap/zap-summary.md`, `trivy/*.json` et `trivy/*.txt`. Variables :
`SEC_WORK`, `SEC_KEEP=1` (garder la pile), `SEC_REUSE_STACK=1` (réutiliser une pile et un
`seed.json` existants), `SEC_SKIP_ZAP=1`, `SEC_SKIP_TRIVY=1`, `SEC_API_IMAGE`, `SEC_FRONT_IMAGE`.

**Jamais contre la QA ni la production** : les sondes épuisent les limites de débit et les scans
actifs envoient des milliers de requêtes d'attaque. Sur la QA, seules des requêtes `GET`/`HEAD`
passives sont admises, par exemple :

```bash
for path in / /api/health "/api/ideas?per_page=1" /.well-known/security.txt; do
  curl -sSI "https://qa.iapolitique.fr$path"
done
```
