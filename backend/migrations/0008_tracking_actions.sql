-- Author actions of the tracking page (AGENTS.md §7.1).

-- Deletion by the author: the text and its related rows are erased at once, the token stays
-- 30 days as a tombstone (`deleted_at`, `expires_at`) so that the tracking page can confirm the
-- deletion, then the daily purge removes it.
ALTER TABLE tracking_tokens ADD COLUMN deleted_at timestamptz;

-- Human review requested by the author of a text refused or masked automatically (DSA art. 17
-- and 20 « light »), queued for the administrator. One request per text. The optional message
-- is masked like every visitor text (§7.5) and erased with the text.
CREATE TABLE review_requests (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    target_kind text NOT NULL CHECK (target_kind IN ('idea', 'contribution', 'report')),
    target_id uuid NOT NULL,
    message text CHECK (char_length(message) BETWEEN 1 AND 2000),
    status text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'done')),
    created_at timestamptz NOT NULL DEFAULT now(),
    handled_at timestamptz,

    CONSTRAINT review_requests_target_key UNIQUE (target_kind, target_id),
    CONSTRAINT review_requests_handled CHECK ((status = 'done') = (handled_at IS NOT NULL))
);

CREATE INDEX review_requests_pending_idx ON review_requests (created_at) WHERE status = 'pending';
CREATE INDEX review_requests_handled_idx ON review_requests (handled_at) WHERE status = 'done';
