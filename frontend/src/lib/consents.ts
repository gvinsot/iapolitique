/**
 * The two boxes required before any publishable submission (idea, improvement, objection),
 * never pre-checked (AGENTS.md §7.5, docs/juridique/memo.md §1).
 */
export interface PublicationConsents {
  /** Licence clause of the rules (article 11). */
  licence: boolean;
  /** "Je comprends que mon texte sera publié s’il est accepté." */
  publication: boolean;
}

export const noConsents: PublicationConsents = { licence: false, publication: false };

export const CONSENTS_REQUIRED_MESSAGE =
  "Cochez les deux cases pour envoyer votre texte : elles conditionnent sa publication.";

export function validateConsents(consents: PublicationConsents): string | null {
  return consents.licence && consents.publication ? null : CONSENTS_REQUIRED_MESSAGE;
}
