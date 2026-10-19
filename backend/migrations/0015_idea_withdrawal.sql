-- Withdrawal of a published idea by its author (owner decision D2 of 2026-10-19, AGENTS.md
-- §5.2, §7.1, §7.3): `published | hidden → withdrawn`. The idea, its analysis, contributions,
-- reports and rights of reply leave the public site, the API and the exports, stay visible to the
-- admin (« retirée par son auteur ») and are purged 12 months after the withdrawal
-- (`withdrawn → purged`). Unpublished texts deleted by their author are still erased at once.
--
-- Backward compatible: the previous binary never writes `withdrawn`; the status list only grows
-- (same constraint name, so that the schema test still finds it).
ALTER TABLE ideas DROP CONSTRAINT ideas_status_check;
ALTER TABLE ideas ADD CONSTRAINT ideas_status_check CHECK (status IN (
    'submitted', 'screening', 'second_opinion', 'rejected', 'purged', 'accepted', 'dedup',
    'merged', 'queued_analysis', 'analyzing', 'analysis_failed', 'published', 'hidden',
    'withdrawn'
));

ALTER TABLE ideas
    -- When the author withdrew it, and from which status (kept after the purge: a withdrawn
    -- idea is never counted again in the public statistics).
    ADD COLUMN withdrawn_at timestamptz,
    ADD COLUMN withdrawn_from text CHECK (withdrawn_from IN ('published', 'hidden')),
    ADD CONSTRAINT ideas_withdrawal_complete CHECK ((withdrawn_at IS NULL) = (withdrawn_from IS NULL)),
    ADD CONSTRAINT ideas_withdrawn_dated CHECK (status <> 'withdrawn' OR withdrawn_at IS NOT NULL);

-- Daily purge of the withdrawals older than 12 months.
CREATE INDEX ideas_withdrawn_idx ON ideas (withdrawn_at) WHERE status = 'withdrawn';
