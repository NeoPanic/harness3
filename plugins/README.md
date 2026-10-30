# 官方插件

此目录存放随 Harness³ 维护和发布的第一方插件，每个插件使用独立子目录。当前尚未发布独立的官方插件包。

```text
plugins/
└── <plugin-name>/
    ├── README.md         # 用途、安装、配置和权限
    ├── manifest.json     # 包 ID、版本、入口、文件摘要和工具 schema
    ├── src/              # 源码；单文件插件也可直接放入口文件
    └── tests/            # 协议与工具行为测试
```

进程插件沿用现有 `harness3.plugins/v1` 包格式和 `harness3-plugin-rpc/v1` 协议。包内可执行入口及依赖需列入 manifest 的 `files` 摘要；按固定版本安装和绑定，放进此目录不会自动启用。Rust 插件依赖公开的协议与 SDK，不依赖平台内部 SQL 或私有实现。

| 目录 | 职责 |
| --- | --- |
| `plugins/` | 正式维护、独立版本的官方插件包 |
| `examples/plugins/` | 演示协议和开发方式的教学示例 |
| `crates/harness3-plugin-protocol/` | 插件协议与包校验类型 |
| `crates/harness3-plugin-sdk/` | Rust 插件作者 SDK |
| `crates/harness3-plugins/` | 插件宿主、安装、进程监督和执行实现 |
| `crates/harness3-platform/src/storage/plugins.rs` | 插件持久化和事务适配 |

当前消息、工作区、验证等内置能力仍由平台模块提供；后续独立打包时再迁入本目录，不复制一份实现。

包格式、安装和执行约束见[进程插件文档](../docs/process-plugins.zh-CN.md)，参考实现见 [HTTP 读取示例](../examples/plugins/http-reader/README.md)。
