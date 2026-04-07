# Mando

CTO for your AI coding agents.

Autonomously works through your backlog: assigns tasks to Claude Code agents, reviews their PRs, nudges them when they go off track, and merges when the work is ready.

![Mando](https://storage.googleapis.com/hypertribe-public/mando/readme-hero.png)

## Key Features

### Captain: Autonomous Agent Orchestration

Captain is the tick-based engine that runs your AI coding agents end-to-end. It assigns tasks to Claude Code workers, monitors their progress, nudges stalled sessions, and intervenes when things go off track. No babysitting required.

### AI Code Review

Every PR goes through Captain's review pipeline before it can merge. It reads the diff, evaluates correctness and style, and either approves or escalates to you with a CTO-level report explaining exactly what needs attention.

### Multi-Surface Control

Manage everything from the native macOS desktop app, the `mando` CLI, or Telegram. All three are thin clients over the same HTTP/SSE API, so you get the same data and actions everywhere.

## Architecture

```
mando-gw daemon (launchd / --foreground)
    ├── axum HTTP API
    ├── SSE /api/events (live updates)
    ├── captain auto-tick (worker orchestration)
    ├── Telegram runtime
    └── cron service
        ▲ HTTP/SSE ▲
    ┌──────────┴──────────┐
    │   Electron app      │  thin client (no Rust in-process)
    │   CLI (mando)       │  pure HTTP client
    │   Telegram runtime  │  owned by daemon
    └─────────────────────┘

Rust crates:
    ├── mando-gateway    ← daemon binary + axum server
    ├── mando-captain    ← tick engine, workers, clarifier
    ├── mando-telegram   ← Telegram runtime library
    ├── mando-config     ← Config struct (serde)
    ├── mando-types      ← TaskItem, ItemStatus, etc.
    ├── mando-shared     ← cron, event bus, helpers
    ├── mando-scout      ← scout queue + AI triage
    ├── mando-readability← HTML article extraction
    └── mando-uuid       ← v4 UUID
CLI (cli/)              ← `mando` binary (HTTP client)
Electron (electron/)    ← Mando.app (HTTP/SSE client)
```

## Building from source

Requires Rust (see `rust-toolchain.toml`), Node.js (see `.node-version`), and **`cargo-nextest`** (`cargo install cargo-nextest --locked`).

```bash
cargo build --workspace          # Build all Rust crates
cd electron && npm ci && npm run build   # Build Electron app
cargo nextest run --workspace --lib      # Run unit tests
```

## License

Apache 2.0, see [LICENSE](LICENSE).
