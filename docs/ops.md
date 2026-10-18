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
