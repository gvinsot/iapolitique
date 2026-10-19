-- Reasoning effort sent with a Mistral call (AGENTS.md §6.1, §6.4): `high` or `none`; NULL when
-- the parameter was omitted (Mistral's default), for the decision service and the embeddings,
-- and for the calls recorded before this migration. Additive and nullable: older binaries
-- ignore the column.
ALTER TABLE ai_calls ADD COLUMN reasoning_effort text
    CHECK (reasoning_effort IN ('high', 'none'));
