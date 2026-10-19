/**
 * Numbered acceptance and rejection criteria published in the rules page (/regles, article 4)
 * and referenced by the statement of reasons of a refusal (DSA art. 17, AGENTS.md §7.1).
 *
 * Keys and reason codes mirror prompts/screening.yaml (criteria `key` and `reason_code`, in the
 * order the decision model applies them, AGENTS.md §5.3). The exact refusal message shown to an
 * author comes from the API (`reason_message` of screening.yaml), not from here. The criterion
 * `contact_data` (grid 1.2.0: the author's own name, contact details) has no article of its own:
 * it refuses with the reason code `personal_data`, so rule 4.4 is the one cited.
 */

export type RuleTarget = "idea" | "contribution" | "report";

export interface ScreeningRule {
  /** Article number in /regles, e.g. "4.3". */
  number: string;
  /** Criterion key in prompts/screening.yaml. */
  key: string;
  /** reason_code returned when the criterion fails. */
  reasonCode: string;
  /** Anchor of the article in /regles. */
  anchor: string;
  title: string;
  /** What is required for the text to be accepted. */
  requirement: string;
  /** Texts this criterion applies to. */
  appliesTo: readonly RuleTarget[];
}

export const screeningRules: readonly ScreeningRule[] = [
  {
    number: "4.1",
    key: "injection",
    reasonCode: "injection",
    anchor: "critere-injection",
    title: "Pas de consigne adressée à une IA",
    requirement:
      "Le texte décrit une idée, une amélioration, une objection ou un problème. Il ne contient aucune instruction destinée à manipuler les systèmes d’intelligence artificielle du site (par exemple « ignore les consignes précédentes »).",
    appliesTo: ["idea", "contribution", "report"],
  },
  {
    number: "4.2",
    key: "respectful",
    reasonCode: "disrespectful",
    anchor: "critere-respect",
    title: "Respect des personnes",
    requirement:
      "Le texte ne contient ni insulte, ni propos haineux, ni menace, ni incitation à la violence. Critiquer une dépense, un dispositif ou une politique publique est permis.",
    appliesTo: ["idea", "contribution", "report"],
  },
  {
    number: "4.3",
    key: "targeted",
    reasonCode: "targeted",
    anchor: "critere-ciblage",
    title: "Aucun parti, élu ni personnalité nommé, aucun groupe visé",
    requirement:
      "Le texte ne nomme aucun parti, aucun élu, aucun candidat ni aucune personnalité, même de façon neutre ou élogieuse : décrivez plutôt la mesure elle-même (le dispositif, la dépense ou la recette concernée). Les fonctions et les institutions (« le Gouvernement », « les élus locaux », « le Parlement ») restent permises. Le texte ne donne aucune consigne de vote et ne vise aucun groupe de personnes défini par leur origine, leur religion, leur sexe ou leur orientation.",
    appliesTo: ["idea", "contribution"],
  },
  {
    number: "4.4",
    key: "personal_data",
    reasonCode: "personal_data",
    anchor: "critere-donnees-personnelles",
    title: "Aucun particulier identifiable",
    requirement:
      "Le texte ne permet de reconnaître aucun particulier (nom, prénom, signature, poste occupé par une seule personne dans un service, un établissement ou une ville nommés) et ne lui reproche aucun fait. Les responsables politiques agissant dans leur fonction ne sont pas des particuliers, mais la règle 4.3 interdit de les nommer, même de façon neutre.",
    appliesTo: ["idea", "contribution"],
  },
  {
    number: "4.5",
    key: "on_topic",
    reasonCode: "off_topic",
    anchor: "critere-sujet",
    title: "Une mesure pour les finances publiques",
    requirement:
      "L’idée propose une mesure qui réduirait une dépense ou augmenterait une recette d’une administration publique française : État, sécurité sociale ou collectivité territoriale.",
    appliesTo: ["idea"],
  },
  {
    number: "4.6",
    key: "lawful",
    reasonCode: "unlawful",
    anchor: "critere-droits-fondamentaux",
    title: "Respect des droits fondamentaux",
    requirement:
      "La mesure est compatible avec les droits fondamentaux : égalité devant la loi, non-discrimination, libertés publiques. Elle s’applique selon des critères objectifs (revenus, situation, usage d’un service…).",
    appliesTo: ["idea"],
  },
  {
    number: "4.7",
    key: "realism",
    reasonCode: "unrealistic",
    anchor: "critere-realisme",
    title: "Réalisme",
    requirement:
      "La mesure peut être mise en œuvre en France. Le modèle évalue son réalisme sur quatre niveaux : irréaliste ou absurde, difficile (réforme importante), faisable, simple à mettre en œuvre. Une mesure difficile reste acceptée ; seule une mesure jugée irréaliste ou absurde est refusée, et une version plus progressive ou plus ciblée peut alors être proposée.",
    appliesTo: ["idea"],
  },
  {
    number: "4.8",
    key: "relevant",
    reasonCode: "irrelevant",
    anchor: "critere-pertinence",
    title: "Pertinence par rapport à l’idée",
    requirement:
      "Une amélioration ou une objection porte sur l’idée publiée à laquelle elle est rattachée ; un signalement décrit un problème précis de cette idée ou de son analyse (erreur, source, passage en cause).",
    appliesTo: ["contribution", "report"],
  },
];

export const RULES_ARTICLE_CRITERIA_ANCHOR = "criteres";
export const RULES_SCORE_ANCHOR = "score";
export const RULES_LICENCE_ANCHOR = "licence-contribution";
export const RULES_RECOURSE_ANCHOR = "recours";

/** The rule matching a refusal reason code, if it is a published one. */
export function ruleForReasonCode(reasonCode: string | null | undefined): ScreeningRule | null {
  if (!reasonCode) return null;
  return screeningRules.find((rule) => rule.reasonCode === reasonCode) ?? null;
}

export const ruleTargetLabels: Record<RuleTarget, string> = {
  idea: "idées",
  contribution: "contributions",
  report: "signalements",
};
