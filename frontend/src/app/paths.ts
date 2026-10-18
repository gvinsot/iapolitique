/** Public URLs of the application (French paths, see AGENTS.md). Never hard-code them elsewhere. */
export const paths = {
  home: "/",
  propose: "/proposer",
  idea: (slug: string) => `/idees/${encodeURIComponent(slug)}`,
  track: (token: string) => `/suivi/${encodeURIComponent(token)}`,
  methodology: "/methodologie",
  about: "/a-propos",
  legalNotice: "/mentions-legales",
  privacy: "/confidentialite",
  terms: "/regles",
  licence: "/licence",
  /** Former address of the rules page, redirected to `terms`. */
  legacyTerms: "/cgu",
  admin: {
    dashboard: "/admin",
    moderation: "/admin/moderation",
    decisions: "/admin/decisions",
    costs: "/admin/couts",
    settings: "/admin/reglages",
  },
} as const;

export const SITE_NAME = "iapolitique";
export const SITE_TAGLINE =
  "Des idées concrètes pour des finances publiques plus saines, sans parti pris";
