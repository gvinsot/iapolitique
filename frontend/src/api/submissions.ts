import type { ContributionFormValues } from "@/components/ideas/contribution-form";
import type { ProposeFormValues } from "@/components/ideas/propose-form";
import type { ReportFormValues } from "@/components/ideas/report-form";

import { apiClient } from "./client";
import { requireData } from "./errors";
import type { SubmissionResponse } from "./types";

/**
 * Public submissions (AGENTS.md §5.1, §13.2). The consents are sent explicitly so the API can
 * store `cgu_version` and `cgu_accepted_at` (§7.5). Failures reject with an ApiError whose
 * French message is shown next to the form.
 */

export async function submitIdea(values: ProposeFormValues): Promise<SubmissionResponse> {
  const { data } = await apiClient.POST("/api/ideas", {
    body: {
      text: values.text,
      admin_level: values.adminLevel,
      territory: values.territory,
      altcha: values.altcha,
      accept_licence: values.acceptedLicence,
      accept_publication: values.acceptedPublication,
    },
  });
  return requireData(data);
}

export async function submitContribution(
  slug: string,
  values: ContributionFormValues,
): Promise<SubmissionResponse> {
  const { data } = await apiClient.POST("/api/ideas/{slug}/contributions", {
    params: { path: { slug } },
    body: {
      kind: values.kind,
      text: values.text,
      altcha: values.altcha,
      accept_licence: values.acceptedLicence,
      accept_publication: values.acceptedPublication,
    },
  });
  return requireData(data);
}

export async function submitReport(
  slug: string,
  values: ReportFormValues,
): Promise<SubmissionResponse> {
  const { data } = await apiClient.POST("/api/ideas/{slug}/reports", {
    params: { path: { slug } },
    body: {
      reason: values.reason,
      details: values.details,
      reporter_name: values.reporterName,
      reporter_email: values.reporterEmail,
      good_faith: values.goodFaith,
      altcha: values.altcha,
    },
  });
  return requireData(data);
}
