-- The `skills` table was created in 0001 but never read or written by any
-- service. Skill inventory + provenance live in the filesystem skill roots and
-- `~/.agents/skills/.skill-lock.json`. Drop the orphan table rather than leave
-- a dead schema.

DROP TABLE IF EXISTS skills;
