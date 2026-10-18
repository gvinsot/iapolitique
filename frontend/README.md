# Frontend iapolitique

SPA React + TypeScript (Vite), servie par nginx. Le cadre général est dans
[`AGENTS.md`](../AGENTS.md) : à lire avant toute modification.

## Commandes

Sans Node installé sur le poste, toutes les commandes passent par Docker (le volume garde les
`node_modules` Linux hors du dépôt) :

```sh
docker run --rm -v "<repo>/frontend:/app" -v iapolitique_node_modules:/app/node_modules -w /app node:22-alpine npm <commande>
```

| Commande            | Rôle                                                                         |
| ------------------- | ---------------------------------------------------------------------------- |
| `npm ci`            | installe les dépendances exactes (`package-lock.json`)                       |
| `npm run dev`       | serveur de dev Vite (port 5173, ajouter `-p 5173:5173` et `-- --host`)       |
| `npm run lint`      | ESLint, Prettier (`--check`) et fraîcheur de `src/api/schema.d.ts`           |
| `npm run typecheck` | `tsc -b` en mode strict                                                      |
| `npm test`          | Vitest + Testing Library (jsdom)                                             |
| `npm run build`     | build de production dans `dist/`                                             |
| `npm run format`    | formate tout le code avec Prettier                                           |
| `npm run gen:api`   | génère `src/api/schema.d.ts` depuis `src/api/openapi.json` (spec du backend) |

Avant chaque commit : `lint`, `typecheck`, `test` et `build` doivent passer.

En développement, Vite relaie `/api/admin` vers `localhost:8081` et le reste de `/api` vers
`localhost:8080` (ports de l'api, AGENTS.md §13.1). Quand le serveur de dev tourne dans Docker et
le backend sur l'hôte (`cargo run`), ajouter `-e API_PROXY_HOST=host.docker.internal` : dans le
conteneur, `localhost` désigne le conteneur lui-même.

## Organisation

```
src/
├── api/            client typé (openapi-fetch), hooks TanStack Query ($api), erreurs, schéma généré
├── app/            fournisseurs globaux, routes, chemins (paths.ts)
├── components/
│   ├── ui/         primitives shadcn/ui (Radix), adaptées aux jetons et au français
│   ├── layout/     en-tête, pied de page, lien d'évitement, gestion du focus
│   ├── common/     en-têtes de page, états vides / erreur / chargement
│   ├── ideas/      composants métier (formulaire de dépôt, filtres du classement)
│   └── theme/      thème clair / sombre
├── hooks/          hooks transverses
├── lib/            fonctions pures testées (format, validation, filtres, libellés)
├── pages/          une page par route (dont admin/ et legal/, chargées à la demande)
├── styles/         jetons de design et styles globaux (voir styles/README.md)
└── test/           configuration et utilitaires de test
```

## Conventions

- **Appels d'API** : uniquement via `$api` / `apiClient` (`src/api/client.ts`), avec les types
  générés. Jamais de type d'API écrit à la main, jamais d'URL absolue : chemins `/api/...`.
- **Erreurs** : toute erreur d'appel est une `ApiError` (`status`, `code`, `message` en français).
  Les requêtes ne réessaient que les erreurs transitoires ; les mutations jamais, et affichent un
  toast d'erreur sauf `meta: { errorToast: false }`.
- **Aucune ressource tierce** : ni CDN, ni police distante, ni analytics, ni cookie. La CSP de
  nginx (`connect-src 'self'`, `script-src 'self'`) bloquerait de toute façon ces appels.
- **Stockage navigateur** : uniquement des préférences (thème en `localStorage`, jeton admin en
  `sessionStorage`), toujours dans un `try/catch`.
- **Accessibilité** : un seul `<h1>` par page, `useDocumentTitle` sur chaque page, libellés sur
  tous les champs, contrastes vérifiés par `src/styles/contrast.test.ts`.
- **Textes** : interface en français (apostrophe typographique ’), code et commentaires en anglais.

## Image Docker

`frontend/Dockerfile`, contexte = racine du dépôt :

```sh
docker build -f frontend/Dockerfile --target runtime -t iapolitique-frontend .
```

- étape `build` : `npm ci` puis `npm run build` (node 22 alpine) ;
- étape `runtime` : nginx alpine sur le port **80**, `/health` pour les sondes ;
- `nginx.conf` : repli SPA vers `index.html`, cache d'un an pour `/assets/` (fichiers
  empreintés), `no-cache` pour le reste, gzip, `server_tokens off`, en-têtes de sécurité (CSP
  stricte, `Referrer-Policy: no-referrer`…), journal d'accès **sans IP, sans user-agent, sans
  query string**, liens de suivi masqués ; `/api` n'est pas relayé (Traefik s'en charge) et
  répond 404 en JSON s'il arrive par erreur jusqu'à nginx.
