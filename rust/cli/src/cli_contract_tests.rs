use std::fmt::Write as _;

use clap::{ArgMatches, Command, CommandFactory};

use super::Cli;

const PARSE_CASES: &[(&[&str], &str)] = &[
    (&["mando", "scout", "simplelist"], "scout simplelist"),
    (
        &["mando", "scout", "simplelist", "--status", "pending"],
        "scout simplelist",
    ),
    (
        &[
            "mando",
            "scout",
            "add",
            "https://example.com",
            "-t",
            "Example",
        ],
        "scout add",
    ),
    (
        &["mando", "scout", "bulk-delete", "7", "8"],
        "scout bulk-delete",
    ),
    (
        &[
            "mando",
            "scout",
            "ask",
            "42",
            "--session",
            "sess-1",
            "What",
            "changed?",
        ],
        "scout ask",
    ),
    (
        &[
            "mando", "scout", "act", "42", "sandbox", "Focus", "on", "tests",
        ],
        "scout act",
    ),
    (&["mando", "scout", "sessions", "9"], "scout sessions"),
    (&["mando", "codex", "app-use", "PT"], "codex app-use"),
    (&["mando", "codex", "app-restore"], "codex app-restore"),
    (&["mando", "codex", "app-status"], "codex app-status"),
    (
        &["mando", "codex", "app-status", "--json"],
        "codex app-status",
    ),
    (&["mando", "captain", "tick", "--dry-run"], "captain tick"),
    (
        &["mando", "captain", "workers", "-w", "-n", "2"],
        "captain workers",
    ),
    (
        &["mando", "captain", "merge", "123", "-p", "mando"],
        "captain merge",
    ),
    (&["mando", "captain", "triage"], "captain triage"),
    (&["mando", "captain", "retry", "42"], "captain retry"),
    (&["mando", "captain", "triage", "ENG-123"], "captain triage"),
    (
        &[
            "mando",
            "captain",
            "adopt",
            "Finish branch",
            "-w",
            "/tmp/worktree",
            "-n",
            "Carry on",
            "-p",
            "sandbox",
        ],
        "captain adopt",
    ),
    (&["mando", "captain", "handoff", "42"], "captain handoff"),
    (&["mando", "captain", "accept", "42"], "captain accept"),
    (
        &["mando", "credentials", "disable", "7"],
        "credentials disable",
    ),
    (
        &["mando", "credentials", "enable", "7"],
        "credentials enable",
    ),
    (&["mando", "daemon", "start"], "daemon start"),
    (&["mando", "daemon", "start", "-p", "9999"], "daemon start"),
    (&["mando", "daemon", "start", "-v"], "daemon start"),
    (&["mando", "daemon", "stop"], "daemon stop"),
    (&["mando", "daemon", "health"], "daemon health"),
    (&["mando", "daemon", "logs", "-n", "100"], "daemon logs"),
    (&["mando", "daemon", "logs", "-f"], "daemon logs"),
    (&["mando", "sessions"], "sessions"),
    (&["mando", "sessions", "--last", "10"], "sessions"),
    (
        &[
            "mando",
            "sessions",
            "--task",
            "42",
            "--caller",
            "captain-review",
        ],
        "sessions",
    ),
    (
        &["mando", "sessions", "transcript", "sess-1"],
        "sessions transcript",
    ),
    (
        &[
            "mando",
            "sessions",
            "stream",
            "sess-1",
            "--type",
            "user",
            "--type",
            "assistant",
        ],
        "sessions stream",
    ),
    (&["mando", "todo", "add", "Fix bug"], "todo add"),
    (
        &["mando", "todo", "add", "Fix bug", "-p", "mando"],
        "todo add",
    ),
    (
        &[
            "mando",
            "todo",
            "add",
            "Ship planned task",
            "--plan",
            "~/.mando/plans/42/brief.md",
            "--no-pr",
        ],
        "todo add",
    ),
    (&["mando", "todo", "list", "--all"], "todo list"),
    (&["mando", "todo", "delete", "42"], "todo delete"),
    (&["mando", "todo", "show", "14"], "todo show"),
    (
        &["mando", "todo", "timeline", "5", "--last", "3"],
        "todo timeline",
    ),
    (&["mando", "todo", "list"], "todo list"),
    (&["mando", "merge", "123", "-p", "mando"], "merge"),
    (&["mando", "worktree", "list"], "worktree list"),
    (&["mando", "scout", "list"], "scout list"),
    (&["mando", "channels"], "channels"),
    (&["mando", "tasks"], "tasks"),
    (&["mando", "health"], "health"),
    (&["mando", "credentials", "list"], "credentials list"),
    (&["mando", "credentials", "pick"], "credentials pick"),
    (
        &["mando", "worktree", "open", "my-feature"],
        "worktree open",
    ),
    (
        &["mando", "worktree", "open", "fix", "-p", "mando"],
        "worktree open",
    ),
    (&["mando", "worktree", "prune"], "worktree prune"),
    (
        &["mando", "worktree", "remove", "/tmp/wt"],
        "worktree remove",
    ),
    (&["mando", "worktree", "cleanup"], "worktree cleanup"),
    (
        &["mando", "worktree", "cleanup", "--dry-run"],
        "worktree cleanup",
    ),
];

