-- prompts/report_review.schema.json returns `corrections` besides `outcome` and `explanation`
-- (AGENTS.md §13.4): the instructions passed as {{report}} to the re-analysis when the outcome is
-- `update`, empty otherwise. Stored so that the re-analysis job and the admin can read them back.
ALTER TABLE report_reviews ADD COLUMN corrections text NOT NULL DEFAULT '';
ALTER TABLE report_reviews ADD CONSTRAINT report_reviews_corrections_only_on_update
    CHECK (outcome = 'update' OR corrections = '');
