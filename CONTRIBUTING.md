# Contributing to Harness³

Bug reports, documentation in English or Chinese, integration examples, and focused runtime changes are welcome. The core stays provider-, transport-, and storage-independent; propose a host adapter before adding a mandatory SDK or database dependency.

## Development setup

Install Rust 1.89 or newer, then clone the [repository](https://github.com/NeoPanic/harness3). Run the offline example before making changes:

```sh
cargo run --locked --example quickstart
```

The Cargo package and import are `harness3`. The user-facing brand is Harness³. See [brand usage](docs/brand/README.md) when editing public-facing assets.

The root manifest defines the workspace; the independent `harness3` package lives in `crates/harness3-core`. Domain contracts, model providers, plugin hosting, worker execution, HTTP hosting, HTTP/MCP clients, and the CLI each have their own crate. The platform retains application services and SQLite persistence. See the [workspace boundaries](docs/workspace.zh-CN.md) before adding a cross-crate dependency. The console lives in `web/src/app` and `web/src/features`; it uses native JavaScript and is embedded by the HTTP adapter at compile time. Run the quickstart from the repository root as shown above; start the platform with `cargo run --locked -p harness3-cli -- up --demo`.

Official plugin packages belong in [`plugins/`](plugins/README.md), one directory per plugin. Keep teaching examples in `examples/plugins` and protocol/SDK/host implementation in their respective crates.

## Before opening a pull request

1. Open an issue first for changes to checkpoint formats, recovery, authorization, cancellation, or public trait contracts. Describe the failure mode and proposed compatibility behavior.
2. Keep changes focused. A runtime behavior change should include a deterministic regression test that exercises the affected contract, using a local model/tool or memory store where practical.
3. Update [runtime contracts](docs/runtime.md) and the [Chinese guide](docs/runtime-guide.zh-CN.md) when semantics change. Keep the English and Chinese README setup steps consistent.
4. Run the checks below and report the result in the pull request. Disclose checks you could not run.

```sh
python3 scripts/render-progress.py --check
python3 scripts/check-boundaries.py
node --test scripts/test-*.cjs
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo run --locked --example quickstart
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
```

Format new Rust code with `rustfmt --edition 2024 path/to/file.rs`. Avoid unrelated repository-wide formatting in a focused patch. CI runs the same compilation, test, example, and documentation checks on Rust 1.89 and stable.

## Review expectations

Explain the concrete trigger and resulting behavior, with a before/after example when useful. Include compatibility consequences for downstream users and persisted sessions. Keep external writes explicit and preserve uncertain outcomes during recovery. New tool implementations must validate their own inputs and document idempotency assumptions.

This repository is pre-1.0. Deterministic workspace tests cover runtime contracts and platform integration. Extend this coverage when changing behavior. See the [local platform guide](docs/getting-started.zh-CN.md) for the current implementation boundary.

## Reporting issues

Use the [issue templates](https://github.com/NeoPanic/harness3/issues/new/choose). Include the Git revision, Rust version, minimal reproduction, expected behavior, and observed outcome. Remove credentials, private conversations, and sensitive trace payloads. Follow [SECURITY.md](SECURITY.md) for vulnerabilities.

Contributions are provided under the repository's [MIT license](LICENSE). Participation follows the [code of conduct](CODE_OF_CONDUCT.md).
