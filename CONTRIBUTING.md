# Contributing to Mando

The best way to contribute is by opening an issue — bug reports, feature requests, and questions are all welcome.

## Building from source

### Prerequisites

- **Rust** -- version pinned in `rust/rust-toolchain.toml`
- **Node.js** -- version pinned in `.node-version`
- **macOS** -- Mando is a macOS application

### Setup

```bash
git clone https://github.com/tribedotrun/mando.git
cd mando
cd electron && npm install && cd ..
```

### Build and run

```bash
cargo build --manifest-path rust/Cargo.toml --workspace                  # Build all Rust crates
cd electron && npm run build && cd ..    # Build Electron app
```

Verify product changes in a running sandbox or local app. Unit and mock
integration tests require a documented strong reason; keep formatting, lint,
type, architecture, and contract checks.

## License

Mando is licensed under the [Apache License 2.0](LICENSE).
