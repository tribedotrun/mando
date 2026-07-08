-- Manual credential disable state. Disabled credentials stay listed and
-- probeable, but picker queries exclude them so neither automatic nor
-- explicit CC/Codex launches can select the credential.
ALTER TABLE credentials ADD COLUMN disabled_at INTEGER;
