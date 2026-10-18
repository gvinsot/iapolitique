import { apiClient } from "./client";
import { requireData } from "./errors";
import type { TrackedItem } from "./types";

/**
 * Tracking calls (AGENTS.md §7.1, §13.2). The token always travels in the JSON body of a POST,
 * never in a URL, and no request of the tracking page sends a Referer.
 */
const PRIVATE = { referrerPolicy: "no-referrer" } as const;

export async function fetchTrackedItem(token: string, signal?: AbortSignal): Promise<TrackedItem> {
  const { data } = await apiClient.POST("/api/track", { ...PRIVATE, body: { token }, signal });
  return requireData(data);
}

/** Erases the text at once; resolves when the API confirmed. */
export async function deleteTrackedItem(token: string): Promise<void> {
  await apiClient.POST("/api/track/delete", { ...PRIVATE, body: { token } });
}

/** Queues a human review, with an optional message for the administrator. */
export async function requestHumanReview(token: string, message: string | null): Promise<void> {
  await apiClient.POST("/api/track/review-request", {
    ...PRIVATE,
    body: message ? { token, message } : { token },
  });
}
