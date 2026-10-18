/**
 * Publisher details and legal page metadata served by GET /api/legal (AGENTS.md §7.5, §13.2).
 *
 * The publisher's identity is never committed (public repository): it lives in the LEGAL_*
 * environment variables of the api and reaches the browser only through this endpoint. The
 * response is validated at runtime so a partial or malformed answer degrades to the
 * "unavailable" notice instead of rendering `undefined`.
 */

import { formatDate } from "./format";

/** Keys of the legal pages in `pages` (dates of their last update). */
export const legalPageKeys = ["mentions_legales", "confidentialite", "regles", "licence"] as const;
export type LegalPageKey = (typeof legalPageKeys)[number];

export interface LegalPublisher {
  name: string;
  address: string;
  phone: string;
}

export interface LegalInfo {
  publisher: LegalPublisher;
  contact_email: string;
  privacy_email: string;
  /** Version of the rules (CGU) accepted with each contribution. */
  cgu_version: string;
  /** Last update of each legal page, ISO date (`YYYY-MM-DD`) or date-time. */
  pages: Partial<Record<LegalPageKey, string>>;
}

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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function nonEmptyString(value: unknown): string | null {
  return typeof value === "string" && value.trim() !== "" ? value.trim() : null;
}

/** Validates the API answer; returns null when a mandatory field is missing. */
export function parseLegalInfo(value: unknown): LegalInfo | null {
  if (!isRecord(value) || !isRecord(value.publisher)) return null;
  const name = nonEmptyString(value.publisher.name);
  const address = nonEmptyString(value.publisher.address);
  const phone = nonEmptyString(value.publisher.phone);
  const contactEmail = nonEmptyString(value.contact_email);
  const privacyEmail = nonEmptyString(value.privacy_email);
  const cguVersion = nonEmptyString(value.cgu_version);
  if (!name || !address || !phone || !contactEmail || !privacyEmail || !cguVersion) return null;

  const pages: Partial<Record<LegalPageKey, string>> = {};
  if (isRecord(value.pages)) {
    for (const key of legalPageKeys) {
      const date = nonEmptyString(value.pages[key]);
      if (date && !Number.isNaN(Date.parse(date))) pages[key] = date;
    }
  }

  return {
    publisher: { name, address, phone },
    contact_email: contactEmail,
    privacy_email: privacyEmail,
    cgu_version: cguVersion,
    pages,
  };
}

/** "18 octobre 2026" (see formatDate). */
export const formatLegalDate = formatDate;
