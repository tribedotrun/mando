//! Tests for mando-config: settings and basic path utilities.

use crate::paths::{expand_tilde, slugify};
use crate::settings::Config;

// ---------------------------------------------------------------------------
// settings: full config parse
// ---------------------------------------------------------------------------

#[test]
fn parse_full_config() {
    let json = r#"{
        "workspace": "~/my-workspace",
        "features": {
            "scout": true
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
            "tickIntervalS": 60,
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

    // Env
    assert_eq!(cfg.env.get("TELEGRAM_MANDO_BOT_TOKEN").unwrap(), "tok-123");
}

// ---------------------------------------------------------------------------
// settings: defaults on minimal config
// ---------------------------------------------------------------------------

#[test]
fn parse_minimal_config_uses_defaults() {
    let json = "{}";
    let cfg: Config = serde_json::from_str(json).unwrap();

    assert_eq!(cfg.workspace, "~/.mando/workspace");
    assert!(!cfg.features.scout);
    assert!(!cfg.channels.telegram.enabled);
    assert_eq!(cfg.channels.telegram.token, "");
    assert_eq!(cfg.gateway.dashboard.port, 18791);
    assert!(!cfg.captain.auto_schedule);
    assert_eq!(cfg.captain.tick_interval_s, 30);
    assert!(cfg.env.is_empty());
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
    let dd = crate::paths::data_dir();
    assert!(dd.to_str().unwrap().contains(".mando"));

    let sd = crate::paths::state_dir();
    assert!(sd.starts_with(&dd));
    assert!(sd.ends_with("state"));

    let ld = crate::paths::logs_dir();
    assert!(ld.starts_with(&dd));

    let cl = crate::paths::captain_lock_path();
    assert!(cl.ends_with("captain.lock"));
}
