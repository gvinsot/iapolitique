/** Open data files of the published ideas, served by the API (AGENTS.md §7.4, §13.2). */
export const EXPORT_URLS = {
  json: "/api/exports/ideas.json",
  csv: "/api/exports/ideas.csv",
  license: "/api/exports/LICENSE",
  metadata: "/api/exports/metadata.json",
} as const;
