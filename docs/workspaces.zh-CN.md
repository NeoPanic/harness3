# 实例文件工作区与共享文件产物

本地 daemon 在数据目录的 `workspaces/<instance_id>/` 保存实例文件。每个 Agent 通过 `platform.workspace` 插件读写自己的相对路径；需要让同一 Goal 的其他 Agent 使用某份文件时，发布不可变快照。文件工具不会直接共享整个实例目录。

## 文件交接流程

1. 使用绑定 `platform.workspace@1.0.0` 的母体创建两个或更多实例。
2. 研究 Agent 用 `platform.workspace.write` 写入 `research/evidence.md`。
3. 用 `platform.workspace.publish` 发布该文件，获得稳定的 `fileartifact_...` ID，并在任务输出或共享上下文里引用这个 ID。
4. 同目标 Agent 用 `platform.workspace.artifacts` 查找文件产物，用 `platform.workspace.read_artifact` 读取原文和来源状态。
5. 后续修改原工作区文件不会改变已发布的快照。汇总任务仍需返回最终文本贡献，按既有 Goal 验收流程交付。

工作区工具也适用于普通独立 Run；没有共享 Goal 时，工具只能读取本实例发布的文件快照。Web 操作者可检查所有实例工作区与已发布快照。

## 母体绑定与工具

```json
{
  "name": "file-researcher",
  "version": "1.0.0",
  "purpose": "研究资料并交付文件",
  "objective": "为同目标成员发布可追溯的研究证据",
  "instructions": "把关键发现保存到 research/evidence.md，发布文件快照，在最终输出中引用快照 ID。使用其他成员证据时检查 source_state。",
  "protocol": "demo",
  "plugins": [
    {"id": "platform.workspace", "version": "1.0.0"},
    {"id": "platform.context", "version": "1.0.0"}
  ]
}
```

`demo` 仅用于离线工具链演示，真实调研应采用已配置模型的母体。

| 工具 | 输入 | 作用 |
|---|---|---|
| `platform.workspace.list` | `path`，根目录为 `""` | 浏览本实例文件目录 |
| `platform.workspace.read` | `path` | 读取 UTF-8 文件 |
| `platform.workspace.write` | `path`、`content`、`command_key` | 创建或替换文件，自动创建父目录 |
| `platform.workspace.publish` | `name`、`command_key`，以及 `path` / `value` 二选一 | 发布文件或结构化 JSON 快照 |
| `platform.workspace.artifacts` | 空对象 | 查询本实例或同 Goal 产物 |
| `platform.workspace.read_artifact` | `artifact_id` | 读取不可变原文、摘要、来源 Run 与状态 |

文件与 JSON 原文每份最多 1 MiB，列表每次最多 200 项（Agent 共享产物查询为最近 100 项）。写入采用完整替换，不提供删除和追加。相同 `command_key` 与相同输入重放取得原回执，改变输入返回冲突；发布回执重放不会重新读取已改变的原文件。

工作区工具只接受相对路径，拒绝 `..`、绝对路径和符号链接。此约束仅限定这些文件工具的路径解析；本地进程插件仍以 daemon 所在操作系统用户权限执行，当前没有操作系统沙箱。

## 来源与验证

快照保留 `instance_id`、`run_id`、`goal_id`、格式、名称、原路径、SHA-256、大小和创建时间。`source_state` 来自真实 Run：运行中发布的文件是临时证据；来源失败、取消或结果未知时，不得把其文字当成已验证结论。读取会显示摘要是否匹配，下载保存原文且不执行 HTML。

文件快照使用独立的 `workspace_artifacts` 存储，最终文本产物与文件快照分别保存。启用独立验证时，本轮成功任务 Run 发布的文件与 JSON 快照会进入冻结证据清单，与最终文本产物一同供验证 Agent 读取和引用；旧轮次被替换的 Run 所发布文件不会混入新清单，沿用任务的原始成功 Run 快照可继续纳入。验证仍依赖给定证据，不会主动联网核对外部事实。人工验收清单固定最终文本产物，文件快照由对应验证清单和发布记录追溯。

## Web 与 HTTP

Web“文件工作区”提供实例目录、文本预览、发布快照目录、原始 Run 跳转及原文下载。列表按需刷新，避免把私有文件误显示成已交付产物。

| HTTP | 内容 |
|---|---|
| `GET /api/v1/workspaces` | 工作区配置与实例目录 |
| `GET /api/v1/agent-instances/{id}/workspace?path=` | 实例相对目录 |
| `GET /api/v1/agent-instances/{id}/workspace/file?path=` | 文本文件预览 |
| `GET /api/v1/workspace-artifacts?goal_id=` | 不可变文件 / JSON 产物 |
| `GET /api/v1/workspace-artifacts/{id}` | 快照原文与来源 |

平台身份来自当前 Run，工具参数不能伪造其他实例或来源 Run。HTTP 操作者使用既有本地认证，工作区文件操作暂未增加独立 CLI/MCP 写入口。

数据库使用 `workspace_artifacts` 和 `workspace_commands` 保存快照与幂等回执；实例文件位于独立数据目录，不随 SQLite 备份自动打包。工作区提供 UTF-8 文件与 JSON 操作，不包含通用 shell、二进制文件编辑或代码执行环境。
