# Mando

CTO for your AI coding agents.

Native macOS desktop app built with Electron + Rust. Standalone `mando-gw` daemon managed by launchd; Electron and CLI are thin HTTP/SSE clients. Manages a task list, spawns Claude Code workers, reviews their work, and merges PRs — all autonomously.

![Mando](https://storage.googleapis.com/hypertribe-public/mando/readme-hero.png)

## Architecture

```
mando-gw daemon (launchd / --foreground)
    ├── axum HTTP API
    ├── SSE /api/events (live updates)
    ├── captain auto-tick (worker orchestration)
    ├── Telegram bots
    └── cron service
        ▲ HTTP/SSE ▲
    ┌───┴──────────┴───┐
    │   Electron app   │  thin client (no Rust in-process)
    │   CLI (mando)    │  pure HTTP client
    │   Telegram bots  │  in-daemon
    └──────────────────┘

Rust crates:
    ├── mando-gateway    ← daemon binary + axum server
    ├── mando-captain    ← tick engine, workers, clarifier
    ├── mando-telegram   ← bot commands
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

Apache 2.0 — see [LICENSE](LICENSE).

<!-- hypertribe:sponsors:start -->
## Sponsors

[![mando Sponsors](https://api.tribe.run/solana/dex/tokens/by-mint/tLSdmcjM9dXdm6ZZZmBwJPFtvJA3b8b3fSzvRzq4co5/sponsors.svg)](https://tribe.run/token/tLSdmcjM9dXdm6ZZZmBwJPFtvJA3b8b3fSzvRzq4co5)

Become a sponsor on [tribe.run](https://tribe.run/token/tLSdmcjM9dXdm6ZZZmBwJPFtvJA3b8b3fSzvRzq4co5).
<!-- hypertribe:sponsors:end -->
