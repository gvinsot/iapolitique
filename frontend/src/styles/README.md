# Identité visuelle et jetons de design

## Intention

iapolitique doit inspirer **confiance** et rester **strictement apolitique**. L'interface est
sobre, lisible, sans effet spectaculaire : ce sont les chiffres et les sources qui comptent.

## Palette

| Rôle                | Choix                                               | Pourquoi                                                                    |
| ------------------- | --------------------------------------------------- | --------------------------------------------------------------------------- |
| Surfaces et texte   | gris chaud « stone » (presque blanc / presque noir) | neutre, reposant, lisible longtemps                                         |
| Accent unique       | **bleu canard** (pétrole), `--primary`              | évoque le sérieux et la donnée sans renvoyer à un parti                     |
| Attente, zone grise | ocre sourd, `--warning`                             | signale « en cours de vérification » sans alarmer                           |
| Erreur              | rouge, `--destructive`                              | **uniquement** sémantique (erreurs, actions destructives), jamais décoratif |

Couleurs volontairement **évitées** comme teinte dominante, car associées à des partis ou
mouvements français : rouge, rose, bleu-blanc-rouge, vert, orange, violet, marine et or.

## Jetons

Les valeurs sont dans [`tokens.css`](./tokens.css) (OKLCH), exposées à Tailwind dans
[`globals.css`](./globals.css) via `@theme inline` (`bg-primary`, `text-muted-foreground`,
`bg-warning-surface`, etc.). Les composants shadcn/ui (`src/components/ui`) ne consomment que ces
jetons : ne jamais coder une couleur en dur dans un composant.

- `--background`, `--foreground` : fond de page et texte courant.
- `--card`, `--popover` (+ `-foreground`) : surfaces surélevées.
- `--primary` (+ `-foreground`) : actions principales, liens, anneau de focus (`--ring`).
- `--secondary`, `--muted` (+ `-foreground`) : surfaces secondaires, texte d'appoint.
- `--accent` (+ `-foreground`) : survol et éléments mis en avant (teinte de l'accent).
- `--destructive` (+ `-foreground`), `--warning`, `--warning-surface` : états.
- `--border` (décoratif), `--input` (bordure des champs, contraste ≥ 3:1).
- `--radius` : rayon de base (0,5 rem).

## Thèmes clair et sombre

Le thème suit `prefers-color-scheme` par défaut. Le bouton de l'en-tête force l'autre thème ;
le choix est mémorisé dans `localStorage` (clé `iapolitique-theme`, jamais de cookie) et
appliqué avant le premier rendu par `public/theme-init.js`. Revenir au thème du système efface
la clé.

## Accessibilité

- Contrastes WCAG 2.2 AA vérifiés **automatiquement** par `contrast.test.ts` pour chaque paire
  texte/surface, dans les deux thèmes (4,5:1 pour le texte, 3:1 pour bordures de champ et focus).
- Focus clavier toujours visible (anneau `--ring` plein, décalé de 2 px).
- Animations neutralisées si `prefers-reduced-motion: reduce`.

## Typographie

Inter (variable), auto-hébergée via `@fontsource-variable/inter` : aucune requête vers un service
tiers. Pile de secours : polices système.
