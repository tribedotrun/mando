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
            "voice": true,
            "decisionJournal": false,
            "cron": true,
            "linear": true,
            "devMode": false,
            "analytics": true
        },
        "channels": {
            "telegram": {
                "enabled": true,
                "owner": "bill"
            }
        },
        "gateway": {
            "host": "127.0.0.1",
            "port": 9090,
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
            },
            "linearTeam": "XYZ"
        },
        "tools": {
            "ccSelfImprove": {
                "pollIntervalS": 5.0,
                "cooldownS": 600,
                "maxRepairsPerHour": 5,
                "timeoutS": 1800
            }
        },
        "env": {
            "TELEGRAM_MANDO_BOT_TOKEN": "tok-123",
            "ELEVENLABS_API_KEY": "el-key"
        }
    }"#;

    let mut cfg: Config = serde_json::from_str(json).unwrap();
    cfg.populate_runtime_fields();

    // Root
    assert_eq!(cfg.workspace, "~/my-workspace");

    // Features
    assert!(cfg.features.voice);
    assert!(!cfg.features.decision_journal);
    assert!(cfg.features.cron);
    assert!(cfg.features.linear);
    assert!(!cfg.features.dev_mode);
    assert!(cfg.features.analytics);

    // Channels
    assert!(cfg.channels.telegram.enabled);
    assert_eq!(cfg.channels.telegram.token, "tok-123");
    assert_eq!(cfg.channels.telegram.owner, "bill");
    // Gateway
    assert_eq!(cfg.gateway.host, "127.0.0.1");
    assert_eq!(cfg.gateway.port, 9090);
    assert_eq!(cfg.gateway.dashboard.host, "0.0.0.0");
    assert_eq!(cfg.gateway.dashboard.port, 8080);

    // Captain
    assert!(cfg.captain.auto_schedule);
    assert_eq!(cfg.captain.tick_interval_s, 60);
    assert_eq!(cfg.captain.linear_team, "XYZ");
    let project = cfg.captain.projects.get("/code/repo").unwrap();
    assert_eq!(project.name, "repo");
    assert_eq!(project.path, "/code/repo");
    assert_eq!(project.github_repo, Some("org/repo".to_string()));
    assert_eq!(project.aliases, vec!["rp", "repo"]);
    assert_eq!(project.hooks.get("pre_spawn").unwrap(), "echo hi");
    assert_eq!(project.worker_preamble, "be careful");

    // Self-improve
    assert_eq!(cfg.tools.cc_self_improve.poll_interval_s, 5.0);
    assert_eq!(cfg.tools.cc_self_improve.cooldown_s, 600);
    assert_eq!(cfg.tools.cc_self_improve.max_repairs_per_hour, 5);
    assert_eq!(cfg.tools.cc_self_improve.timeout_s, 1800);

    // Env
    assert_eq!(cfg.env.get("ELEVENLABS_API_KEY").unwrap(), "el-key");
}

// ---------------------------------------------------------------------------
// settings: defaults on minimal config
// ---------------------------------------------------------------------------

#[test]
fn parse_minimal_config_uses_defaults() {
    let json = "{}";
    let cfg: Config = serde_json::from_str(json).unwrap();

    assert_eq!(cfg.workspace, "~/.mando/workspace");
    assert!(!cfg.features.voice);
    assert!(!cfg.features.decision_journal);
    assert!(cfg.features.cron);
    assert!(!cfg.features.linear);
    assert!(!cfg.features.dev_mode);
    assert!(!cfg.features.analytics);
    assert!(!cfg.channels.telegram.enabled);
    assert_eq!(cfg.channels.telegram.token, "");
    assert_eq!(cfg.gateway.host, "0.0.0.0");
    assert_eq!(cfg.gateway.port, 18790);
    assert_eq!(cfg.gateway.dashboard.port, 18791);
    assert!(!cfg.captain.auto_schedule);
    assert_eq!(cfg.captain.tick_interval_s, 30);
    assert_eq!(cfg.captain.linear_team, "");
    assert_eq!(cfg.tools.cc_self_improve.cooldown_s, 300);
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
    assert_eq!(cfg.gateway.port, cfg2.gateway.port);
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
