import type { LegalInfo, LegalPages } from "@/api/types";

import { formatDate } from "./format";

/**
 * Publisher details and legal page metadata served by GET /api/legal (AGENTS.md §7.5, §13.2).
 *
 * The publisher's identity is never committed (public repository): it lives in the LEGAL_*
 * environment variables of the api and reaches the browser only through this endpoint. Each
 * detail is `null` while it is not configured: pages show a visible placeholder instead.
 */

/** Keys of the legal pages in `pages` of GET /api/legal (dates of their last update). */
export type LegalPageKey = keyof LegalPages;
export type { LegalInfo };

/** Shown wherever a detail coming from the API is missing. */
export const LEGAL_DETAILS_UNAVAILABLE = "coordonnées momentanément indisponibles";

/**
 * Date of the texts shipped with this version of the site, used when the API does not give a
 * page date (outage). Update it whenever a legal text of src/pages/legal changes.
 */
export const LEGAL_TEXTS_DATE = "2026-10-18";

export const SITE_DOMAIN = "iapolitique.fr";
export const SOURCE_CODE_URL = "https://github.com/gvinsot/iapolitique";
export const PHAROS_URL = "https://www.internet-signalement.gouv.fr";
export const CNIL_COMPLAINT_URL = "https://www.cnil.fr/fr/plaintes";

export const LICENCE_NAME = "Licence Ouverte 2.0 (Etalab)";
export const LICENCE_SPDX = "etalab-2.0";
export const LICENCE_URL = "https://www.data.gouv.fr/pages/legal/licences/etalab-2.0";
export const CC_BY_URL = "https://creativecommons.org/licenses/by/4.0/deed.fr";

/** Placeholder of the update date in the attribution text (AGENTS.md §7.4). */
export const ATTRIBUTION_DATE_PLACEHOLDER = "AAAA-MM-JJ";

/** Attribution of the original texts of visitors, in the site and in the exports. */
export const CONTRIBUTOR_ATTRIBUTION = `Contribution anonyme – ${SITE_DOMAIN}`;

/** Request added to the attribution (the licence itself forbids extra conditions). */
export const AI_REUSE_REQUEST =
  "Analyses produites automatiquement, non relues par un humain ; vérifiez les sources citées.";

/** Attribution text ready to copy; `updatedOn` is the `YYYY-MM-DD` date of the export used. */
export function attributionText(updatedOn?: string | null): string {
  const date = updatedOn && /^\d{4}-\d{2}-\d{2}/.test(updatedOn) ? updatedOn.slice(0, 10) : null;
  return `Source : ${SITE_DOMAIN} – Propositions citoyennes et analyses générées par IA (Mistral AI), mise à jour du ${date ?? ATTRIBUTION_DATE_PLACEHOLDER} – Licence Ouverte 2.0`;
}

/** "18 octobre 2026" (see formatDate). */
export const formatLegalDate = formatDate;
