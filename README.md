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
mando-gw daemon (Rust, launchd / --foreground)
    ├── axum API on 127.0.0.1:{port}
    │   ├── JSON / multipart / static routes
    │   ├── NDJSON terminal streams
    │   └── SSE /api/events live updates
    ├── typed API registry
    │   └── api_route! → api-types → generated Electron contracts
    ├── captain runtime
    │   └── auto-tick, workers, review, merge, reopen/rework
    ├── scout runtime
    │   └── content fetch, article extraction, research, triage
    ├── sessions + terminal runtimes
    ├── embedded Telegram transport
    │   └── Bot API polling + local daemon HTTP/SSE
    └── Electron UI supervisor

Clients:
    Electron app (electron/)  ← React/TypeScript HTTP + SSE client
    CLI (rust/cli/)           ← `mando` HTTP client
    Telegram                  ← embedded daemon transport, external Bot API
```

The daemon API is the shared boundary. Rust handlers register routes through
`api_route!`, request/response/event types live in `api-types`, and
`api-types-codegen` generates the TypeScript route map and Zod schemas consumed
by Electron. The Electron renderer keeps daemon-backed state in React Query and
patches that cache from `/api/events`.

Rust workspace layout:

- **Domain crates:** `captain`, `scout`, `sessions`, `sessions-db`, `settings`,
  `terminal`.
- **Global providers:** `global-types`, `global-infra`, `global-db`,
  `global-bus`, `global-claude`, `global-net`.
- **Contracts, transports, and binary:** `api-types`, `api-types-codegen`,
  `gateway-client`, `transport-http`, `transport-http-macros`, `transport-tg`,
  `transport-ui`, `mando-gateway`.
- **Apps:** `rust/cli` builds the `mando` CLI; `electron/` builds `Mando.app`.

## Building from source

Requires Rust (see `rust/rust-toolchain.toml`), Node.js 24 (see
`.node-version`), and npm. The public source build uses direct Cargo and npm
commands. Run each block from the repository root.

Build the Rust workspace:

```bash
cd rust
cargo build --workspace
```

Build the Electron app:

```bash
cd electron
npm ci
npm run build:test
npm run typecheck
```

Optional Rust tests use `cargo-nextest`:

```bash
cargo install cargo-nextest --locked
cd rust
cargo nextest run --workspace --lib
```

Run the app from source after the Rust build:

```bash
cd electron
npm run start
```

Package the macOS app with Electron Forge. The packaged app embeds the release
Rust daemon and CLI binaries; macOS signing and notarization require the local
Apple credentials referenced by the Electron Forge configuration.

```bash
cd rust
cargo build --release --bin mando-gw --no-default-features -p mando-gateway
cargo build --release --bin mando

cd ../electron
npm ci
npm run package
```

## License

Apache 2.0, see [LICENSE](LICENSE).
