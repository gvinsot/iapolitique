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
    reason_code: "off_topic",
    reason_message:
      "Votre texte ne propose pas de mesure qui réduirait une dépense ou augmenterait une recette d’une administration publique.",
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
    reason_code: "report_hide",
    reason_message: "L’analyse reposait sur une source qui ne contient pas le chiffre cité.",
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

export const acceptedIdeaResultFixture: SubmissionResult = {
  status: "accepted",
  tracking_token: TRACKING_TOKEN,
  reason_code: null,
  reason_message: null,
};

export const rejectedIdeaResultFixture: SubmissionResult = {
  status: "rejected",
  tracking_token: TRACKING_TOKEN,
  reason_code: "personal_data",
  reason_message: "Votre texte permet de reconnaître une personne.",
};
