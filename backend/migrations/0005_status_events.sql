-- Timestamped history of the status changes of contributions and reports, like `idea_events`
-- for ideas (AGENTS.md §5.2: every transition is recorded). Written by `db::status` in the
-- transaction that changes the status. Internal identifiers only, never user text.

CREATE TABLE contribution_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    contribution_id uuid NOT NULL REFERENCES contributions (id) ON DELETE CASCADE,
    -- NULL for the creation event.
    from_status text,
    to_status text NOT NULL,
    actor text NOT NULL CHECK (actor IN ('author', 'api', 'worker', 'admin')),
    reason_code text,
    details jsonb NOT NULL DEFAULT '{}' CHECK (jsonb_typeof(details) = 'object'),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX contribution_events_contribution_idx
    ON contribution_events (contribution_id, created_at, id);

CREATE TABLE report_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    report_id uuid NOT NULL REFERENCES reports (id) ON DELETE CASCADE,
    -- NULL for the creation event.
    from_status text,
    to_status text NOT NULL,
    actor text NOT NULL CHECK (actor IN ('author', 'api', 'worker', 'admin')),
    reason_code text,
    details jsonb NOT NULL DEFAULT '{}' CHECK (jsonb_typeof(details) = 'object'),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX report_events_report_idx ON report_events (report_id, created_at, id);
