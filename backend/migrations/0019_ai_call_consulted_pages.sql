-- Web research (AGENTS.md §6.1): number of distinct pages of the answer that an analysis may
-- cite (`tool_reference` chunks with a publishable http(s) URL). NULL for the other calls, for
-- a research call without an answer, and for the calls recorded before this migration. Tells a
-- model that did not search (0 with no web search counted) from references the parser missed
-- (0 with web searches counted). Additive and nullable: older binaries ignore the column.
ALTER TABLE ai_calls ADD COLUMN consulted_pages integer CHECK (consulted_pages >= 0);
