import { $api } from "@/api/client";

export interface ExportStatus {
  /** `YYYY-MM-DD` day of the latest generation, null while unknown. */
  generatedOn: string | null;
  /** Number of ideas in the latest export, when given. */
  ideaCount: number | null;
  pending: boolean;
  /** No export generated yet (503 `exports_unavailable`) or API unreachable. */
  unavailable: boolean;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/**
 * Date of the latest open data export (GET /api/exports/metadata.json, AGENTS.md §7.4). The
 * file is documented as free JSON: only `generated_at` and `idea_count` are read, checked here.
 */
export function useExportStatus(): ExportStatus {
  // No retry: before the first generation the API answers 503 with Retry-After: 300.
  const query = $api.useQuery(
    "get",
    "/api/exports/metadata.json",
    {},
    { staleTime: 5 * 60_000, retry: false },
  );
  const metadata: unknown = query.data;
  const generatedAt = isRecord(metadata) ? metadata.generated_at : undefined;
  const ideaCount = isRecord(metadata) ? metadata.idea_count : undefined;
  return {
    generatedOn:
      typeof generatedAt === "string" && /^\d{4}-\d{2}-\d{2}/.test(generatedAt)
        ? generatedAt.slice(0, 10)
        : null,
    ideaCount: typeof ideaCount === "number" && Number.isInteger(ideaCount) ? ideaCount : null,
    pending: query.isPending,
    unavailable: query.isError,
  };
}
