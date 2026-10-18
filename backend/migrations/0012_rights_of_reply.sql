-- Right of reply (LCEN art. 1-1 III, AGENTS.md §7.5): a person named or designated by a
-- published idea asks the publisher to insert a reply; the administrator publishes it under the
-- idea (within 3 days of the request). Written by the administrator, shown as plain text on the
-- idea page. Erased with the idea (deletion by its author, purge 12 months after a masking).

CREATE TABLE rights_of_reply (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    idea_id uuid NOT NULL REFERENCES ideas (id) ON DELETE CASCADE,
    -- How the person replying is designated, as they asked (« La direction de … »).
    author_label text NOT NULL CHECK (char_length(author_label) BETWEEN 1 AND 200),
    body text NOT NULL CHECK (char_length(body) BETWEEN 1 AND 8000),
    -- Day the request was received: the legal 3-day delay runs from it.
    requested_on date NOT NULL,
    published_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TRIGGER rights_of_reply_set_updated_at BEFORE UPDATE ON rights_of_reply
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
CREATE INDEX rights_of_reply_idea_idx ON rights_of_reply (idea_id, published_at, id);
