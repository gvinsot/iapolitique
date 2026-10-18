-- Extensions used by iapolitique (AGENTS.md §4.3).
--
-- `vector` (pgvector) is NOT a trusted extension: on the production cluster the application
-- user cannot create it. A superuser must run `CREATE EXTENSION vector;` in the database
-- beforehand; `IF NOT EXISTS` then makes this migration a no-op for it. The other extensions
-- are trusted and can be created by the database owner.
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE EXTENSION IF NOT EXISTS unaccent;
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- French full text search that ignores accents ("etat" matches "État"). Being a constant
-- `regconfig`, it can be used in generated columns and indexes (`unaccent()` itself is not
-- immutable).
CREATE TEXT SEARCH CONFIGURATION french_unaccent (COPY = pg_catalog.french);
ALTER TEXT SEARCH CONFIGURATION french_unaccent
    ALTER MAPPING FOR hword, hword_part, word WITH unaccent, french_stem;
