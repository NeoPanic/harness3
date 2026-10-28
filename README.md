<p align="center"><img src="docs/brand/mark.svg" width="64" alt="Harness³"></p>

# Harness³

**A local Managed Agents platform, built on a composable Rust runtime.**

Define an Agent with a Preset, launch independent instances, and give a team a shared objective. Harness³ manages their lifecycle, task dependencies, collaboration channels, workspace artifacts, verification, and human acceptance in one inspectable control plane.

[简体中文](README.zh-CN.md) · [Documentation](docs/README.md) · [Architecture](docs/managed-agents.zh-CN.md) · [Current status](docs/status.zh-CN.md) · [Contributing](CONTRIBUTING.md)

![Harness³ desktop console overview](docs/images/console-overview.png)

## Start locally

Requires **Rust 1.89+**. The deterministic Demo needs no model account or API key; Cargo downloads dependencies on the first build.

```sh
git clone https://github.com/NeoPanic/harness3.git
cd harness3
cargo run --locked -p harness3-platform -- up --demo
```

Open [localhost:7333](http://127.0.0.1:7333) and sign in with the contents of `.harness3/token`. Local state is stored in `.harness3/`.

1. Open **自主调研** (Autonomous Research) and create a task using `demo-team@1.0.0`.
2. Inspect the plan, participating Agents, task dependencies, and individual Runs.
3. Follow channel posts and file/JSON evidence through synthesis, verification, and acceptance.

The Demo exercises the real control plane with deterministic responses; its outputs are explicitly labeled as demonstrations. It does not perform external research or autonomously append tasks. With a configured Responses model, `research-team@1.1.0` also supports Agent-requested follow-up work.

For real models, configure one of **OpenAI-compatible Chat Completions**, **OpenAI Responses**, or **Anthropic Messages**. See the [getting-started guide](docs/getting-started.zh-CN.md) for configuration, CLI/API/MCP usage, and trusted local process plugins.

## What you can use

| Area | Available behavior |
| --- | --- |
| Agent definitions | Versioned Presets, pinned inheritance, parameters, model and plugin bindings. |
| Lifecycle | Independent instances; Agent pools for provisioning, scaling, retirement, and TTL. |
| Team execution | Planning, task dependencies, candidate-based claiming, bounded delegation and follow-up tasks. |
| Collaboration | Direct messages, Goal channels, explicit membership, subscribed wake-ups, shared context. |
| Deliverables | Private workspaces, immutable text/file/JSON snapshots, peer evidence handoffs. |
| Review | Independent verification, evidence-linked repair, review rounds, human acceptance. |
| Extension | Prompt packages and local process Tool plugins with pinned versions and health checks. |
| Access | Desktop Web, HTTP API, CLI, and outward MCP stdio share persistent local state. |

Agent pools manage resources from one Preset; teams coordinate work around a Goal. Creating a pool alone does not start a collaborative task.

<details>
<summary>Explore the running Demo: team collaboration and workspace artifacts</summary>

**Team collaboration.** Members, subscriptions, and discussion records remain inspectable after the team retires.

![Team collaboration in the deterministic Demo](docs/images/team-collaboration.png)

**Workspace evidence.** Shared immutable snapshots retain their source Agent and Run; private instance files are listed separately.

![Workspace files and shared evidence](docs/images/workspace-artifacts.png)

</details>

This is a **pre-1.0 local platform for trusted environments**. Local defaults include 256 instances, 8 execution slots, and 64 tasks per Goal; these are configured limits, not measured capacity claims. Distributed scheduling, hosted deployment, strong multi-tenant isolation, and remote MCP plugin loading are not provided. See [current status](docs/status.zh-CN.md) and the [capability/source index](docs/capabilities.zh-CN.md) for precise boundaries.

## Architecture

```mermaid
flowchart LR
    Entry[Web / API / CLI / MCP] --> Control[Local control plane]
    Preset[Versioned Presets] --> Control
    Control --> Team[Goals / tasks / Agent instances]
    Team --> Runtime[Harness runtime]
    Runtime <--> Models[Three model protocols]
    Runtime <--> Plugins[Tools and plugins]
    Control <--> State[SQLite / sessions / artifacts]
```

The control plane owns coordination and lifecycle. The independent runtime owns model/tool execution, policies, session checkpoints, cancellation, and execution receipts. Checkpoints do not guarantee exactly-once external effects: tools must supply their own idempotency or transaction contract. See [runtime contracts](docs/runtime.md) and the [target architecture](docs/managed-agents.zh-CN.md).

## Use the runtime independently

```sh
cargo run --locked --example quickstart
```

The [offline example](examples/quickstart.rs) runs a model → tool → model cycle, then redelivers the same event to demonstrate session deduplication.

```toml
[dependencies]
harness3 = { git = "https://github.com/NeoPanic/harness3.git", branch = "main" }
```

```rust
use harness3::{Harness, Limits, Registry};

let harness = Harness::new(Registry::default(), Limits::default());
```

Register capabilities as shown in the complete example. Pin a Git revision for reproducible builds. API reference: `cargo doc --locked --workspace --no-deps --open`.

## Why the cube?

> 道生一，一生二，二生三，三生万物。 — *Dao De Jing*, chapter 42

The **³** connects the generative meaning of 三 with multiplication: harness × harness × harness, extending to N cooperating harnesses. Use **Harness³** in prose and `harness3` for code and repository identifiers. [Brand assets and usage](docs/brand/README.md).

## Contribute

See [development checks](CONTRIBUTING.md), [security reporting](SECURITY.md), and the [code of conduct](CODE_OF_CONDUCT.md). The [documentation index](docs/README.md) links setup, design goals, current capabilities, and workflow contracts.

[MIT license](LICENSE).
