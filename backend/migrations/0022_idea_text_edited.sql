-- Text of a refused idea corrected by the administrator before accepting it (AGENTS.md §5.2,
-- §8): the idea page and the exports then say « texte modifié par l'éditeur du site ».
-- Additive and nullable: older binaries ignore the column.
ALTER TABLE ideas ADD COLUMN text_edited_at timestamptz;
