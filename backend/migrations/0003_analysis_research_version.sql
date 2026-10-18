-- The analysis runs in two steps (AGENTS.md §13.4): prompts/analysis_research.md (web search,
-- sourced memo) then prompts/analysis.md (structured output). `analyses.prompt_version` holds the
-- version of analysis.md; this column holds the version of analysis_research.md, NULL when the
-- analysis ran as a single call without the research step.
ALTER TABLE analyses ADD COLUMN research_prompt_version text;
