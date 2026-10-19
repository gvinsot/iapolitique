import { $api } from "@/api/client";

/** Aggregate public counters (GET /api/stats, cached 60 s by the API). */
export function usePublicStats() {
  return $api.useQuery("get", "/api/stats", {}, { staleTime: 60_000 });
}
