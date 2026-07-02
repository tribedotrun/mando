-- Persist whether implementation is routed to the configured GLM worker adapter.
ALTER TABLE tasks ADD COLUMN use_glm_worker INTEGER NOT NULL DEFAULT 0;
