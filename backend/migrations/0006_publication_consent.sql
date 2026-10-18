-- Consent to the publication rules (AGENTS.md §7.5): each publishable text (idea, contribution)
-- stores the version of the rules (CGU) accepted with it and the day of the acceptance, nothing
-- else. NULL only for rows created before this migration and for merged duplicates, which carry
-- the consent of their source idea.

ALTER TABLE ideas
    ADD COLUMN cgu_version text CHECK (char_length(cgu_version) BETWEEN 1 AND 20),
    ADD COLUMN cgu_accepted_at date,
    ADD CONSTRAINT ideas_cgu_consent_complete
        CHECK ((cgu_version IS NULL) = (cgu_accepted_at IS NULL));

ALTER TABLE contributions
    ADD COLUMN cgu_version text CHECK (char_length(cgu_version) BETWEEN 1 AND 20),
    ADD COLUMN cgu_accepted_at date,
    ADD CONSTRAINT contributions_cgu_consent_complete
        CHECK ((cgu_version IS NULL) = (cgu_accepted_at IS NULL));
