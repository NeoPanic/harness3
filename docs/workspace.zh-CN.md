# Workspace 边界

仓库根清单定义 13 个包（12 个 crates/ 下的 crate，加上 plugins/ 内置插件集合）。包名 `harness3` 对应独立执行内核；同名命令行二进制由 `harness3-cli` 提供。

| Crate | 职责 | 项目内直接依赖 |
| --- | --- | --- |
| `harness3`（目录 `harness3-core`） | 单 Agent 执行内核 | 无 |
| `harness3-domain` | 共享请求、策略、领域类型、纯校验与容量常量 | 无 |
| `harness3-plugin-protocol` | 版本化插件协议与 schema 校验 | 无 |
| `harness3-plugin-sdk` | 插件作者 SDK | plugin-protocol |
| `harness3-models` | 三类模型协议、配置加载与 Provider | core、domain |
| `harness3-plugins` | 插件安装、摘要验证、进程管理、执行与取消 | core、domain、plugin-protocol |
| `harness3-storage` | SQLite 连接、schema、事务、SessionStore | core |
| `harness3-builtin-plugins`（根目录 `plugins`） | 内置文本、网页读取、网页搜索工具 | core |
| `harness3-platform` | 控制面、协作、任务调度、业务事务与插件存储适配 | core、domain、storage、models、plugins、plugin-protocol、builtin-plugins |
| `harness3-worker` | Run 执行、恢复、队列消费与工具装配 | core、platform、models、plugins、builtin-plugins |
| `harness3-server` | HTTP、认证、Web 嵌入与本地服务装配 | core、domain、platform、models、plugins、worker、client、builtin-plugins |
| `harness3-client` | 类型化 HTTP 客户端 | domain |
| `harness3-cli` | 命令解析、输出与启动服务入口 | domain、client、server |

domain 不依赖 SQLite、HTTP、异步执行或平台实现。存储错误由 storage 错误类型转换，HTTP 状态响应由 server 适配；错误代码和响应结构保持不变。需要读取已发布母体的策略校验仍在平台内执行。

插件宿主依赖 `harness3_plugins::Repository`，平台 Store 实现六个操作：内置 ID 检查、插件读取、事务安装、健康记录、启停状态和事件记录。禁用状态提交与活动调用取消继续共用原有宿主锁；SQL 事务不移入插件 crate。

client 的常规依赖不包含 core、platform、Worker、插件宿主、Axum 或 SQLite。MCP 适配继续使用同一 HTTP 客户端，不直接访问数据库。CLI 为提供 `up` 命令依赖 server，因此完整 CLI 仍会链接服务端。

`python3 scripts/check-boundaries.py` 检查常规依赖图、上述底层包的传递依赖约束和二进制归属，CI 同步执行。集成测试在 server crate，CLI 子进程测试在 cli crate，插件作者契约测试仍在 SDK crate。

## 命令与导入迁移

```sh
cargo run --locked -p harness3-cli -- up --demo
cargo run --locked --example quickstart
cargo test --locked --workspace
```

已有构建产物路径 `target/debug/harness3`、HTTP 资源 URL、数据库位置、MCP 启动参数和协议保持不变。原 `cargo run -p harness3-platform` 入口移至 `harness3-cli`。

| 原 Rust 导入 | 当前导入 |
| --- | --- |
| `harness3_platform::api` | `harness3_server::api` |
| `harness3_platform::runtime` | `harness3_worker` |
| `harness3_platform::client` | `harness3_client` |
| `harness3_platform::mcp` | `harness3_server::mcp` |
| `harness3_platform::models` | `harness3_models`；平台保留重导出 |
| `harness3_platform::plugins` | `harness3_plugins`；平台保留重导出 |
| 共享请求与策略类型 | `harness3_domain`；原平台业务模块保留对应重导出 |

storage 负责连接、schema 初始化、事务和会话持久化；platform::storage::Store 保留应用查询、业务事务与历史数据回填。Worker 通过 Store 使用应用用例，尚未抽象为完整 Repository 接口。

对外 MCP Server 位于 server/src/mcp.rs，通过 client 调用 HTTP；未来 MCP 客户端接入应放在 client 内。无需独立 MCP 包或目录。

worker 按 demo、runner、recovery、scheduler 分文件；server/api 按 routes、auth、handlers、status 分文件。仅在模块具有独立职责时继续拆分，不为目标树预建空目录。
