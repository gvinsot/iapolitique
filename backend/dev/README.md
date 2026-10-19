# Données et pile de développement

Outils pour faire tourner le site en local et remplir les pages publiques sans clé Mistral.
**Jamais en production ni en QA** : `seed.sql` refuse de s'exécuter sur une base dont le nom
contient `prod` ou `qa`, et les données qu'il insère sont fictives (montants, sources et textes
inventés pour tester l'affichage).

## Pile locale

1. PostgreSQL de test (pgvector) et une base vide :

   ```sh
   docker start iapolitique-test-pg   # ou docker run, voir ../README.md
   docker exec iapolitique-test-pg psql -U postgres -c "CREATE DATABASE iapolitique_dev"
   docker exec iapolitique-test-pg psql -U postgres -d iapolitique_dev -c "CREATE EXTENSION vector"
   ```

2. api et worker, depuis `backend/`, avec des secrets factices (32 caractères au moins) :

   ```sh
   export DATABASE_CONNECTION_STRING=postgres://postgres:postgres@localhost:55432/iapolitique_dev
   export ADMIN_API_TOKEN=dev-admin-token-0123456789abcdefghijklmnop
   export ALTCHA_HMAC_KEY=dev-altcha-key-0123456789abcdefghijklmnopqr
   export PROMPTS_DIR=../prompts LOG_FORMAT=text
   # Modèle de décision joignable depuis le LAN ; sans lui, les textes restent `submitted`.
   export DECISION_SERVICE_URL=http://192.168.1.27:8001/v1/systemone DECISION_SERVICE_TIMEOUT_SECS=30
   # Coordonnées fictives pour les pages légales (facultatif).
   export LEGAL_PUBLISHER_NAME="Camille Exemple" LEGAL_CONTACT_EMAIL=contact@example.org
   cargo run -- api      # puis, dans un autre terminal avec le même environnement :
   cargo run -- worker
   ```

   Sans `MISTRAL_API_KEY`, le worker laisse en file les jobs IA (second avis, doublons,
   analyse, revue des signalements) : une idée acceptée reste `accepted`. Pour faire tourner
   tout le pipeline sans appel payant, lancer le faux Mistral
   (`python ../devops/dev/mock_mistral.py --port 8090`) et ajouter
   `MISTRAL_API_BASE_URL=http://127.0.0.1:8090 MISTRAL_API_KEY=dev-mock` à l'environnement
   (voir `devops/dev/README.md`).

3. Données de démonstration (six idées publiées avec analyse complète, deux contributions
   intégrées, un droit de réponse, une idée masquée qui doit répondre 404), puis génération des
   exports par le worker :

   ```sh
   docker exec -i iapolitique-test-pg psql -U postgres -d iapolitique_dev -v ON_ERROR_STOP=1 < dev/seed.sql
   ```

   Le script peut être relancé : il supprime d'abord les idées qu'il a créées.

4. Frontend (sans Node local, serveur Vite dans un conteneur qui joint l'api de l'hôte) :

   ```sh
   docker run --rm -it -p 5173:5173 -e API_PROXY_HOST=host.docker.internal \
     -v "<dépôt>/frontend:/app" -v iapolitique_node_modules:/app/node_modules -w /app \
     node:22-alpine npm run dev -- --host 0.0.0.0
   ```

   Site public sur <http://localhost:5173>, administration sur <http://localhost:5173/admin>
   (jeton : `ADMIN_API_TOKEN`).
