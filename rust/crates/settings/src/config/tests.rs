//! Tests for mando-config: settings and basic path utilities.

use super::loader::parse_config;
use super::settings::Config;
use global_infra::ids::slugify;
use global_infra::paths::expand_tilde;
use std::path::Path;

// ---------------------------------------------------------------------------
// settings: full config parse
// ---------------------------------------------------------------------------

#[test]
fn parse_full_config() {
    let json = r#"{
        "workspace": "~/my-workspace",
        "ui": {
            "openAtLogin": false
        },
        "features": {
            "scout": true,
            "setupDismissed": false,
            "claudeCodeVerified": true
        },
        "scout": {
            "interests": {
                "high": [],
                "low": []
            },
            "userContext": {
                "role": "",
                "knownDomains": [],
                "explainDomains": []
            }
        },
        "channels": {
            "telegram": {
                "enabled": true,
                "owner": "bill"
            }
        },
        "gateway": {
            "dashboard": {
                "host": "0.0.0.0",
                "port": 8080
            }
        },
        "captain": {
            "autoSchedule": true,
            "autoMerge": false,
            "tickIntervalS": 60,
            "tz": "UTC",
            "defaultTerminalAgent": "claude",
            "claudeTerminalArgs": "--dangerously-skip-permissions",
            "codexTerminalArgs": "--full-auto",
            "projects": {
                "/code/repo": {
                    "name": "repo",
                    "path": "/code/repo",
                    "githubRepo": "org/repo",
                    "aliases": ["rp", "repo"],
                    "hooks": { "pre_spawn": "echo hi" },
                    "workerPreamble": "be careful"
                }
            }
        },
        "env": {
            "TELEGRAM_MANDO_BOT_TOKEN": "tok-123",
            "SOME_OTHER_KEY": "test-val"
        }
    }"#;

    let mut cfg: Config = serde_json::from_str(json).unwrap();
    cfg.populate_runtime_fields();

    // Root
    assert_eq!(cfg.workspace, "~/my-workspace");

    // Features
    assert!(cfg.features.scout);

    // Channels
    assert!(cfg.channels.telegram.enabled);
    assert_eq!(cfg.channels.telegram.token, "tok-123");
    assert_eq!(cfg.channels.telegram.owner, "bill");
    // Gateway
    assert_eq!(cfg.gateway.dashboard.host, "0.0.0.0");
    assert_eq!(cfg.gateway.dashboard.port, 8080);

    // Captain (projects are serde(skip) -- loaded from DB, not config.json)
    assert!(cfg.captain.auto_schedule);
    assert_eq!(cfg.captain.tick_interval_s, 60);
    assert!(cfg.captain.projects.is_empty());
    assert!(cfg.captain.task_db_path.ends_with("mando.db"));
    assert!(cfg.captain.lockfile_path.ends_with("captain.lock"));
    assert!(cfg
        .captain
        .worker_health_path
        .ends_with("worker-health.json"));

    // Env
    assert_eq!(cfg.env.get("TELEGRAM_MANDO_BOT_TOKEN").unwrap(), "tok-123");
}

// ---------------------------------------------------------------------------
// settings: raw serde still rejects partial input (documents strictness
// contract — `parse_config` is the forgiving entry point)
// ---------------------------------------------------------------------------

#[test]
fn raw_serde_rejects_missing_fields() {
    let json = "{}";
    assert!(serde_json::from_str::<Config>(json).is_err());
}

// ---------------------------------------------------------------------------
// parse_config: partial configs inherit defaults for missing fields
// ---------------------------------------------------------------------------

#[test]
fn parse_config_empty_object_yields_defaults() {
    let cfg = parse_config("{}", Path::new("test.json")).expect("empty {} should parse");
    let defaults = Config::default();
    assert_eq!(cfg.workspace, defaults.workspace);
    assert_eq!(cfg.gateway.dashboard.port, defaults.gateway.dashboard.port);
    assert_eq!(
        cfg.captain.tick_interval_s,
        defaults.captain.tick_interval_s
    );
    assert!(cfg.captain.task_db_path.ends_with("mando.db"));
}

