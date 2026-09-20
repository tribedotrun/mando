ALTER TABLE credentials ADD COLUMN cli_eligible INTEGER NOT NULL DEFAULT 1 CHECK (cli_eligible IN (0, 1));
