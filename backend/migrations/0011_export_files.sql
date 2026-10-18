-- Open data exports (AGENTS.md §7.4): each generation stores, besides `ideas.json` and
-- `ideas.csv`, the `LICENSE` file (Licence Ouverte 2.0) and the metadata file (`metadata.json`)
-- served next to them. The files of one generation share `generation_id` and `generated_at`.
-- Older binaries only read the `json` and `csv` rows: the wider CHECK stays compatible.

ALTER TABLE exports DROP CONSTRAINT exports_format_check;
ALTER TABLE exports ADD CONSTRAINT exports_format_check
    CHECK (format IN ('json', 'csv', 'license', 'metadata'));

ALTER TABLE exports
    ADD COLUMN generation_id uuid,
    -- Version of the layout of the export files (`EXPORT_SCHEMA_VERSION`).
    ADD COLUMN export_schema_version text;

CREATE INDEX exports_generation_idx ON exports (generation_id);
