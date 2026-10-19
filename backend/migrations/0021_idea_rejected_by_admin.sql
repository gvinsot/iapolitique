-- Refusal decided by the administrator (AGENTS.md §8), not by the screening: the statement of
-- reasons then says the decision was human (DSA art. 17). Any accepted idea, published or not,
-- may be refused this way; it is purged 30 days later like any refused idea (§7.3).
-- Additive with a default: older binaries ignore the column.
ALTER TABLE ideas ADD COLUMN rejected_by_admin boolean NOT NULL DEFAULT false;
