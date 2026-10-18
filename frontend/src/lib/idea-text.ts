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

export function formatCharacterCount(length: number): string {
  return `${count.format(length)} / ${count.format(IDEA_TEXT_MAX_LENGTH)} caractères`;
}
