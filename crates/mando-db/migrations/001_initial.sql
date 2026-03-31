-- Unified mando.db schema.

-- ── CC Sessions ───────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS cc_sessions (
    session_id      TEXT    PRIMARY KEY,
    created_at      TEXT    NOT NULL,
    caller          TEXT    NOT NULL,
    cwd             TEXT    NOT NULL DEFAULT '',
    model           TEXT    NOT NULL DEFAULT '',
    status          TEXT    NOT NULL DEFAULT 'stopped',
    cost_usd        REAL,
    duration_ms     INTEGER,
    resumed         INTEGER NOT NULL DEFAULT 0,
    turn_count      INTEGER NOT NULL DEFAULT 1,
    task_id         TEXT,
    scout_item_id   INTEGER,
    worker_name     TEXT
);

CREATE INDEX IF NOT EXISTS idx_cc_sessions_caller    ON cc_sessions(caller);
CREATE INDEX IF NOT EXISTS idx_cc_sessions_status    ON cc_sessions(status);
CREATE INDEX IF NOT EXISTS idx_cc_sessions_ts        ON cc_sessions(created_at);
CREATE INDEX IF NOT EXISTS idx_cc_sessions_task_id   ON cc_sessions(task_id);
CREATE INDEX IF NOT EXISTS idx_cc_sessions_scout     ON cc_sessions(scout_item_id);

-- ── Tasks ─────────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS tasks (
    id                         INTEGER PRIMARY KEY AUTOINCREMENT,
    title                      TEXT    NOT NULL,
    status                     TEXT    NOT NULL DEFAULT 'new',
    project                    TEXT,
    worker                     TEXT,
    linear_id                  TEXT,
    resource                   TEXT,
    context                    TEXT,
    original_prompt            TEXT,
    created_at                 TEXT,
    worktree                   TEXT,
    branch                     TEXT,
    pr                         TEXT,
    worker_started_at          TEXT,
    intervention_count         INTEGER NOT NULL DEFAULT 0,
    captain_review_trigger     TEXT,
    session_ids                TEXT    NOT NULL DEFAULT '{}',
    clarifier_questions        TEXT,
    last_activity_at           TEXT,
    plan                       TEXT,
    no_pr                      INTEGER NOT NULL DEFAULT 0,
    reopen_seq                 INTEGER NOT NULL DEFAULT 0,
    reopen_source              TEXT,
    images                     TEXT,
    retry_count                INTEGER NOT NULL DEFAULT 0,
    escalation_report          TEXT,
    source                     TEXT,
    archived_at                TEXT,
    worker_seq                 INTEGER NOT NULL DEFAULT 0,
    github_repo                TEXT
);

CREATE INDEX IF NOT EXISTS idx_tasks_status    ON tasks(status);
CREATE INDEX IF NOT EXISTS idx_tasks_linear_id ON tasks(linear_id);
CREATE INDEX IF NOT EXISTS idx_tasks_worker    ON tasks(worker);
CREATE INDEX IF NOT EXISTS idx_tasks_source    ON tasks(source);

-- ── Task rebase state ───────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS task_rebase_state (
    task_id    INTEGER PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
    worker     TEXT,
    status     TEXT    NOT NULL DEFAULT 'pending',
    retries    INTEGER NOT NULL DEFAULT 0,
    head_sha   TEXT
);

-- ── Timeline events ─────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS timeline_events (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id    INTEGER NOT NULL,
    event_type TEXT    NOT NULL,
    timestamp  TEXT    NOT NULL,
    actor      TEXT    NOT NULL DEFAULT 'captain',
    summary    TEXT    NOT NULL DEFAULT '',
    data       TEXT    NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_timeline_task_ts   ON timeline_events(task_id, timestamp);
CREATE INDEX IF NOT EXISTS idx_timeline_type      ON timeline_events(event_type);

-- ── Ask history ─────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS ask_history (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id    INTEGER NOT NULL,
    role       TEXT    NOT NULL,
    content    TEXT    NOT NULL,
    timestamp  TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_ask_history_task ON ask_history(task_id, timestamp);

-- ── Cron jobs ───────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS cron_jobs (
    id              TEXT PRIMARY KEY,
    name            TEXT    NOT NULL,
    enabled         INTEGER NOT NULL DEFAULT 1,
    schedule_json   TEXT    NOT NULL DEFAULT '{}',
    payload_json    TEXT    NOT NULL DEFAULT '{}',
    state_json      TEXT    NOT NULL DEFAULT '{}',
    created_at_ms   INTEGER NOT NULL DEFAULT 0,
    updated_at_ms   INTEGER NOT NULL DEFAULT 0,
    delete_after_run INTEGER NOT NULL DEFAULT 0,
    job_type        TEXT    NOT NULL DEFAULT 'system',
    cwd             TEXT,
    timeout_s       INTEGER NOT NULL DEFAULT 1200
);

-- ── Linear workpad ──────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS linear_workpad (
    linear_id  TEXT PRIMARY KEY,
    comment_id TEXT NOT NULL
);

-- ── Scout items ───────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS scout_items (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    url            TEXT    UNIQUE NOT NULL,
    type           TEXT    NOT NULL,
    title          TEXT,
    status         TEXT    DEFAULT 'pending',
    relevance      INTEGER,
    quality        INTEGER,
    date_added     TEXT    NOT NULL,
    date_processed TEXT,
    added_by       TEXT,
    error_count    INTEGER DEFAULT 0,
    source_name    TEXT,
    date_published TEXT
);

-- ── Task decisions ────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS task_decisions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    tick_id     TEXT    NOT NULL,
    worker      TEXT    NOT NULL,
    item_id     TEXT,
    action      TEXT    NOT NULL,
    source      TEXT    NOT NULL,
    rule        TEXT    NOT NULL,
    state       TEXT    NOT NULL,
    outcome     TEXT,
    resolved_at TEXT,
    created_at  TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_task_decisions_worker_outcome ON task_decisions(worker, outcome);
CREATE INDEX IF NOT EXISTS idx_task_decisions_action_rule    ON task_decisions(action, rule, outcome);

-- ── Task patterns ─────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS task_patterns (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    pattern        TEXT    NOT NULL,
    signal         TEXT    NOT NULL,
    recommendation TEXT    NOT NULL,
    confidence     REAL    NOT NULL,
    sample_size    INTEGER NOT NULL,
    status         TEXT    NOT NULL DEFAULT 'pending',
    created_at     TEXT    NOT NULL
);

-- ── Voice messages ────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS voice_messages (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id    TEXT    NOT NULL,
    role          TEXT    NOT NULL,
    content       TEXT    NOT NULL,
    action_name   TEXT,
    action_result TEXT,
    created_at    TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_voice_messages_session ON voice_messages(session_id);

-- ── Voice TTS usage ───────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS voice_tts_usage (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id       TEXT,
    timestamp        TEXT    NOT NULL,
    input_chars      INTEGER NOT NULL,
    voice_id         TEXT    NOT NULL,
    model            TEXT    NOT NULL,
    latency_ms       INTEGER NOT NULL,
    audio_duration_ms INTEGER,
    error            TEXT
);
