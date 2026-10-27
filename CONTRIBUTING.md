# Contributing to Harness³

Bug reports, documentation in English or Chinese, integration examples, and focused runtime changes are welcome. The core stays provider-, transport-, and storage-independent; propose a host adapter before adding a mandatory SDK or database dependency.

## Development setup

Install Rust 1.89 or newer, then clone the [repository](https://github.com/NeoPanic/harness3). Run the offline example before making changes:

```sh
cargo run --locked --example quickstart
```

The Cargo package and import are `harness3`. The user-facing brand is Harness³. See [brand usage](docs/brand/README.md) when editing public-facing assets.

## Before opening a pull request

1. Open an issue first for changes to checkpoint formats, recovery, authorization, cancellation, or public trait contracts. Describe the failure mode and proposed compatibility behavior.
2. Keep changes focused. A runtime behavior change should include a deterministic regression test that exercises the affected contract, using a local model/tool or memory store where practical.
3. Update [runtime contracts](docs/runtime.md) and the [Chinese guide](GUIDE.zh-CN.md) when semantics change. Keep the English and Chinese README setup steps consistent.
4. Run the checks below and report the result in the pull request. Disclose checks you could not run.

```sh
cargo check --locked --all-targets
cargo test --locked
cargo run --locked --example quickstart
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
```

Format new Rust code with `rustfmt --edition 2024 path/to/file.rs`. Avoid unrelated repository-wide formatting in a focused patch. CI runs the same compilation, test, example, and documentation checks on Rust 1.89 and stable.

## Review expectations

Explain the concrete trigger and resulting behavior, with a before/after example when useful. Include compatibility consequences for downstream users and persisted sessions. Keep external writes explicit and preserve uncertain outcomes during recovery. New tool implementations must validate their own inputs and document idempotency assumptions.

This repository is pre-1.0 and its original application-level behavior suite has not been migrated. New tests should expand standalone coverage rather than depend on the original host.

## Reporting issues

Use the [issue templates](https://github.com/NeoPanic/harness3/issues/new/choose). Include the Git revision, Rust version, minimal reproduction, expected behavior, and observed outcome. Remove credentials, private conversations, and sensitive trace payloads. Follow [SECURITY.md](SECURITY.md) for vulnerabilities.

Contributions are provided under the repository's [MIT license](LICENSE). Participation follows the [code of conduct](CODE_OF_CONDUCT.md).