#[test]
fn parser_smoke_cases_match_expected_commands() {
    for (argv, expected_command) in PARSE_CASES {
        let matches = Cli::command()
            .try_get_matches_from(argv.iter().copied())
            .unwrap_or_else(|error| panic!("failed to parse {argv:?}: {error}"));
        assert_eq!(command_path(&matches), *expected_command, "argv: {argv:?}");
    }
}

#[test]
fn sessions_last_rejects_zero() {
    let error = Cli::command()
        .try_get_matches_from(["mando", "sessions", "--last", "0"])
        .expect_err("--last 0 must be rejected");
    assert_eq!(error.kind(), clap::error::ErrorKind::ValueValidation);
}

fn command_path(matches: &ArgMatches) -> String {
    let mut names = Vec::new();
    let mut current = matches;
    while let Some((name, subcommand)) = current.subcommand() {
        names.push(name);
        current = subcommand;
    }
    names.join(" ")
}

#[test]
fn clap_inventory_matches_generated_dump() {
    let generated = render_inventory();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/cli-inventory.txt");
    if std::env::var_os("UPDATE_CLI_INVENTORY").is_some() {
        std::fs::write(path, generated).expect("write CLI inventory");
        return;
    }

    assert_eq!(
        include_str!("../cli-inventory.txt"),
        generated,
        "CLI inventory drifted; rerun with UPDATE_CLI_INVENTORY=1"
    );
}

fn render_inventory() -> String {
    let root = Cli::command();
    let mut output = String::new();
    let mut path = vec![root.get_name().to_string()];
    render_command(&root, &mut path, &mut output);
    output
}

fn render_command(command: &Command, path: &mut Vec<String>, output: &mut String) {
    let mut flags = command
        .get_arguments()
        .filter(|argument| !matches!(argument.get_id().as_str(), "help" | "version"))
        .filter_map(|argument| {
            let mut names = Vec::new();
            if let Some(short) = argument.get_short() {
                names.push(format!("-{short}"));
            }
            if let Some(long) = argument.get_long() {
                names.push(format!("--{long}"));
            }
            (!names.is_empty()).then(|| names.join("/"))
        })
        .collect::<Vec<_>>();
    flags.sort();
    writeln!(output, "{}\t{}", path.join(" "), flags.join(",")).expect("write inventory");

    let mut subcommands = command.get_subcommands().collect::<Vec<_>>();
    subcommands.sort_by_key(|subcommand| subcommand.get_name());
    for subcommand in subcommands {
        path.push(subcommand.get_name().to_string());
        render_command(subcommand, path, output);
        path.pop();
    }
}
