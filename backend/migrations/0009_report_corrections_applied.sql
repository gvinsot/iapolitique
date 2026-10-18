-- Corrections requested by a report review (`outcome = 'update'`, AGENTS.md §5.6) are handed to
-- the next analysis of the idea (`{{report}}` of the analysis prompts). This column records the
-- analysis that took them into account: a review with `applied_in_analysis_id IS NULL` still
-- waits for its re-analysis. Additive: older binaries ignore it.
ALTER TABLE report_reviews
    ADD COLUMN applied_in_analysis_id uuid REFERENCES analyses (id) ON DELETE SET NULL;

CREATE INDEX report_reviews_pending_corrections_idx ON report_reviews (idea_id, created_at)
    WHERE outcome = 'update' AND applied_in_analysis_id IS NULL;
