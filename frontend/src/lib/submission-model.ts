import type { SubmissionResponse } from "@/api/types";

/**
 * Immediate answer to a submission (POST /api/ideas, …/contributions, …/reports; AGENTS.md
 * §5.2): the screening outcome, the statement of reasons of a refusal and the tracking token,
 * returned only this once.
 */
export type SubmissionResult = SubmissionResponse;
