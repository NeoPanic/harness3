# Harness³ 文档

先运行平台，再根据操作需求查阅专题。设计文档描述目标与约束；当前状态和能力索引描述实际落地范围。

## 启动与使用

- [入门指南](getting-started.zh-CN.md)：本地启动、模型配置、任务发布与兼容流程入口。
- [共享任务板](task-board.zh-CN.md)：任务先发布，控制面动态选型，Agent 认领、拆分与回投。
- [固定调研模板](research.zh-CN.md)：显式使用规划、研究、汇总和验收的兼容流程。
- [客户端示例](../examples/clients/)：CLI 计划文件与 MCP 配置。

## 当前状态

- [平台状态](status.zh-CN.md)：可用链路、容量限制、未提供能力和验证边界。
- [能力与源码索引](capabilities.zh-CN.md)：逐项实现状态、当前模块与目标目录映射、完整源码树。修改能力 JSON 后运行 `python3 scripts/render-progress.py` 更新。

## 设计与内核

- [Managed Agents 架构](managed-agents.zh-CN.md)：设计目标、理论基础、对象与状态约束、插件范式和完整目标目录树。
- [持久工作流职责](durable-workflows.zh-CN.md)：运行恢复与 Agent 协作的职责边界。
- [运行契约](runtime.md)：检查点、恢复、授权、压缩、投递与追踪。
- [Rust 内核导读](runtime-guide.zh-CN.md)：从输入到执行结果的源码路径与设计推理。
- [控制台设计规范](../DESIGN.md)与[品牌规范](brand/README.md)。

## 协作与扩展参考

| 主题 | 内容 |
| --- | --- |
| [母体组合](presets.zh-CN.md) | 固定版本继承、参数和插件绑定 |
| [共享任务板](task-board.zh-CN.md) | 无实例绑定发布、Preset 选型、原子认领与拆分 |
| [Goal 任务领取](task-claims.zh-CN.md) | 兼容候选池、所有权和执行尝试 |
| [Goal 自主协作](autonomy.zh-CN.md) | 兼容后续工作追加和接续约束 |
| [动态委派](dynamic-delegation.zh-CN.md) | 角色、父子任务和续作 |
| [共享上下文](shared-context.zh-CN.md) | 发现、决策、来源与修订 |
| [协作频道](channels.zh-CN.md) | 成员可见性、订阅与唤醒 |
| [点对点消息](messaging.zh-CN.md) | 投递、派发、消费和恢复 |
| [文件工作区](workspaces.zh-CN.md) | 私有文件与跨 Agent 快照共享 |
| [协作产物](artifacts.zh-CN.md) | 不可变产物、轮次与验收 |
| [独立验证](verification.zh-CN.md) | 固定证据、验证报告与局部修复 |
| [进程插件](process-plugins.zh-CN.md) | 包格式、安装、绑定与执行契约 |

插件示例：[文本统计](../examples/plugins/text-inspector/)、[HTTP 读取](../examples/plugins/http-reader/)、[文件交接母体](../examples/plugins/workspace-preset/)。

开发流程见[参与贡献](../CONTRIBUTING.md)。公共 API 文档通过 `cargo doc --locked --workspace --no-deps --open` 生成。
