import { isWellFormedTrackingToken } from "./tracking-token";

/**
 * Immediate answer to a submission (POST /api/ideas, …/contributions, …/reports; AGENTS.md
 * §5.2): the screening outcome and the tracking token, returned only this once.
 */
export interface SubmissionResult {
  /** Status right after the synchronous screening (submitted, second_opinion, accepted…). */
  status: string;
  tracking_token: string;
  /** Refusal reason (reason_code and reason_message of prompts/screening.yaml). */
  reason_code: string | null;
  reason_message: string | null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function optionalString(value: unknown): string | null {
  return typeof value === "string" && value !== "" ? value : null;
}

export function parseSubmissionResult(value: unknown): SubmissionResult | null {
  if (!isRecord(value)) return null;
  const status = optionalString(value.status);
  const token = optionalString(value.tracking_token);
  if (!status || !isWellFormedTrackingToken(token)) return null;
  return {
    status,
    tracking_token: token,
    reason_code: optionalString(value.reason_code),
    reason_message: optionalString(value.reason_message),
  };
}
