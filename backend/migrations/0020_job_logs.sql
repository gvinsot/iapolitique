-- Logs of each job, readable from the admin (AGENTS.md §8): the lines the worker writes to
-- stdout while it runs a job (same content as the container logs: internal identifiers, codes
-- and counts, never a visitor text, an IP or a token, §14.6). Deleted with their job, and 30
-- days after they were written by the daily purge (§7.3). Additive: older binaries ignore it.
CREATE TABLE job_logs (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    job_id bigint NOT NULL REFERENCES jobs (id) ON DELETE CASCADE,
    logged_at timestamptz NOT NULL,
    level text NOT NULL CHECK (level IN ('error', 'warn', 'info', 'debug', 'trace')),
    target text NOT NULL,
    message text NOT NULL,
    fields jsonb NOT NULL DEFAULT '{}' CHECK (jsonb_typeof(fields) = 'object')
);

CREATE INDEX job_logs_job_id_idx ON job_logs (job_id, id);
CREATE INDEX job_logs_logged_at_idx ON job_logs (logged_at);
