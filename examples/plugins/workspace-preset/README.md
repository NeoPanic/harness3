# 文件协作母体示例

`preset.json` 展示第一方工作区插件与共享上下文插件组合。将定义发布到本地平台后，在同一 Goal 中创建多个实例即可交接文件快照。

真实模型场景应将 `protocol` 设为已配置的协议，例如 `openai-responses`；Demo 仅验证确定性的工具调用链。写文件：`platform.workspace.write`；发布：`platform.workspace.publish`；对端读取：`platform.workspace.artifacts` / `read_artifact`。操作示例和当前边界见 [工作区指南](../../../docs/workspaces.zh-CN.md)。
