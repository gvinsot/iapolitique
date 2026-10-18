-- Report form (AGENTS.md §5.6, §7.5): optional name and e-mail of the reporter, kept only with
-- the report and erased after 6 months by the daily purge; mandatory declaration of good faith.
-- Never published, never exported, never sent to an AI.

ALTER TABLE reports
    ADD COLUMN reporter_name text CHECK (char_length(reporter_name) BETWEEN 1 AND 100),
    ADD COLUMN reporter_email text CHECK (char_length(reporter_email) BETWEEN 3 AND 254),
    -- The form cannot be sent without the declaration; false only for rows created before.
    ADD COLUMN good_faith boolean NOT NULL DEFAULT false,
    ADD COLUMN reporter_purged_at timestamptz;

CREATE INDEX reports_reporter_contact_idx ON reports (created_at)
    WHERE reporter_name IS NOT NULL OR reporter_email IS NOT NULL;
