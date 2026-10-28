# 当前平台状态

Harness³ 可在本地运行一套 Managed Agents 平台：通过 Preset 定义母体，以 Agent 池管理实例数量和生命周期，围绕 Goal 创建团队，完成规划、并行执行、证据交接、验证与验收。HTTP API、CLI、MCP stdio 与桌面 Web 共用同一份持久状态。启动和模型配置见[使用指南](getting-started.zh-CN.md)，逐项能力及源码落点见[能力索引](capabilities.zh-CN.md)。

## 可用链路

- **自主协作**：Agent 查看任务板、请求领取下一项工作、追加任务或按授权委派子任务。来源 Run 成功后，平台持久激活请求并继续调度；未派发的直接下游等待新增贡献。任务、认领代次和执行尝试均可追溯。
- **协作空间**：同 Goal 的成员通过频道讨论、持久消息交接及版本化共享上下文交换发现。频道插件参与者自动进入团队讨论，订阅默认关闭；开启订阅后，新发言可唤醒成员，通知 Run 的回复不继续广播。
- **文件与验收**：实例读写自己的 UTF-8 文件，发布不可变文件/JSON 快照供同 Goal 成员读取。验证器检查冻结证据，可按失败产物定位局部返工任务及其下游；人工验收保留最终决定。
- **能力扩展**：提示词包与本地进程工具支持在线安装、固定版本绑定和健康检查。提供[文本统计](../examples/plugins/text-inspector/)、[HTTP 读取](../examples/plugins/http-reader/)和[工作区母体](../examples/plugins/workspace-preset/)示例。

离线体验选择 `demo-team@1.0.0`，真实 Responses 调研选择 `research-team@1.1.0`。Demo 实际经过任务板、频道、文件/JSON 与共享上下文工具，使用确定性模型且不自主追加任务；真实模型按任务需要决定是否追加或委派。

## 运行边界

平台采用单 daemon、SQLite、本地 Worker 和受信任插件。进程插件以 daemon 所在系统用户权限执行；文件工具限制相对路径和符号链接，但没有 OS 沙箱、多租户授权或多机租约。团队费用没有硬上限，Run 仍受轮次、工具调用次数和超时约束。模型接入提供 OpenAI-compatible Chat Completions、OpenAI Responses、Anthropic Messages 三类非流式协议。

| 本地容量约束 | 默认值 |
| --- | --- |
| 活跃实例及待创建预留 | 256 |
| Worker 槽 | 8；同一实例串行 |
| 每 Goal 任务及待追加/待委派预留 | 64 |
| 委派深度 / 单次子任务 / 每根后代 | 6 / 8 / 48 |

任务追加不删除或改写已运行节点，自主领取要求实例在任务候选池中。频道不自动裁决事实冲突；文件证据及 HTTP 回执不能证明外部结论正确。任意任务图重写、通用自主修复、流式模型、远程插件 loader、分布式 Worker、预算治理、细粒度授权和执行隔离尚未提供。容量值属于输入和调度限制，不构成吞吐性能保证。

## 验证范围

已验证原生 Cargo check、Clippy、Rust 测试、rustdoc、quickstart、HTTP 插件 fixture，以及桌面 Web 的主要操作链路。确定性 Demo 和真实 OpenAI Responses / `gpt-6-luna` 均验证了多实例协作、文件交接与验证；真实模型还验证了自主追加、依赖调整和后续汇总。浏览器覆盖 1024、1440、1920 px；完整屏幕阅读器、跨浏览器、多机故障及单/多 Agent 效果对照尚未完成。

能力契约见[任务自主性](autonomy.zh-CN.md)、[频道](channels.zh-CN.md)、[工作区](workspaces.zh-CN.md)和[验证](verification.zh-CN.md)。目标架构及扩展边界见[架构设计](managed-agents.zh-CN.md)。
