-- Escalation of serious content found by a report review (AGENTS.md §5.6, §7.5): the review
-- flags `escalate` (defamation, threat, hatred, terrorism, child abuse), the idea is hidden at
-- once and queued for a human, who forwards it to PHAROS by hand (target: 24 hours).

ALTER TABLE report_reviews
    ADD COLUMN escalate boolean NOT NULL DEFAULT false,
    ADD CONSTRAINT report_reviews_escalate_hides CHECK (NOT escalate OR outcome = 'hide');

-- Admin queue « escalade »: at most one pending entry per idea. Internal identifiers only.
CREATE TABLE escalations (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    idea_id uuid NOT NULL REFERENCES ideas (id) ON DELETE CASCADE,
    -- Review that asked for the escalation.
    review_id uuid REFERENCES report_reviews (id) ON DELETE SET NULL,
    status text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'done')),
    created_at timestamptz NOT NULL DEFAULT now(),
    handled_at timestamptz,

    CONSTRAINT escalations_handled CHECK ((status = 'done') = (handled_at IS NOT NULL))
);

CREATE UNIQUE INDEX escalations_pending_idea_idx ON escalations (idea_id) WHERE status = 'pending';
CREATE INDEX escalations_pending_idx ON escalations (created_at) WHERE status = 'pending';
