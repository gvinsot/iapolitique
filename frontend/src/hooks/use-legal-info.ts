import { useQuery } from "@tanstack/react-query";

import { requestJson } from "@/api/json-fetch";
import { type LegalInfo, parseLegalInfo } from "@/lib/legal";

export const LEGAL_INFO_QUERY_KEY = ["get", "/api/legal"] as const;

/** Reads GET /api/legal, validated at runtime (failures are ApiError, see api/errors.ts). */
export function fetchLegalInfo(signal?: AbortSignal): Promise<LegalInfo> {
  return requestJson("GET", "/api/legal", parseLegalInfo, { signal });
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
