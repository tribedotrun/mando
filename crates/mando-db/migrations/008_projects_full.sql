-- Move all project config fields from config.json into the projects table.

ALTER TABLE projects ADD COLUMN aliases         TEXT NOT NULL DEFAULT '[]';
ALTER TABLE projects ADD COLUMN hooks           TEXT NOT NULL DEFAULT '{}';
ALTER TABLE projects ADD COLUMN worker_preamble TEXT NOT NULL DEFAULT '';
ALTER TABLE projects ADD COLUMN check_command   TEXT NOT NULL DEFAULT '';
ALTER TABLE projects ADD COLUMN logo            TEXT;
ALTER TABLE projects ADD COLUMN scout_summary   TEXT NOT NULL DEFAULT '';
ALTER TABLE projects ADD COLUMN classify_rules  TEXT NOT NULL DEFAULT '[]';
ALTER TABLE projects ADD COLUMN created_at      TEXT NOT NULL DEFAULT '';
ALTER TABLE projects ADD COLUMN updated_at      TEXT NOT NULL DEFAULT '';
