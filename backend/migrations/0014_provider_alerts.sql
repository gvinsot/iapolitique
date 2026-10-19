-- Configuration alerts of the AI providers (AGENTS.md §6.4, §8, §13.1): today only
-- `mistral_auth`, the Mistral API key refused (HTTP 401 or 403). One row while the alert lasts,
-- deleted by the first successful Mistral call. Additive: older binaries ignore the table.
CREATE TABLE provider_alerts (
    alert text PRIMARY KEY CHECK (alert IN ('mistral_auth')),
    -- First refusal of the current alert.
    since timestamptz NOT NULL DEFAULT now(),
    -- Last refusal, and its HTTP status.
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    last_http_status integer,
    failures bigint NOT NULL DEFAULT 1,
    -- Last time a job was allowed to call the provider to see whether the alert is over: the
    -- other jobs wait without calling it.
    probed_at timestamptz NOT NULL DEFAULT now(),
    -- Last ERROR log of the alert: one per period, never one per job.
    logged_at timestamptz NOT NULL DEFAULT now()
);