#[test]
fn parse_config_partial_overrides_selected_fields() {
    let json = r#"{
        "workspace": "~/custom-ws",
        "gateway": { "dashboard": { "port": 9999 } },
        "captain": { "tickIntervalS": 7 }
    }"#;
    let cfg = parse_config(json, Path::new("test.json")).expect("partial should parse");
    // Overridden
    assert_eq!(cfg.workspace, "~/custom-ws");
    assert_eq!(cfg.gateway.dashboard.port, 9999);
    assert_eq!(cfg.captain.tick_interval_s, 7);
    // Sibling fields keep defaults
    let defaults = Config::default();
    assert_eq!(cfg.gateway.dashboard.host, defaults.gateway.dashboard.host);
    assert_eq!(
        cfg.captain.default_terminal_agent,
        defaults.captain.default_terminal_agent
    );
    // Runtime-only paths still populate
    assert!(cfg.captain.task_db_path.ends_with("mando.db"));
}

#[test]
fn parse_config_unknown_keys_in_top_level_ignored() {
    // Fields not present on Config silently drop (no deny_unknown_fields on
    // settings structs). Kept loose on purpose so old fields from prior
    // versions don't break boot.
    let json = r#"{ "legacyField": 42 }"#;
    let cfg = parse_config(json, Path::new("test.json"));
    assert!(
        cfg.is_ok(),
        "unknown top-level key should not error: {:?}",
        cfg.err()
    );
}

// ---------------------------------------------------------------------------
// settings: roundtrip serialize/deserialize
// ---------------------------------------------------------------------------

#[test]
fn config_roundtrip() {
    let cfg = Config::default();
    let json = serde_json::to_string(&cfg).unwrap();
    let cfg2: Config = serde_json::from_str(&json).unwrap();
    assert_eq!(cfg.workspace, cfg2.workspace);
    assert_eq!(cfg.gateway.dashboard.port, cfg2.gateway.dashboard.port);
    assert_eq!(cfg.captain.tick_interval_s, cfg2.captain.tick_interval_s);
}

#[test]
fn token_not_serialized() {
    let mut cfg = Config::default();
    cfg.channels.telegram.token = "secret".into();
    let json = serde_json::to_string(&cfg).unwrap();
    assert!(!json.contains("secret"), "token must not appear in JSON");
}

// ---------------------------------------------------------------------------
// paths: expand_tilde
// ---------------------------------------------------------------------------

#[test]
fn expand_tilde_works() {
    let home = std::env::var("HOME").unwrap();
    let expanded = expand_tilde("~/.mando");
    assert_eq!(expanded.to_str().unwrap(), format!("{home}/.mando"));

    let abs = expand_tilde("/tmp/foo");
    assert_eq!(abs.to_str().unwrap(), "/tmp/foo");

    let just = expand_tilde("~");
    assert_eq!(just.to_str().unwrap(), home);
}

// ---------------------------------------------------------------------------
// paths: slugify
// ---------------------------------------------------------------------------

#[test]
fn slugify_basic() {
    assert_eq!(slugify("Hello World!", 20), "hello-world");
    assert_eq!(slugify("  Multiple   Spaces  ", 30), "multiple-spaces");
    assert_eq!(slugify("UPPER_case-Mixed", 50), "upper-case-mixed");
}

#[test]
fn slugify_truncates() {
    assert_eq!(slugify("a-very-long-title-here", 10), "a-very-lon");
    assert_eq!(slugify("hello world again", 6), "hello");
}

#[test]
fn slugify_empty() {
    assert_eq!(slugify("", 10), "");
    assert_eq!(slugify("!!!", 10), "");
}

// ---------------------------------------------------------------------------
// paths: data_dir and friends
// ---------------------------------------------------------------------------

#[test]
fn path_constants_under_data_dir() {
    let dd = global_infra::paths::data_dir();
    assert!(dd.to_str().unwrap().contains(".mando"));

    let sd = global_infra::paths::state_dir();
    assert!(sd.starts_with(&dd));
    assert!(sd.ends_with("state"));

    let ld = global_infra::paths::logs_dir();
    assert!(ld.starts_with(&dd));
}
