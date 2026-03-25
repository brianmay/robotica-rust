# AGENTS.md — Robotica Rust

Developer and agentic coding agent reference for the `robotica-rust` workspace.

---

## Project Overview

A Tokio-based IoT home automation system built around an actor-like, pipe-driven
message-passing architecture. MQTT is the central message bus. The workspace has 7 crates:

| Crate | Role |
|---|---|
| `robotica-common` | Shared types: MQTT messages, scheduler types, Protobuf |
| `robotica-tokio` | Core library: pipe system, MQTT client, HTTP/WS server, Tesla, LIFX |
| `robotica-backend` | Binary: wires up all devices and automation logic |
| `robotica-frontend` | WASM frontend (Yew) communicating via WebSocket + Protobuf |
| `robotica-slint` | Embedded GUI binary for Raspberry Pi touchscreen (Slint) |
| `robotica-freeswitch` | FreeSWITCH ESL integration for VOIP automation |
| `robotica-macro` | Proc-macros for compile-time time/duration constants |

---

## Build Commands

The primary build system is **Nix** via `flake.nix`. Local development uses Cargo directly.

```bash
# Enter dev shell (sets up DATABASE_URL, CONFIG_FILE, STATIC_PATH, etc.)
nix develop
# or, with direnv:
direnv allow

# Nix builds
nix build .#robotica-backend
nix build .#robotica-frontend
nix build .#robotica-slint
nix build .#robotica-slint-aarch64   # cross-compile ARM

# Cargo builds
cargo build                           # all workspace crates
cargo build -p robotica-backend
cargo build -p robotica-frontend --target wasm32-unknown-unknown

# Run the backend
cargo run -p robotica-backend --bin robotica-backend

# Frontend WASM + wasm-bindgen + webpack
./robotica-frontend/build.sh
# or inside robotica-frontend/:
npm run build
npm run dev
```

---

## Lint Commands

```bash
# Clippy (matches CI — deny all warnings)
cargo clippy -- --deny warnings
cargo clippy -p robotica-backend -- --deny warnings

# Formatting (no custom rustfmt.toml — use defaults)
cargo fmt
cargo fmt -- --check    # CI-style dry run
```

**Clippy lint level applied in every crate** (set via crate-level attributes, not `clippy.toml`):
```rust
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![warn(missing_docs)]
```

Common selective allows used across the codebase:
```rust
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::use_self)]
```

---

## Test Commands

```bash
# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p robotica-tokio
cargo test -p robotica-backend
cargo test -p robotica-common

# Run a single test by name (substring match)
cargo test test_get_next_period
cargo test -p robotica-backend test_estimate_charge_time

# Run a specific integration test file and test
cargo test -p robotica-tokio --test hdmi test_client_once

# WASM tests (inside robotica-frontend/)
wasm-pack test --headless
```

`unwrap()` and `expect()` are allowed inside `#[cfg(test)]` blocks via:
```rust
#![allow(clippy::unwrap_used)]
```

---

## CI

GitHub Actions (`.github/workflows/ci.yml`) runs `nix flake check --impure`, which executes:
- Clippy with `--deny warnings` for each crate
- Build + unit tests for `robotica-backend`, `robotica-freeswitch`, `robotica-slint`
- WASM build + wasm-bindgen + webpack for `robotica-frontend`

Coverage via `cargoTarpaulin` is defined but currently disabled (broken since Rust 1.77).

Commit messages must follow **Conventional Commits** (enforced by `.commitlintrc.yaml`):
- Allowed types: `build`, `chore`, `ci`, `docs`, `feat`, `fix`, `perf`, `refactor`, `revert`, `style`, `test`
- Max header length: 100 chars, type must be lower-case
- Example: `feat: add tesla charging monitor`

---

## Code Style

### Formatting
Standard `rustfmt` defaults — no `.rustfmt.toml`. Always run `cargo fmt` before committing.

