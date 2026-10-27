<p align="center">
  <img src="docs/brand/banner.svg" alt="Harness³ — 为 Agent 集群打造的可组合执行平台" width="960">
</p>

<p align="center">
  <a href="https://github.com/NeoPanic/harness3/actions/workflows/check.yml"><img src="https://github.com/NeoPanic/harness3/actions/workflows/check.yml/badge.svg" alt="持续集成"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Rust-1.89%2B-DEA584" alt="Rust 1.89 及以上"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-5BD6C5" alt="MIT 许可证"></a>
</p>

<p align="center"><a href="README.md">English</a> · <a href="GUIDE.zh-CN.md">技术导读</a> · <a href="docs/runtime.md">运行契约</a> · <a href="CONTRIBUTING.md">参与贡献</a></p>

# Harness³

**为 Agent 集群打造的可组合 harness 平台，以 Rust 构建。**

Harness³ 为 Agents 提供共同的执行基础：模型与工具接口、明确的权限边界、可恢复的会话、可追踪的执行过程。宿主应用可以把这些基础组合成更大的 Agent 系统，自行选择模型、存储、通信渠道和协作方式。

当前仓库提供**运行时内核**。集群调度、Agent 间路由、部署、模型供应商 SDK 和生产存储适配器由宿主接入；仓库暂未提供托管服务或集群控制面。

## 为什么是三次方

> 道生一，一生二，二生三，三生万物。——《道德经》第四十二章

**³** 寄托着从基础能力生发复杂系统的含义：一个 harness 支撑一个执行单元，多个 harness 相互组合，构成 Agents 的协作网络。立方也代表相乘的关系：harness × harness × harness，可以继续扩展到 N 个 harness。无限组合是设计愿景，实际规模受宿主架构与资源限制。

品牌展示名为 **Harness³**，小写形式为 `harness³`；GitHub 仓库、Cargo 包和 Rust 导入统一使用 **`harness3`**。Logo 与使用规范见[品牌目录](docs/brand/README.md)。

## 内核提供什么

| 能力 | 实际行为 |
| --- | --- |
| 能力组合 | 通过 Registry 注册模型、工具、技能、提示词、策略和类型化服务，扩展能力无需修改执行循环。 |
| 会话持久化契约 | 带版本栅栏的检查点、输入事件去重、原始请求快照、回放与分叉；内置内存存储，生产持久化由宿主实现。 |
| 执行边界 | 按任务授权工具，支持有序策略、取消、截止时间和显式对外发送。 |
| 工具并发 | 安全的只读工具可主动声明并行能力，每批最多四个；写操作和带执行策略的调用保持顺序。 |
| 上下文管理 | 按预算摘要历史，保留完整交互、回执和当前用户请求。 |
| 可观测性 | 有序 Observer 事件、进程内有界 Trace，以及可由宿主接入的 TraceSink。 |

工具作为受信任的 Rust 代码在进程内执行。外部写操作需要工具侧幂等或事务保证，检查点无法单独保证任意外部操作只执行一次。接入持久化前请阅读[运行契约](docs/runtime.md)。

## 一分钟运行

要求 **Rust 1.89+**。示例采用确定性的本地模型与内存会话存储；依赖安装完成后，运行不需要 API Key、网络或数据库。

```sh
git clone git@github.com:NeoPanic/harness3.git
cd harness3
cargo run --locked --example quickstart
```

预期输出：

```text
Harness³: 8 characters.
Duplicate delivery reused the settled run: true
```

[完整示例](examples/quickstart.rs) 注册输入源、模型、字符计数工具和 Agent 配置，完成“模型 → 工具 → 模型”循环，再次投递同一事件以验证回执复用。接入真实供应商时替换模型适配器即可。

## 在 Rust 项目中使用

```toml
[dependencies]
harness3 = { git = "https://github.com/NeoPanic/harness3.git", branch = "main" }
```

```rust
use harness3::{Harness, Limits, Registry};

let harness = Harness::new(Registry::default(), Limits::default());
```

执行请求前需要按示例注册能力。连续会话使用 `Harness::run_session` 与 `SessionStore`，`Harness::run` 创建独立的临时会话。下游生产构建建议固定 Git revision；这里使用 Git 依赖，未声明已发布至 crates.io。

旧版本的 Cargo 包名 `agent-harness` 和导入名 `agent_harness` 统一改为 `harness3`；`Harness` 等 Rust 类型名称与执行契约保持原有命名。

## 文档与项目状态

- [中文技术导读](GUIDE.zh-CN.md)：时序图、逐文件说明、设计取舍。
- [运行契约](docs/runtime.md)：恢复、权限、上下文压缩、交付与追踪的详细语义。
- [可运行示例](examples/quickstart.rs)：从能力注册到工具执行与会话去重。
- API 文档：`cargo doc --locked --no-deps --open`。
- [贡献说明](CONTRIBUTING.md)、[安全报告](SECURITY.md)、[社区行为准则](CODE_OF_CONDUCT.md)。

当前版本 **0.2.7**，尚未承诺 1.0 稳定 API。独立仓库暂未迁入原应用中的确定性行为测试集。CI 在 Rust 1.89 与 stable 上检查编译和 rustdoc，并运行离线示例；示例覆盖基础集成流程，恢复和并发等行为仍需补充测试。

```sh
cargo check --locked --all-targets
cargo test --locked
cargo run --locked --example quickstart
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
```

欢迎提交可复现的问题、适配器经验、文档修订与针对运行契约的测试。涉及持久化格式、授权或取消语义的变更，请先通过 Issue 讨论约束。

## 许可证

采用 [MIT](LICENSE)，保留原有版权声明。品牌资产随仓库提供，使用与署名约定见[品牌说明](docs/brand/README.md)。
