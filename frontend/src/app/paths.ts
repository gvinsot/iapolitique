import { TRACKING_PATH } from "@/lib/tracking-token";

/** Public URLs of the application (French paths, see AGENTS.md). Never hard-code them elsewhere. */
export const paths = {
  home: "/",
  propose: "/proposer",
  idea: (slug: string) => `/idees/${encodeURIComponent(slug)}`,
  /** Tracking page; the token goes in the fragment only (see lib/tracking-token.ts). */
  track: TRACKING_PATH,
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
    ideas: "/admin/idees",
    replies: (ideaId: string) => `/admin/idees/${encodeURIComponent(ideaId)}/reponses`,
    reports: "/admin/signalements",
    escalations: "/admin/escalades",
    /** Sealed copies of escalated content (owner decision D3). */
    seals: "/admin/copies-scellees",
    seal: (escalationId: string) =>
      `/admin/escalades/${encodeURIComponent(escalationId)}/copie-scellee`,
    reviewRequests: "/admin/reexamens",
    decisions: "/admin/decisions",
    jobs: "/admin/jobs",
    costs: "/admin/couts",
    settings: "/admin/reglages",
    journal: "/admin/journal",
    /** Former address of the moderation page, redirected to `reports`. */
    moderation: "/admin/moderation",
  },
} as const;

export const SITE_NAME = "iapolitique";
export const SITE_TAGLINE =
  "Des idées concrètes pour des finances publiques plus saines, sans parti pris";
