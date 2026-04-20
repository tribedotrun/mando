ALTER TABLE cc_sessions ADD COLUMN rev INTEGER NOT NULL DEFAULT 1;
ALTER TABLE scout_research_runs ADD COLUMN rev INTEGER NOT NULL DEFAULT 1;

CREATE TABLE IF NOT EXISTS lifecycle_transitions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    aggregate_type TEXT NOT NULL,
    aggregate_id TEXT NOT NULL,
    command TEXT NOT NULL,
    from_state TEXT,
    to_state TEXT NOT NULL,
    actor TEXT NOT NULL,
    cause TEXT,
    metadata TEXT NOT NULL DEFAULT '{}',
    rev_before INTEGER NOT NULL,
    rev_after INTEGER NOT NULL,
    idempotency_key TEXT,
    occurred_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_lifecycle_transitions_aggregate
    ON lifecycle_transitions(aggregate_type, aggregate_id, id DESC);
CREATE INDEX IF NOT EXISTS idx_lifecycle_transitions_occurred_at
    ON lifecycle_transitions(occurred_at DESC);
CREATE UNIQUE INDEX IF NOT EXISTS idx_lifecycle_transitions_idempotency
    ON lifecycle_transitions(idempotency_key)
    WHERE idempotency_key IS NOT NULL;

CREATE TABLE IF NOT EXISTS lifecycle_outbox (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    transition_id INTEGER NOT NULL REFERENCES lifecycle_transitions(id) ON DELETE CASCADE,
    aggregate_type TEXT NOT NULL,
    aggregate_id TEXT NOT NULL,
    effect_kind TEXT NOT NULL,
    payload TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    processed_at TEXT,
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT
);

CREATE INDEX IF NOT EXISTS idx_lifecycle_outbox_pending
    ON lifecycle_outbox(processed_at, id);
CREATE INDEX IF NOT EXISTS idx_lifecycle_outbox_transition
    ON lifecycle_outbox(transition_id, id);
