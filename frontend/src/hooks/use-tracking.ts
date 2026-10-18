import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { deleteTrackedItem, fetchTrackedItem, requestHumanReview } from "@/api/tracking";

/** The token stays in memory (query cache), never in a URL nor in a log. */
function trackingKey(token: string | null) {
  return ["post", "/api/track", token] as const;
}

/** Status of the submission behind a tracking token (null: nothing to load). */
export function useTrackedItem(token: string | null) {
  return useQuery({
    queryKey: trackingKey(token),
    queryFn: ({ signal }) => {
      if (token === null) throw new Error("No tracking token");
      return fetchTrackedItem(token, signal);
    },
    enabled: token !== null,
    staleTime: 30_000,
  });
}

/** Deletion by the author; errors are shown inline by the tracking actions. */
export function useDeleteTrackedItem(token: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => deleteTrackedItem(token),
    meta: { errorToast: false },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: trackingKey(token) }),
  });
}

/** Human review request; errors are shown inline by the tracking actions. */
export function useRequestHumanReview(token: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (message: string | null) => requestHumanReview(token, message),
    meta: { errorToast: false },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: trackingKey(token) }),
  });
}
