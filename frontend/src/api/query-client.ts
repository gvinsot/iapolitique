import { MutationCache, QueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { ApiError, errorMessage } from "./errors";

declare module "@tanstack/react-query" {
  interface Register {
    mutationMeta: {
      /** Set to false when the component displays the error itself (e.g. inline form error). */
      errorToast?: boolean;
    };
  }
}

const MAX_QUERY_RETRIES = 2;

/** Retries only failures that may resolve by themselves; never a 4xx or a bug. */
export function shouldRetryQuery(failureCount: number, error: unknown): boolean {
  return failureCount < MAX_QUERY_RETRIES && error instanceof ApiError && error.isTransient;
}

/** 1 s, 2 s, 4 s… capped at 10 s, or what the server asked for (Retry-After). */
export function queryRetryDelay(attemptIndex: number, error: unknown): number {
  if (error instanceof ApiError && error.retryAfterSeconds !== null) {
    return Math.min(error.retryAfterSeconds * 1000, 30_000);
  }
  return Math.min(1000 * 2 ** attemptIndex, 10_000);
}

export function createQueryClient(): QueryClient {
  return new QueryClient({
    mutationCache: new MutationCache({
      onError: (error, _variables, _context, mutation) => {
        if (mutation.meta?.errorToast === false) return;
        toast.error(errorMessage(error));
      },
    }),
    defaultOptions: {
      queries: {
        // Published ideas change slowly: avoid refetching on every navigation.
        staleTime: 60_000,
        gcTime: 5 * 60_000,
        retry: shouldRetryQuery,
        retryDelay: queryRetryDelay,
        refetchOnWindowFocus: false,
      },
      mutations: {
        // Submissions are not idempotent (rate limit, proof of work): never replay them.
        retry: false,
      },
    },
  });
}
