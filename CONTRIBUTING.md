# Contributing to Mando

The best way to contribute is by opening an issue — bug reports, feature requests, and questions are all welcome.

## Building from source

### Prerequisites

- **Rust** -- version pinned in `rust-toolchain.toml`
- **Node.js** -- version pinned in `.node-version`
- **cargo-nextest** -- `cargo install cargo-nextest --locked`
- **macOS** -- Mando is a macOS application

### Setup

```bash
git clone https://github.com/tribedotrun/mando.git
cd mando
cd electron && npm install && cd ..
```

### Build and run

```bash
cargo build --workspace                  # Build all Rust crates
cd electron && npm run build && cd ..    # Build Electron app
cargo nextest run --workspace --lib      # Run unit tests
```

## License

Mando is licensed under the [Apache License 2.0](LICENSE).
