# Backend iapolitique

Binaire Rust unique (`iapolitique`) : API publique et admin, worker, migrations. Les règles
produit et les contrats techniques sont dans [`AGENTS.md`](../AGENTS.md) (§4.1, §13).

## Commandes

| Commande | Rôle |
|---|---|
| `iapolitique api` | API publique (8080) et admin (8081), applique les migrations au démarrage |
| `iapolitique worker` | file de jobs, santé interne sur 8082 (`/health`) |
| `iapolitique migrate` | applique les migrations puis s'arrête |
| `iapolitique openapi [--output FICHIER] [--public]` | spec OpenAPI (complète par défaut) |
| `iapolitique healthcheck [--url URL]` | GET HTTP, code de sortie 0 si 2xx (healthchecks Docker) |

Configuration : variables d'environnement listées au §13.1 d'`AGENTS.md`
(`src/config.rs`). Les fichiers de `/run/secrets/` sont chargés dans l'environnement au
démarrage s'ils n'y sont pas déjà.

## Organisation

| Chemin | Contenu |
|---|---|
| `src/domain/` | sans I/O : machines à états (`status.rs`), score (`scoring.rs`), montants, slugs, validation des textes, énumérations partagées |
| `src/config.rs` | configuration typée, secrets masqués dans les logs |
| `src/db/` | pool PostgreSQL, migrations embarquées (`MIGRATOR`), tests du schéma |
| `migrations/` | schéma complet (§5 à §8), migrations additives uniquement |
| `src/jobs/` | file de jobs PostgreSQL, worker, planification quotidienne, purge (§7.3) |
| `src/http/` | routeurs public et admin, erreurs JSON unifiées, extracteurs, IP client, OpenAPI |
| `src/app.rs` | démarrage des processus api et worker, arrêt propre sur SIGTERM |

Ajouter un traitement asynchrone : implémenter `jobs::worker::JobHandler` pour un
`jobs::JobKind`, l'enregistrer dans `jobs::worker::default_registry`, et l'enfiler avec
`jobs::queue::enqueue` (dans la même transaction que le changement d'état qui le déclenche).
Les charges utiles et messages d'erreur des jobs ne contiennent que des identifiants.

Ajouter une route : handler annoté `#[utoipa::path]`, ajouté au routeur (`src/http/mod.rs`)
et à `ApiDoc` (`src/http/openapi.rs`), puis régénérer la spec :
`cargo run -- openapi --output ../frontend/src/api/openapi.json` (un test échoue sinon) et
`npm run gen:api` côté frontend.

## Tests

```sh
docker run -d --name iapolitique-test-pg -e POSTGRES_PASSWORD=postgres -p 55432:5432 pgvector/pgvector:pg18
DATABASE_URL=postgres://postgres:postgres@localhost:55432/postgres cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

`#[sqlx::test]` crée une base temporaire par test et y applique les migrations. Aucun test
n'appelle le modèle de décision ni Mistral.

## Image Docker

`Dockerfile` (contexte : racine du dépôt) : stage `build` puis `runtime` (Debian slim,
utilisateur non root, binaire `/app/iapolitique`, prompts dans `/app/prompts`).

En production, l'extension `vector` doit être créée par un superutilisateur avant le premier
démarrage (`CREATE EXTENSION vector;`) : l'utilisateur applicatif ne peut pas la créer.
