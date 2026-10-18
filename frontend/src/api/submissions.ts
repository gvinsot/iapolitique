import type { ContributionFormValues } from "@/components/ideas/contribution-form";
import type { ProposeFormValues } from "@/components/ideas/propose-form";
import type { ReportFormValues } from "@/components/ideas/report-form";
import { parseSubmissionResult, type SubmissionResult } from "@/lib/submission-model";

import { requestJson } from "./json-fetch";

/**
 * Public submissions (AGENTS.md §13.2). Bodies use the API's snake_case names; the consents
 * are sent explicitly so the API can store `cgu_version` and `cgu_accepted_at` (§7.5).
 */

export function submitIdea(values: ProposeFormValues): Promise<SubmissionResult> {
  return requestJson("POST", "/api/ideas", parseSubmissionResult, {
    body: {
      text: values.text,
      admin_level: values.adminLevel,
      territory: values.territory,
      altcha: values.altcha,
      accept_licence: values.acceptedLicence,
      accept_publication: values.acceptedPublication,
    },
  });
}

export function submitContribution(
  slug: string,
  values: ContributionFormValues,
): Promise<SubmissionResult> {
  return requestJson(
    "POST",
    `/api/ideas/${encodeURIComponent(slug)}/contributions`,
    parseSubmissionResult,
    {
      body: {
        kind: values.kind,
        text: values.text,
        altcha: values.altcha,
        accept_licence: values.acceptedLicence,
        accept_publication: values.acceptedPublication,
      },
    },
  );
}

export function submitReport(slug: string, values: ReportFormValues): Promise<SubmissionResult> {
  return requestJson(
    "POST",
    `/api/ideas/${encodeURIComponent(slug)}/reports`,
    parseSubmissionResult,
    {
      body: {
        reason: values.reason,
        details: values.details,
        reporter_name: values.reporterName,
        reporter_email: values.reporterEmail,
        good_faith: values.goodFaith,
        altcha: values.altcha,
      },
    },
  );
}