### Imports
- Group: `std` → external crates → internal crates (`crate::`, `super::`)
- Import specific items, not glob imports (except `use super::*` in test modules)
- Serde `rename_all = "snake_case"` is the dominant JSON convention

### Naming Conventions
- Functions, variables, modules, fields: `snake_case`
- Types, traits, enums: `PascalCase`
- Constants: `SCREAMING_SNAKE_CASE`
- Annotate public return types with `#[must_use]` (used extensively on pipe receivers)

### Types and Traits
- Prefer `impl Trait` for function parameters; use concrete types for return types
- Use `async_trait` for async trait methods
- Derive `Debug`, `Clone`, `Serialize`, `Deserialize` as appropriate — not blindly
- Use `tap::Pipe` / `tap::Tap` for readable method-chaining (the `tap` crate is available)

---

## Error Handling

- **`thiserror`** — all library/domain errors; use `#[derive(Error, Debug)]` with typed variants and `#[from]` conversions:
  ```rust
  #[derive(Error, Debug)]
  pub enum SubscribeError {
      #[error("Send error")]
      SendError(),
      #[error("Receive error: {0}")]
      ReceiveError(#[from] oneshot::error::RecvError),
  }
  ```
- **`anyhow`** — only in binary `main()` functions for top-level error propagation (`fn main() -> anyhow::Result<()>`)
- **`eyre`** — used in `robotica-frontend` (WASM context)
- **Do not** use `unwrap()` or `expect()` in non-test code; the clippy denies will reject it
- Fatal startup failures (bad config/env) may use `unwrap_or_else(|e| panic!("{e}"))` in `main()`
- Use `tracing_log_error::log_error!` to log errors in async loops without propagating them

---

## Async Patterns

- **Runtime:** `tokio` with `#[tokio::main]` for binaries, `#[tokio::test]` for async tests
- Use `tokio::select!` for concurrent event handling in loops
- Use the project's `robotica_tokio::spawn` wrapper instead of `tokio::spawn` directly — it fails fast (calls `process::exit(1)`) on task panic
- Channel/pipe types:
  - `stateful::create_pipe(name)` — broadcasts current value, deduplicates identical values
  - `stateless::create_pipe(name)` — fires every message through
  - Receivers expose a functional combinator API: `.map()`, `.filter()`, `.translate()`, `.for_each()`, `.merge()`, `.rate_limit()`, `.send_to_mqtt_json()`

---

## Key Dependencies

| Category | Crates |
|---|---|
| Async | `tokio`, `futures`, `async-trait` |
| MQTT | `rumqttc`, `rustls`, `rustls-native-certs` |
| Serialization | `serde`, `serde_json`, `serde_yml`, `prost` (Protobuf) |
| Error handling | `thiserror`, `anyhow`, `eyre` |
| HTTP / Web | `axum` (WS + macros), `tower`, `tower-http`, `hyper`, `openid` |
| Database | `sqlx` (Postgres + migrate), `geozero` (PostGIS) |
| HTTP client | `reqwest` |
| Logging | `tracing`, `tracing-subscriber`, `tracing-opentelemetry`, `color-backtrace` |
| Date/time | `chrono`, `chrono-tz` |
| Testing | `rstest`, `tokio::test`, `float-cmp`, `approx`, `test-log` |
| Frontend | `yew`, `yew-router`, `wasm-bindgen`, `gloo-*`, `leaflet` |
| GUI | `slint`, `slint-build` |
| Parsing | `lalrpop`, `regex` |
| Utilities | `tap`, `itertools`, `envconfig`, `arc-swap` |

---

## Test Patterns

```rust
// Standard inline unit test module
#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn test_something() { ... }

    // Parameterized tests with rstest
    #[rstest::rstest]
    #[case(input_a, expected_a)]
    #[case(input_b, expected_b)]
    fn test_parameterized(#[case] input: Type, #[case] expected: Type) { ... }

    // Async tests
    #[tokio::test]
    async fn test_async_thing() { ... }
}
```

Integration tests live in `robotica-tokio/tests/`.
