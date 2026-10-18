import { $api } from "@/api/client";
import type { LegalInfo } from "@/api/types";

/** Publisher details and legal page dates (GET /api/legal); they change only with a deployment. */
export function useLegalInfo() {
  return $api.useQuery("get", "/api/legal", {}, { staleTime: 60 * 60_000 });
}

export interface LegalDetails {
  /** Undefined while loading or unavailable; each publisher field is null when not configured. */
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
