import { parseTrackedItem, type TrackedItem } from "@/lib/tracking-model";

import { requestJson } from "./json-fetch";

/**
 * Tracking calls (AGENTS.md §7.1, §13.2). The token always travels in the JSON body of a POST,
 * never in a URL.
 */

export function fetchTrackedItem(token: string, signal?: AbortSignal): Promise<TrackedItem> {
  return requestJson("POST", "/api/track", parseTrackedItem, { body: { token }, signal });
}

/** Erases the text at once; resolves when the API confirmed. */
export async function deleteTrackedItem(token: string): Promise<void> {
  await requestJson("POST", "/api/track/delete", () => true, { body: { token } });
}

/** Queues a human review, with an optional message for the administrator. */
export async function requestHumanReview(token: string, message: string | null): Promise<void> {
  await requestJson("POST", "/api/track/review-request", () => true, {
    body: message ? { token, message } : { token },
  });
}
