import type { SubmissionResult } from "@/lib/submission-model";
import type { TrackedItem } from "@/lib/tracking-model";

/** Fictitious answers of POST /api/track and of the submission endpoints. */

export const TRACKING_TOKEN = "AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_abcde";

export const rejectedIdeaFixture: TrackedItem = {
  kind: "idea",
  status: "rejected",
  submitted_on: "2026-10-18",
  idea: null,
  decision: {
    measure: "rejected",
    reason_code: "off_topic",
    reason_message:
      "Votre texte ne propose pas de mesure qui réduirait une dépense ou augmenterait une recette d’une administration publique.",
    rule: {
      article: "4.5",
      title: "Une mesure pour les finances publiques",
      url: "/regles#critere-sujet",
    },
    automated: true,
    decided_on: "2026-10-18",
    purge_on: "2026-11-17",
    second_opinion: true,
  },
  report_outcome: null,
  review_request: null,
};

export const publishedIdeaFixture: TrackedItem = {
  kind: "idea",
  status: "published",
  submitted_on: "2026-10-10",
  idea: { slug: "mutualiser-achats", title: "Mutualiser les achats des communes" },
  decision: null,
  report_outcome: null,
  review_request: null,
};

export const mergedIdeaFixture: TrackedItem = { ...publishedIdeaFixture, status: "merged" };

export const hiddenIdeaFixture: TrackedItem = {
  ...publishedIdeaFixture,
  status: "hidden",
  decision: {
    measure: "hidden",
    reason_code: "hidden",
    reason_message: "L’analyse reposait sur une source qui ne contient pas le chiffre cité.",
    rule: { article: "7", title: "Mesures et durées", url: "/regles#mesures" },
    automated: true,
    decided_on: "2026-10-20",
    purge_on: "2027-10-20",
    second_opinion: false,
  },
};

export const reviewedReportFixture: TrackedItem = {
  kind: "report",
  status: "reviewed",
  submitted_on: "2026-10-19",
  idea: publishedIdeaFixture.idea,
  decision: null,
  report_outcome: "update",
  review_request: null,
};

/** A contribution whose idea was withdrawn by its author (owner decision D2). */
export const withdrawnIdeaContributionFixture: TrackedItem = {
  kind: "contribution",
  status: "integrated",
  submitted_on: "2026-10-12",
  idea: null,
  decision: null,
  report_outcome: null,
  review_request: null,
  idea_withdrawn: true,
};

export const acceptedIdeaResultFixture: SubmissionResult = {
  status: "accepted",
  tracking_token: TRACKING_TOKEN,
  reason_code: null,
  reason_message: null,
  statement: null,
};

export const rejectedIdeaResultFixture: SubmissionResult = {
  status: "rejected",
  tracking_token: TRACKING_TOKEN,
  reason_code: "personal_data",
  reason_message: "Votre texte permet de reconnaître une personne.",
  statement: {
    measure: "rejected",
    reason_code: "personal_data",
    reason_message: "Votre texte permet de reconnaître une personne.",
    rule: {
      article: "4.4",
      title: "Aucun particulier identifiable",
      url: "/regles#critere-donnees-personnelles",
    },
    automated: true,
    second_opinion: false,
    decided_on: "2026-10-18",
    purge_on: "2026-11-17",
  },
};
