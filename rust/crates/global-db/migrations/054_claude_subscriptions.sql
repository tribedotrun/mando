CREATE TABLE claude_subscriptions (
    credential_id INTEGER PRIMARY KEY REFERENCES credentials(id) ON DELETE CASCADE,
    account_uuid TEXT NOT NULL,
    plan_name TEXT,
    billing_interval TEXT,
    subscription_status TEXT,
    renews_at INTEGER,
    ends_at INTEGER,
    ends_before TEXT,
    next_charge_date TEXT,
    checked_at INTEGER,
    attempted_at INTEGER NOT NULL,
    error TEXT,
    keychain_access_required INTEGER NOT NULL DEFAULT 0
);
