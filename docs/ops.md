# Exploitation

Ce document décrit ce qui se fait **une fois**, à la main, sur le cluster, et les opérations
courantes (secrets, logs, incidents). Le reste du cycle de vie (build, tests, QA, production)
passe uniquement par PulsarCD ([AGENTS.md §10](../AGENTS.md#10-déploiement-pulsarcd--docker-swarm)).

| | Production | QA |
|---|---|---|
| Stack PulsarCD | `iapolitique` | `qa-iapolitique` |
| Site public | `https://iapolitique.fr` (`www.` redirige en 301) | `https://qa.iapolitique.fr` |
| Administration (LAN) | `https://admin.iapolitique.fr` | `https://qa.admin.iapolitique.fr` |
| Base | `iapolitique_prod`, utilisateur `iapolitique_prod_app` | `iapolitique_qa`, utilisateur `iapolitique_qa_app` |
| Réseau LLM | `llm-net` | `qa-llm-net` (renommage automatique par PulsarCD) |
| Variables | `devops/.env` | `devops/.env` puis `devops/.env.qa` par-dessus |

Pipeline : chaque commit poussé sur `main` est construit, testé puis, si les tests passent,
déployé automatiquement en QA (porte `test_to_deploy` en `auto_with_success`, QA activée). Le
passage QA → production est décidé par un humain.

## 1. Mise en place initiale

### 1.1 DNS

Enregistrements A vers l'IP publique du cluster (82.64.64.144), déjà créés : `iapolitique.fr`,
`www.iapolitique.fr`, `admin.iapolitique.fr`, `qa.iapolitique.fr`, `qa.admin.iapolitique.fr`.
Traefik obtient les certificats Let's Encrypt par challenge HTTP-01.

La stack QA déclare aussi `www.qa.iapolitique.fr` (même fichier compose, préfixe `qa.` ajouté
par PulsarCD). Sans enregistrement DNS pour ce nom, seul ce routeur échoue (erreurs ACME dans
les logs de Traefik) ; créer l'enregistrement A les fait disparaître.

### 1.2 Bases PostgreSQL

Sur un manager du swarm, depuis le dépôt `PostgreSqlCluster` (son `devops/.env` contient les
identifiants administrateur), créer chaque base avec son utilisateur. Le rôle `owner` est
nécessaire : l'api applique les migrations au démarrage (création de tables, d'index et
d'extensions de confiance).

```bash
cd PostgreSqlCluster
./devops/create-app-user.sh iapolitique_prod iapolitique_prod_app '<mot de passe prod>' owner
./devops/create-app-user.sh iapolitique_qa   iapolitique_qa_app   '<mot de passe qa>'   owner
```

Générer les mots de passe avec `openssl rand -hex 24` (pas de caractère à échapper dans une URL).

L'extension `vector` (pgvector) n'est **pas** une extension de confiance : seul un superutilisateur
peut la créer. Une fois par base :

```bash
for db in iapolitique_prod iapolitique_qa; do
  docker run --rm --network postgresqlcluster_internal -e PGPASSWORD="$PG_ADMIN_PASSWORD" \
    timescale/timescaledb:2.25.2-pg18 \
    psql -v ON_ERROR_STOP=1 -h pg-primary -U postgres -d "$db" \
    -c 'CREATE EXTENSION IF NOT EXISTS vector;' \
    -c 'CREATE EXTENSION IF NOT EXISTS pg_trgm;' \
    -c 'CREATE EXTENSION IF NOT EXISTS unaccent;' \
    -c 'CREATE EXTENSION IF NOT EXISTS pgcrypto;'
done
```

`pg_trgm`, `unaccent` et `pgcrypto` sont de confiance (le propriétaire de la base pourrait les
créer), mais les créer ici évite toute surprise lors de la première migration. Vérification :
`\dx` dans `psql` doit lister `vector` en version 0.8.x.

### 1.3 Variables de la stack

Dans PulsarCD, éditeur `.env` de la stack `iapolitique` (ou MCP `set_stack_env`), partir de
[`devops/.env.example`](../devops/.env.example) et renseigner au minimum :

| Clé | Valeur |
|---|---|
| `DATABASE_CONNECTION_STRING` | `postgres://iapolitique_prod_app:<mdp>@pg-primary:5432/iapolitique_prod?application_name=iapolitique` |
| `MISTRAL_API_KEY` | clé du compte Mistral (voir le mémo juridique : refus de l'« Anonymous improvement data ») |
| `ALTCHA_HMAC_KEY` | `openssl rand -hex 32` |
| `ADMIN_API_TOKEN` | `openssl rand -hex 32` |
| `TRAEFIK_HOST`, `ADMIN_HOST` | `iapolitique.fr`, `admin.iapolitique.fr` |
| `DEPLOY_HEALTHCHECK_PATH` | `/api/health` |
| `LEGAL_PUBLISHER_NAME`, `LEGAL_PUBLISHER_ADDRESS`, `LEGAL_PUBLISHER_PHONE`, `LEGAL_CONTACT_EMAIL`, `LEGAL_PRIVACY_EMAIL` | identité et adresses de l'éditeur (AGENTS.md §7.5), publiées sur les pages légales ; jamais dans git. Vides : « coordonnées momentanément indisponibles » et avertissement au démarrage de l'api ; une adresse électronique invalide empêche le démarrage |

Onglet `.env.qa` (visible quand la QA est activée), **indispensable** : sans lui, la QA utilise
la base et le budget de production.

```dotenv
DATABASE_CONNECTION_STRING=postgres://iapolitique_qa_app:<mdp>@pg-primary:5432/iapolitique_qa?application_name=iapolitique-qa
MISTRAL_DAILY_BUDGET_EUR=0.5
ALTCHA_HMAC_KEY=<clé propre à la QA>
ADMIN_API_TOKEN=<jeton propre à la QA>
```

Ne mettre dans `.env.qa` ni `TRAEFIK_HOST`/`ADMIN_HOST` (PulsarCD ajoute déjà `qa.`), ni
`LLM_NETWORK` (PulsarCD renomme déjà le réseau externe `llm-net` en `qa-llm-net` ; écrire
`qa-llm-net` produirait `qa-qa-llm-net`).

Les clés se terminant par `_CONNECTION_STRING`, `_KEY` ou `_TOKEN` deviennent des secrets Docker
(`iapolitique_<CLÉ>` et `qa-iapolitique_<CLÉ>`), montés dans `/run/secrets/` et rechargés dans
l'environnement par le backend au démarrage.

### 1.4 Prérequis du cluster

Déjà en place, à vérifier en cas de nouveau cluster : réseaux overlay externes `proxy`,
`postgresqlcluster_internal`, `llm-net` et `qa-llm-net` ; service `llamacpp-decision-service`
attaché à `llm-net` et `qa-llm-net` ; Traefik publiant 80/443 en `mode: host` (sinon l'api voit
une IP interne et la limitation de débit par empreinte ne distingue plus les visiteurs) ; nœuds
étiquetés `gpu=none` pour accueillir la stack.

## 2. Rotation des secrets

Un secret Docker est immuable et PulsarCD réutilise tel quel un secret existant : modifier la
valeur dans `.env` ne suffit pas.

1. Modifier la valeur dans `.env` (ou `.env.qa`) via PulsarCD.
2. Sur un manager, arrêter les services qui utilisent le secret, le leur retirer, puis le
   supprimer. Exemple pour `ADMIN_API_TOKEN` (utilisé par l'api seule) :
   ```bash
   docker service scale iapolitique_api=0
   docker service update --detach --secret-rm iapolitique_ADMIN_API_TOKEN iapolitique_api
   docker secret rm iapolitique_ADMIN_API_TOKEN
   ```
   Passer d'abord à 0 réplique évite qu'une tâche démarre sans la variable, échoue et déclenche
   le rollback automatique (qui remonterait l'ancien secret). `DATABASE_CONNECTION_STRING` et
   `MISTRAL_API_KEY` sont aussi montés dans `iapolitique_worker` : traiter les deux services.
   Le site renvoie des erreurs 502 sur `/api` jusqu'à l'étape 3.
3. Redéployer la même version depuis PulsarCD (`deploy_stack` avec la version en production) :
   le secret est recréé avec la nouvelle valeur et les services retrouvent leur réplique.

Pour la QA, même procédure avec le préfixe `qa-iapolitique_` (services `qa-iapolitique_api`,
`qa-iapolitique_worker`). Après une rotation de mot de passe PostgreSQL, changer d'abord le mot
de passe (`ALTER ROLE iapolitique_prod_app PASSWORD '…'`), puis suivre la procédure ci-dessus pour
`DATABASE_CONNECTION_STRING`.

Effets d'une rotation : `ADMIN_API_TOKEN` doit être ressaisi dans l'administration ;
`ALTCHA_HMAC_KEY` invalide les défis en cours (quelques minutes).

## 3. Logs et diagnostic

Les logs du backend sont en JSON (`tracing`), en anglais, sans IP, jeton, en-tête
d'identification ni texte d'idée (AGENTS.md §7.1). Le niveau se règle avec `RUST_LOG` puis un
redéploiement.

**Via PulsarCD** (MCP `pulsarcd-read`, recommandé) :

- `search_logs(github_project="iapolitique", last_hours=24)` : logs de la stack ;
  filtrer sur le service (`iapolitique_api`, `iapolitique_worker`, `iapolitique_frontend`) ou
  le niveau `ERROR` ;
- `get_health_summary()` : erreurs par service après un déploiement ;
- `get_service_tasks("iapolitique_api")` : état des tâches Swarm (redémarrages, échecs de
  healthcheck) ;
- `get_pipeline_status("iapolitique")`, `get_pipeline_history()` puis `get_pipeline_run(id)` :
  sortie des étapes build, test, QA et déploiement (dont les résultats de `cargo test`).

**Sur un manager** :

```bash
docker service ps iapolitique_api --no-trunc          # tâches, motif d'échec
docker service logs -f --since 30m iapolitique_api    # logs en direct
docker service logs --since 1h iapolitique_worker | grep '"level":"ERROR"'
```

Symptômes fréquents :

| Symptôme | Piste |
|---|---|
| L'api redémarre en boucle, log `invalid configuration` | Variable manquante ou invalide, listée dans le message (secret vide, clé < 32 caractères) |
| Déploiement annulé par rollback | Healthcheck en échec : `docker service ps` puis logs de la tâche refusée ; migration en erreur |
| Idées bloquées en « en cours de vérification » | Modèle de décision injoignable (`llm-net`) ou budget Mistral du jour atteint |
| Analyses en attente | `MISTRAL_API_KEY` vide, budget atteint, ou worker arrêté |
| 403 sur `admin.` | Accès hors LAN : filtré par `dashboard-allowlist@file` |

## 4. Tests de CI en local

Mêmes contrôles que l'étape de test de PulsarCD, depuis `devops/` avec un `.env` (copie de
`.env.example`) :

```bash
docker compose -f docker-compose.swarm.yml build test     # lint, typecheck, Vitest, build, fmt, clippy
docker compose -f docker-compose.swarm.yml run --rm test  # cargo test contre test-db
docker compose -f docker-compose.swarm.yml down
```

`test-db` est un service global contraint sur une étiquette qu'aucun nœud ne porte : le swarm
ne le démarre jamais, alors que `docker compose run` le lance comme dépendance de `test`.

Avant une mise en QA d'un changement du pipeline, le parcours de bout en bout des images
runtime (api, worker, frontend, base neuve, modèle de décision réel, faux Mistral) se lance avec
`devops/dev/e2e.sh` : voir [`devops/dev/README.md`](../devops/dev/README.md).

## 5. Infrastructure partagée : exigences de sécurité et de conformité

Traefik, ses middlewares (`global@file`, `security-headers@file`, `dashboard-allowlist@file`) et
le cluster PostgreSQL sont gérés hors de ce dépôt (stacks `privatenetwork` et
`postgresqlcluster`). Constat du 2026-10-19 (Traefik v3.7.6, lecture seule de
`privatenetwork_traefik_config` et `privatenetwork_dynamic_config`) et réglages attendus.

### 5.1 Journaux d'accès de Traefik (AGENTS.md §7.5)

Aujourd'hui, `accessLog` est en JSON sur la sortie standard, **avec l'IP complète du client**
(`ClientAddr`, `ClientHost`) et le `User-Agent` (`headers.names.User-Agent: keep`), pour tous les
hôtes ; ces lignes sont collectées par PulsarCD (vue Sécurité : requêtes par IP, bannissements).
Or le site s'engage à ne conserver aucune IP en clair au-delà de 7 jours. Traefik ne sait pas
tronquer une IP : il faut choisir l'une des options suivantes (décision de l'éditeur, à
reporter dans `/confidentialite` si elle change ce qui y est écrit).

**Option A — rétention de 7 jours au plus** (garde la vue Sécurité de PulsarCD) : configurer
l'index des journaux de Traefik dans PulsarCD/OpenSearch pour supprimer les entrées de plus de
7 jours (politique de cycle de vie), et borner les journaux Docker du conteneur Traefik, par
exemple dans le service `traefik` de la stack `privatenetwork` :

```yaml
    logging:
      driver: json-file
      options:
        max-size: "50m"
        max-file: "3"
```

(La rotation de `json-file` est fondée sur la taille : vérifier qu'elle couvre moins de 7 jours
de trafic, sinon la durée de rétention d'OpenSearch fait foi.)

**Option B — journaux sans IP ni User-Agent** (configuration statique `traefik.yml`, pour tous
les hôtes du cluster ; la vue Sécurité de PulsarCD ne voit plus les IP) :

```yaml
accessLog:
  format: json
  fields:
    defaultMode: keep
    names:
      ClientAddr: drop
      ClientHost: drop
      ClientPort: drop
      ClientUsername: drop
    headers:
      defaultMode: drop
      names:
        User-Agent: drop
```

(`redact` à la place de `drop` remplace la valeur par `REDACTED` au lieu de retirer le champ.)

**Option C — aucun journal d'accès Traefik pour iapolitique seulement** : depuis Traefik v3.1,
l'étiquette de routeur `traefik.http.routers.<routeur>.observability.accessLogs=false` coupe le
journal d'accès d'un routeur. Ajoutée aux routeurs `iapolitique-*` de
`devops/docker-compose.swarm.yml`, elle laisse les autres sites inchangés ; le WAF et le rate
limit de `global@file` s'appliquent toujours, mais PulsarCD ne voit plus les requêtes du site.
Non activée : à décider.

Le journal d'accès de nginx (frontend) ne contient ni IP, ni User-Agent, ni chaîne de requête,
ni jeton (`frontend/nginx.conf`) ; le backend ne journalise jamais d'IP (AGENTS.md §7.1).

### 5.2 En-têtes posés par Traefik

Le middleware `security-headers` (dans `global@file` et sur les routeurs admin) remplace les
en-têtes du même nom envoyés par nginx et par l'api :

| En-tête | Envoyé par Traefik | Attendu (AGENTS.md §14.2) |
|---|---|---|
| `Strict-Transport-Security` | `max-age=31536000; includeSubDomains; preload` (`stsSeconds`, `stsIncludeSubdomains`, `stsPreload`) | `max-age=63072000; includeSubDomains` |
| `X-Robots-Tag` | `none` (= `noindex, nofollow`) sur **toutes** les réponses | seulement l'admin, `/suivi` et l'API |
| `X-XSS-Protection` | `1; mode=block` (`browserXssFilter`) | absent ou `0` (filtre retiré des navigateurs, la CSP le remplace) |

Conséquences et recommandations :

- **`preload`** : la directive n'a d'effet que si le domaine est soumis à hstspreload.org ; une
  fois inscrit dans les navigateurs, `iapolitique.fr` **et tous ses sous-domaines** (dont
  `qa.` et `admin.`) ne seront plus jamais joignables en HTTP, et un retrait prend des mois.
  Recommandation : ne pas soumettre le domaine et retirer `stsPreload` tant que l'éditeur n'a pas
  décidé explicitement du préchargement. Pour aligner la durée : `stsSeconds: 63072000`.
- **`X-Robots-Tag: none` sur tout le site** : en production, les moteurs de recherche
  n'indexeraient aucune page publique (classement, idées). Si ce n'est pas voulu, retirer
  `X-Robots-Tag` de `customResponseHeaders` (nginx et l'api posent déjà `noindex, nofollow` là où
  il le faut), ou créer pour iapolitique une chaîne sans cet en-tête, par exemple dans
  `dynamic.yml` :

  ```yaml
  http:
    middlewares:
      security-headers-indexable:
        headers:
          frameDeny: true
          contentTypeNosniff: true
          referrerPolicy: "no-referrer"
          stsSeconds: 63072000
          stsIncludeSubdomains: true
          customResponseHeaders:
            Server: ""
      global-indexable:
        chain:
          middlewares:
            - waf
            - security-headers-indexable
            - rate-limit
  ```

  puis remplacer `global@file` par `global-indexable@file` dans le compose (et dans
  `devops/check_compose_security.py`, qui exige aujourd'hui `global@file`).
- `X-XSS-Protection` : retirer `browserXssFilter` (ou poser `X-XSS-Protection: 0`).

Vérification (lecture seule) : `curl -sSI https://qa.iapolitique.fr/suivi`.

### 5.3 Sauvegardes de la base (rétention 30 jours)

Le cluster PostgreSQL a deux réplicas (`pg-standby1`, `pg-standby2`), qui **ne sont pas** des
sauvegardes (une suppression est répliquée), et aucune sauvegarde automatique n'est en place
(`PostgreSqlCluster/MANAGE.md` ne décrit que des commandes manuelles). Le mémo juridique fixe la
rétention des sauvegardes à **30 jours** : passé ce délai, une sauvegarde contiendrait encore des
textes refusés, supprimés par leur auteur ou purgés.

Mise en place attendue, une sauvegarde par jour et par base, conservée 30 jours au plus, sur un
manager (cron) :

```bash
#!/bin/sh
# /usr/local/bin/backup-iapolitique.sh (cron : 30 3 * * *), PG_ADMIN_PASSWORD dans l'environnement
set -eu
dir=/srv/backups/iapolitique
mkdir -p "$dir" && chmod 700 "$dir"
for db in iapolitique_prod iapolitique_qa; do
  docker run --rm --network postgresqlcluster_internal -e PGPASSWORD="$PG_ADMIN_PASSWORD" \
    timescale/timescaledb:2.25.2-pg18 \
    pg_dump -h pg-primary -U postgres -Fc "$db" >"$dir/$db-$(date +%Y%m%d).dump"
done
find "$dir" -name 'iapolitique_*.dump' -mtime +29 -delete
```

- Les sauvegardes ne contiennent ni sel de limitation de débit ni compteurs (en mémoire
  seulement, AGENTS.md §7.2) ni IP ; elles contiennent les textes et les coordonnées des
  signalants : stockage chiffré ou sur un disque réservé, accès root seulement, jamais hors de
  l'infrastructure de l'éditeur.
- Après une restauration, laisser le worker repasser les purges (refus à 30 jours, pierres
  tombales, contenus masqués) avant de rouvrir le site : la base restaurée peut contenir des
  données dont l'échéance est passée.
- Restauration : `pg_restore -h pg-primary -U postgres -d <base> --clean <fichier>.dump`
  (voir `PostgreSqlCluster/MANAGE.md`).

### 5.4 Réseaux overlay partagés : exposition directe des ports 8080 et 8081

Constat de la revue de sécurité (NET-1, NET-4, 2026-10-19) : l'api rejoint trois réseaux overlay
partagés, `proxy` (Traefik et une cinquantaine de services d'autres stacks, `attachable`, non
`internal`), `postgresqlcluster_internal` et `llm-net`, et écoute sur `0.0.0.0` pour ses ports
8080 (public) et 8081 (admin). « Le port 8081 n'est jamais routé depuis Internet » reste vrai pour
**Traefik** : mais tout conteneur de ces réseaux joint directement `iapolitique_api:8080` et
`:8081`, sans le WAF, sans le rate limit de Traefik et sans `dashboard-allowlist@file`. Un
attaquant qui compromet un service voisin pourrait :

- choisir son empreinte de limitation de débit en posant lui-même `X-Forwarded-For` (l'api prend
  l'entrée la plus à droite, quel que soit le pair TCP : `TRUSTED_PROXY_HOPS=1`) ; ALTCHA et le
  budget Mistral restent les seules limites des dépôts ;
- envoyer dix jetons faux toutes les 5 minutes sur 8081 et tenir l'administration verrouillée
  (`429 admin_locked`, AGENTS.md §14.4), ce qui bloque la modération et les retraits.

Le jeton admin lui-même (≥ 32 caractères aléatoires) reste hors de portée d'une recherche
exhaustive.

**Correction attendue côté infrastructure** (propriétaire de la stack `privatenetwork`) : un
réseau dédié que seuls Traefik, l'api et le frontend partagent.

```bash
# Sur un manager. Le second réseau sert à la QA si PulsarCD y préfixe ce réseau externe
# comme il le fait pour llm-net (qa-llm-net) : à vérifier au premier déploiement QA.
docker network create --driver overlay --opt encrypted iapolitique_edge
docker network create --driver overlay --opt encrypted qa-iapolitique_edge
```

puis ajouter ces réseaux au service `traefik` dans le compose de la stack `privatenetwork`
(pas de `docker service update` manuel : il serait perdu au prochain déploiement). Ensuite,
dans `devops/docker-compose.swarm.yml` : remplacer `proxy` par `iapolitique_edge` pour api et
frontend, poser `traefik.swarm.network=iapolitique_edge`, et faire exiger ce réseau par
`devops/check_compose_security.py` (api et frontend hors de `proxy`). Vérifier sur la QA avant la
production que PulsarCD réécrit aussi le nom du réseau dans l'étiquette `traefik.swarm.network`.

**Complément dans l'application** (en attente côté backend) : ne croire `X-Forwarded-For` que si
le pair TCP est dans une plage de confiance (`TRUSTED_PROXY_CIDRS`, sous-réseau du réseau de
Traefik, valeur prise de `docker network inspect`), ou exiger un en-tête secret posé par Traefik
(middleware défini dans le provider de fichiers de Traefik, pas en étiquette) avant toute
vérification de jeton sur 8081, pour qu'un client hors Traefik ne puisse ni choisir son empreinte
ni faire compter des échecs d'authentification.

Tant que ni l'un ni l'autre n'est en place, l'exposition résiduelle est acceptée et notée dans
AGENTS.md §14.4 et §14.7 : 8080 et 8081 sont joignables depuis les réseaux partagés, 8081
protégé par le seul jeton.
