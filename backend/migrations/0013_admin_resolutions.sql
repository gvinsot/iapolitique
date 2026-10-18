-- How the administrator closed the human queues (AGENTS.md §7.1, §7.5, §8). Additive: older
-- binaries never write these columns.

-- Escalation of serious content: forwarded to PHAROS (`transmitted`) or not (`dismissed`), with
-- the administrator's note (date of the referral, reference…), never a visitor's text.
ALTER TABLE escalations
    ADD COLUMN outcome text CHECK (outcome IN ('transmitted', 'dismissed')),
    ADD COLUMN note text CHECK (char_length(note) BETWEEN 1 AND 2000),
    ADD CONSTRAINT escalations_outcome_when_done CHECK (outcome IS NULL OR status = 'done');

-- Human review requested by an author: the automated decision is `confirmed` or `overturned`
-- (the refused text then resumes the pipeline as accepted, the masked idea is republished).
ALTER TABLE review_requests
    ADD COLUMN resolution text CHECK (resolution IN ('confirmed', 'overturned')),
    ADD COLUMN note text CHECK (char_length(note) BETWEEN 1 AND 2000),
    ADD CONSTRAINT review_requests_resolution_when_done
        CHECK (resolution IS NULL OR status = 'done');
