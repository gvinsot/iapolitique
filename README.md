# iapolitique

Plateforme **apolitique** qui recueille, filtre, améliore et chiffre des idées pour réduire les
dépenses ou augmenter les recettes des administrations publiques françaises.

- N'importe qui propose une idée, anonymement et sans compte.
- Un modèle de décision (d3-mini) la filtre immédiatement : respectueuse, dans le sujet, réaliste.
- Mistral analyse les idées retenues : reformulation neutre, pour et contre, impacts par public,
  chiffrage sourcé avec une fourchette basse, médiane et haute.
- Le portail classe les idées par rentabilité (score composé et public).

Site : [iapolitique.fr](https://iapolitique.fr).

**Tout le cadre du projet est dans [AGENTS.md](AGENTS.md)** : mission et principes, architecture,
règles de travail (un commit par unité de travail, poussé sur `main`), contrats techniques.
L'exploitation (base de données, secrets, logs) est décrite dans [docs/ops.md](docs/ops.md).

## Arborescence

| Dossier | Contenu |
|---|---|
| `backend/` | API et worker en Rust (axum, sqlx), migrations |
| `frontend/` | SPA React + TypeScript (Vite, Tailwind, shadcn/ui), servie par nginx |
| `prompts/` | Prompts, grille de filtrage et taxonomie versionnés, publiés sur la page Méthodologie |
| `devops/` | Stack Docker Swarm, image de CI, `.env.example` |
| `docs/` | Documentation fonctionnelle et d'exploitation |

## Développement local

Prérequis : Rust stable, Docker. Node n'a pas besoin d'être installé : npm tourne dans un
conteneur.

```bash
# PostgreSQL + pgvector jetable pour les tests d'intégration (#[sqlx::test])
docker run -d --name iapolitique-test-pg -e POSTGRES_PASSWORD=postgres -p 55432:5432 pgvector/pgvector:pg18

# Backend : formatage, lint, tests (les appels IA sont simulés, jamais réels)
cd backend
cargo fmt --check
cargo clippy --all-targets -- -D warnings
DATABASE_URL=postgres://postgres:postgres@localhost:55432/postgres cargo test

# Frontend : npm via Docker (node_modules dans un volume nommé)
docker run --rm -v "$PWD/frontend:/app" -v iapolitique_node_modules:/app/node_modules -w /app node:22-alpine npm ci
docker run --rm -v "$PWD/frontend:/app" -v iapolitique_node_modules:/app/node_modules -w /app node:22-alpine npm run lint
docker run --rm -v "$PWD/frontend:/app" -v iapolitique_node_modules:/app/node_modules -w /app node:22-alpine npm run typecheck
docker run --rm -v "$PWD/frontend:/app" -v iapolitique_node_modules:/app/node_modules -w /app node:22-alpine npm test
```

Pour lancer l'application : `cargo run -- api` (ports 8080 et 8081) et `cargo run -- worker`
avec les variables de [`devops/.env.example`](devops/.env.example) (au minimum
`DATABASE_CONNECTION_STRING`, `ADMIN_API_TOKEN` et `ALTCHA_HMAC_KEY`), puis le serveur Vite, qui
redirige `/api` vers le backend :

```bash
docker run --rm -it -p 5173:5173 -e API_PROXY_HOST=host.docker.internal -v "$PWD/frontend:/app" -v iapolitique_node_modules:/app/node_modules -w /app node:22-alpine npm run dev -- --host
```

Le modèle de décision est joignable depuis le LAN : `DECISION_SERVICE_URL=http://192.168.1.27:8001/v1/systemone`.

La CI complète (mêmes contrôles que PulsarCD) se lance depuis `devops/` :

```bash
cd devops
docker compose -f docker-compose.swarm.yml build test
docker compose -f docker-compose.swarm.yml run --rm test
docker compose -f docker-compose.swarm.yml down
```

Il faut un `devops/.env` (copie de `.env.example`) car le fichier compose exige certaines variables.

## Déploiement

Uniquement par **PulsarCD** (stack `iapolitique`), jamais par `docker stack deploy` à la main.
Chaque commit poussé sur `main` est construit (images `registry.methodinfo.fr/iapolitique-*`),
testé (service `test`), puis déployé automatiquement en QA (`qa.iapolitique.fr`) si les tests
passent. La promotion en production est décidée par un humain. Détails dans
[AGENTS.md §10](AGENTS.md#10-déploiement-pulsarcd--docker-swarm) et [docs/ops.md](docs/ops.md).

## Licence

Code sous licence [MIT](LICENSE).
