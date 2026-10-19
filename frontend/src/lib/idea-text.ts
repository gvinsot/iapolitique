import { isValidEmail } from "./email";

/** Submission rules for an idea (AGENTS.md §5.1). The API validates again: this is UX only. */
export const IDEA_TEXT_MIN_LENGTH = 50;
export const IDEA_TEXT_MAX_LENGTH = 4000;
export const TERRITORY_MAX_LENGTH = 120;

const count = new Intl.NumberFormat("fr-FR");

/** Length as the API counts it: Unicode characters of the trimmed text. */
export function ideaTextLength(text: string): number {
  return Array.from(text.trim()).length;
}

/** Returns a French error message, or null when the text can be submitted. */
export function validateIdeaText(text: string): string | null {
  const length = ideaTextLength(text);
  if (length === 0) {
    return "Décrivez votre idée avant de l’envoyer.";
  }
  if (length < IDEA_TEXT_MIN_LENGTH) {
    return `Votre idée doit comporter au moins ${count.format(IDEA_TEXT_MIN_LENGTH)} caractères (actuellement ${count.format(length)}). Précisez la mesure proposée et ce qu’elle permettrait d’économiser ou de financer.`;
  }
  if (length > IDEA_TEXT_MAX_LENGTH) {
    return `Votre idée ne doit pas dépasser ${count.format(IDEA_TEXT_MAX_LENGTH)} caractères (actuellement ${count.format(length)}).`;
  }
  return null;
}

export function formatCharacterCount(length: number, max: number = IDEA_TEXT_MAX_LENGTH): string {
  return `${count.format(length)} / ${count.format(max)} caractères`;
}

/**
 * Improvement or objection text (AGENTS.md §5.5). The bounds are a frontend default until the
 * API publishes its own: the API validates again and its message is shown if they differ.
 */
export const CONTRIBUTION_TEXT_MIN_LENGTH = 30;
export const CONTRIBUTION_TEXT_MAX_LENGTH = 2000;

export function validateContributionText(text: string): string | null {
  const length = ideaTextLength(text);
  if (length === 0) return "Rédigez votre contribution avant de l’envoyer.";
  if (length < CONTRIBUTION_TEXT_MIN_LENGTH) {
    return `Votre contribution doit comporter au moins ${count.format(CONTRIBUTION_TEXT_MIN_LENGTH)} caractères (actuellement ${count.format(length)}). Expliquez ce qu’elle apporte à l’idée.`;
  }
  if (length > CONTRIBUTION_TEXT_MAX_LENGTH) {
    return `Votre contribution ne doit pas dépasser ${count.format(CONTRIBUTION_TEXT_MAX_LENGTH)} caractères (actuellement ${count.format(length)}).`;
  }
  return null;
}

/** Report form (AGENTS.md §5.6, §7.5): every field but the reason is optional. */
export const REPORT_DETAILS_MAX_LENGTH = 1000;
export const REPORTER_NAME_MAX_LENGTH = 100;
export const REPORTER_EMAIL_MAX_LENGTH = 254;

export function validateReportDetails(text: string): string | null {
  const length = ideaTextLength(text);
  return length > REPORT_DETAILS_MAX_LENGTH
    ? `La précision ne doit pas dépasser ${count.format(REPORT_DETAILS_MAX_LENGTH)} caractères (actuellement ${count.format(length)}).`
    : null;
}

export function validateOptionalEmail(email: string): string | null {
  const value = email.trim();
  if (value === "") return null;
  if (value.length > REPORTER_EMAIL_MAX_LENGTH || !isValidEmail(value)) {
    return "Cette adresse e-mail ne semble pas valide. Corrigez-la ou laissez le champ vide.";
  }
  return null;
}
