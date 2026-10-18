import { useQuery } from "@tanstack/react-query";

import { apiErrorFromResponse, isAbortError, networkError } from "@/api/errors";
import { type LegalInfo, parseLegalInfo } from "@/lib/legal";

export const LEGAL_INFO_QUERY_KEY = ["get", "/api/legal"] as const;

/**
 * Reads GET /api/legal. Uses a plain same-origin fetch validated at runtime; failures are
 * normalised to ApiError like every other call (see src/api/errors.ts).
 */
export async function fetchLegalInfo(signal?: AbortSignal): Promise<LegalInfo> {
  let response: Response;
  try {
    response = await globalThis.fetch(new URL("/api/legal", window.location.origin), {
      headers: { Accept: "application/json" },
      credentials: "omit",
      signal: signal ?? null,
    });
  } catch (error) {
    if (isAbortError(error)) throw error;
    throw networkError(error);
  }
  if (!response.ok) throw await apiErrorFromResponse(response);
  const info = parseLegalInfo(await response.json());
  if (!info) throw new Error("Unexpected /api/legal response");
  return info;
}

/** Publisher details and legal page dates; `data` stays undefined while unavailable. */
export function useLegalInfo() {
  return useQuery({
    queryKey: LEGAL_INFO_QUERY_KEY,
    queryFn: ({ signal }) => fetchLegalInfo(signal),
    // Changes only with a redeployment of the api.
    staleTime: 60 * 60_000,
  });
}

export interface LegalDetails {
  info: LegalInfo | undefined;
  pending: boolean;
  failed: boolean;
  retry: () => void;
}

/** Publisher details for the legal pages, with explicit loading and failure states. */
export function useLegalDetails(): LegalDetails {
  const query = useLegalInfo();
  return {
    info: query.data,
    pending: query.isPending,
    failed: query.isError,
    retry: () => {
      void query.refetch();
    },
  };
}
