-- Mistral models that refuse `reasoning_effort` (AGENTS.md §6.1): Mistral documents the parameter
-- for some models only, and a model that does not take it answers HTTP 400. The client then sends
-- the same call again without it and remembers the model in memory; this row lets the api show the
-- alert `mistral_reasoning_unsupported` on the admin dashboard and the effective effort on the
-- methodology page. Deleted by the first call that the model accepts with the parameter (after a
-- restart, the worker tries it again once). Additive: older binaries ignore the table.
CREATE TABLE mistral_reasoning_refusals (
    model text PRIMARY KEY,
    -- First refusal, and the last one with its HTTP status (400, or 422 for a validation error).
    since timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    last_http_status integer NOT NULL,
    refusals bigint NOT NULL DEFAULT 1
);
