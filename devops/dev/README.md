# Bout en bout local (jamais en production)

Outils pour vérifier la pile complète avec les **images runtime** avant une mise en QA :

| Fichier | Rôle |
|---|---|
| `mock_mistral.py` | Faux serveur Mistral (bibliothèque standard Python) : `POST /v1/chat/completions` (sorties structurées `analysis`, `second_opinion`, `report_review`, valides contre les schémas de `prompts/`), `POST /v1/conversations` (recherche web : mémo sourcé `[S1]`, `[S2]`), `POST /v1/embeddings` (vecteurs déterministes de 1 024 dimensions : deux textes presque identiques ont une similarité proche de 1). Réponses déterministes : second avis `reject` sur une insulte, revue `hide` + `escalate` pour « Contenu problématique », `update` pour « Erreur de chiffrage ». Avec `reasoning_effort: high` (défaut de l'analyse, §6.1), le contenu devient une liste de fragments comme chez Mistral : un fragment `thinking` (texte `PENSEE-PRIVEE-MOCK`, que le scénario vérifie absent des réponses de l'API et des journaux) puis les fragments `text` ; un effort hors de l'énumération de Mistral répond 400 ; `GET /stats` compte aussi les appels par effort envoyé, et sous `refused_auth` les appels refusés par l'interrupteur de clé : `POST /control {"auth_status": 401}` (ou 403) fait répondre ce statut à tout appel `/v1`, comme Mistral avec une clé fausse ou révoquée, `{"auth_status": null}` accepte de nouveau la clé ; `POST /control {"reject_reasoning_effort": ["mistral-large-latest"]}` (ou `--reject-reasoning-effort MODÈLE`, répétable) fait refuser `reasoning_effort` par ces modèles (HTTP 400 au format d'erreur de Mistral, `param: "reasoning_effort"`, comptés sous `refused_reasoning_effort`), la même requête sans le paramètre étant servie, `[]` lève le refus. `GET /v1/models` liste quelques modèles sans rien facturer (vérification de la clé par l'admin), sous le même interrupteur de clé. Aucun prompt n'est journalisé. |
| `e2e_scenario.py` | Parcours complet par l'API publique et l'API admin (ALTCHA résolu par le script, jetons uniquement dans des corps `POST`) : idée acceptée → dédoublonnage → analyse → publiée ; idée de près de 4 000 caractères réglée (jamais bloquée en `submitted`) ; idée insultante refusée avec exposé des motifs ; quasi-doublon fusionné ; contribution intégrée par ré-analyse ; retrait par son auteur d'une idée publiée portant la contribution d'un autre visiteur (D2 : 404 public, absente de la liste, des exports et du formulaire de contribution, suivi du contributeur « retirée par son auteur », idée visible de l'admin en `withdrawn`) ; réexamen humain ; exports (JSON, CSV, `LICENSE`, métadonnées) ; signalement `contenu_problematique` → masquage + escalade, puis suppression de l'idée escaladée par son auteur : sa copie scellée reste lisible par l'admin, lecture journalisée (D3) ; routes admin. Phases `auth-refused` puis `auth-restored` : le faux Mistral refuse la clé (401), l'alerte apparaît au tableau de bord, les jobs Mistral attendent (15 min, sans consommer de tentative, un seul appel de contrôle par période), puis la vérification immédiate de la clé par l'admin (`POST /api/admin/provider-alerts/mistral/check`) répond « toujours refusée » et est journalisée, puis la clé est de nouveau acceptée et, une fois le délai passé, les idées sont publiées, l'alerte close et chaque job n'a consommé qu'une tentative. Phase `reasoning-refused` : le faux Mistral refuse `reasoning_effort` pour le modèle configuré ; une nouvelle idée est publiée quand même (chaque processus envoie le paramètre une fois, puis s'en passe), le tableau de bord affiche l'alerte `mistral_reasoning_unsupported` et la méthodologie l'effort réellement envoyé (aucun). Phase `budget` : plafond atteint, le job Mistral attend minuit (Europe/Paris) sans consommer de tentative. |
| `e2e.sh` | Orchestration : base neuve, faux Mistral, api, worker et frontend en conteneurs, les phases du scénario (entre `auth-refused` et `auth-restored`, avance l'horloge de l'alerte et des jobs en attente au lieu d'attendre 15 minutes), puis contrôle des journaux (aucune IP, aucun jeton, aucun secret, aucun texte de visiteur, aucune erreur hormis l'unique ERROR « Mistral refused the API key » attendue, un seul WARN « rejects reasoning_effort » du worker). Le faux Mistral est publié sur `127.0.0.1:18090` (`E2E_MOCK_PORT`). |
| `traefik-dynamic.yml` | Routage Traefik local identique aux étiquettes du compose (`/api/admin` vers le port admin sur `admin.localhost`, le reste de `/api` vers l'api, le reste vers nginx), partagé par `browser_check.sh` et `security/run.sh`. |
| `security/` | Campagne de sécurité locale (`security/run.sh`) : sondes manuelles, XSS stocké en navigateur, OWASP ZAP, scan Trivy des images runtime (`security/trivy.sh`, aussi lançable seul) ; voir `docs/securite.md` §7. Le faux Mistral y passe en **mode hostile** quand l'idée, le texte ou le signalement contient `sondexss` : charges XSS, formules CSV et URL piégées dans chaque texte libre de ses réponses. |
| `browser_check.sh`, `browser_check.mjs` | Contrôle dans de vrais navigateurs (Chromium et Firefox, image Playwright) derrière un Traefik HTTPS qui route comme la production (`/api` vers l'api, `/api/admin` vers le port admin sur `admin.localhost`, le reste vers nginx) : toutes les routes publiques et admin, dépôt d'une idée par le formulaire (ALTCHA résolu par son worker), page de suivi et dialogue, menu mobile, listes Radix. Échoue sur toute violation de CSP (documents et workers), erreur de page, requête tierce, en-tête manquant, page non isolée (`crossOriginIsolated`) ou `<style>` bloqué ; vérifie que le jeton admin n'atteint pas `localStorage`. Lancé par `e2e.sh` après le scénario principal (`E2E_BROWSER=0` pour l'omettre) ou à la main après `E2E_KEEP=1 devops/dev/e2e.sh`. |

## Lancer

Prérequis : Docker, Python 3, le PostgreSQL de test local
(`docker start iapolitique-test-pg`, AGENTS.md §13.5) et le modèle de décision joignable.

```bash
# Images runtime, depuis devops/ (copie de .env.example comme fichier d'environnement)
cp .env.example /tmp/e2e.env
docker compose -f docker-compose.swarm.yml --env-file /tmp/e2e.env build api frontend

# Parcours complet (8 à 10 minutes : les exports sont régénérés 2 min après une publication)
devops/dev/e2e.sh
```

Variables facultatives : `E2E_PG_URL` (serveur PostgreSQL vu depuis les conteneurs, défaut
`postgres://postgres:postgres@host.docker.internal:55432`), `E2E_DB` (défaut
`iapolitique_e2e`, recréée à chaque lancement ; un nom contenant `prod` ou `qa` est refusé),
`E2E_DECISION_SERVICE_URL` (défaut `http://192.168.1.27:8001/v1/systemone`),
`E2E_DECISION_TIMEOUT_SECS` / `E2E_DECISION_JOB_TIMEOUT_SECS` (défauts de production : 5 et
30), `E2E_PUBLIC_PORT` / `E2E_ADMIN_PORT` / `E2E_FRONT_PORT` (18080, 18081, 18000),
`E2E_KEEP=1` pour garder les conteneurs après la fin, `E2E_STACK_ONLY=1` pour démarrer et
contrôler la pile sans aucun scénario (elle reste lancée, comme avec `E2E_KEEP=1`).

Le faux Mistral sert aussi au développement natif (`backend/dev/README.md`) :
`python devops/dev/mock_mistral.py --port 8090`, puis `MISTRAL_API_BASE_URL=http://127.0.0.1:8090`
et une `MISTRAL_API_KEY` quelconque non vide.

En-têtes de nginx : `e2e.sh` lance aussi `devops/check_frontend_headers.sh` sur le conteneur
frontend (même contrôle qu'en CI, AGENTS.md §13.5).
