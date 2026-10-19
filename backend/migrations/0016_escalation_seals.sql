-- Sealed copy of escalated content (owner decision D3 of 2026-10-19, AGENTS.md §7.3, §7.5).
--
-- When a report review escalates an idea (serious content: defamation, threat, hatred,
-- terrorism, child abuse), a copy of what was escalated is sealed: the masked original text and
-- what was published from it, the analysis, the review and the reports that raised it (reason
-- and masked details, never the reporter's contact). It is kept one year from the escalation,
-- whatever happens to the idea meanwhile (withdrawal or deletion by its author, 12-month purge),
-- then purged. Legal basis: GDPR art. 17(3)(e) (establishment, exercise or defence of legal
-- claims) and the LCEN obligations. Admin only: read through the admin API, every read logged in
-- `admin_actions`; never public, never exported.
--
-- No foreign key on purpose: the copy outlives the idea and the escalation rows.
CREATE TABLE escalation_seals (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    escalation_id uuid NOT NULL UNIQUE,
    idea_id uuid NOT NULL,
    escalated_at timestamptz NOT NULL,
    -- escalated_at + 1 year: the daily purge deletes the copy after this instant.
    purge_after timestamptz NOT NULL,
    content jsonb NOT NULL CHECK (jsonb_typeof(content) = 'object'),
    created_at timestamptz NOT NULL DEFAULT now(),

    CONSTRAINT escalation_seals_one_year CHECK (purge_after = escalated_at + interval '1 year')
);

CREATE INDEX escalation_seals_purge_idx ON escalation_seals (purge_after);
CREATE INDEX escalation_seals_idea_idx ON escalation_seals (idea_id);

-- Escalations of the last year raised before this migration: sealed from what is left.
INSERT INTO escalation_seals (escalation_id, idea_id, escalated_at, purge_after, content)
SELECT e.id, e.idea_id, e.created_at, e.created_at + interval '1 year',
       jsonb_build_object(
           'backfilled', true,
           'idea', jsonb_build_object(
               'status', i.status, 'slug', i.slug, 'title', i.title, 'summary', i.summary,
               'original_text', i.original_text, 'submitted_territory', i.submitted_territory,
               'submitted_admin_level', i.submitted_admin_level, 'submitted_at', i.created_at,
               'published_at', i.published_at, 'hidden_at', i.hidden_at,
               'hidden_reason', i.hidden_reason),
           'analysis', (SELECT a.content FROM analyses a WHERE a.id = i.current_analysis_id),
           'review', (SELECT jsonb_build_object(
                          'outcome', rr.outcome, 'escalate', rr.escalate,
                          'explanation', rr.explanation, 'model', rr.model,
                          'prompt_version', rr.prompt_version, 'reviewed_at', rr.created_at)
                      FROM report_reviews rr WHERE rr.id = e.review_id),
           'reports', coalesce((SELECT jsonb_agg(jsonb_build_object(
                                    'reason', r.reason, 'details', r.details,
                                    'reported_at', r.created_at) ORDER BY r.created_at, r.id)
                                FROM reports r
                                WHERE e.review_id IS NOT NULL AND r.review_id = e.review_id),
                               '[]'::jsonb))
FROM escalations e JOIN ideas i ON i.id = e.idea_id
WHERE e.created_at > now() - interval '1 year';
