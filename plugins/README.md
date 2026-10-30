# 官方内置插件

此目录是 `harness3-builtin-plugins` Rust 包，随服务编译发布。控制台插件页仅展示内置能力及配置状态，暂不提供安装操作。

| 插件 ID | 工具 | 用途与配置 |
| --- | --- | --- |
| `text` | `text.inspect` | Unicode 字符、行数和 UTF-8 字节统计，无需配置 |
| `web.fetch` | `web.fetch` | 读取公开 HTTP/HTTPS 文本，无需密钥 |
| `web.search` | `web.search` | Brave Search 网页搜索，服务启动前设置 `HARNESS3_WEB_SEARCH_API_KEY` |

网页工具通过母体的插件绑定显式启用，使用版本 `1.0.0`；锁定描述摘要，安装或配置不会自动给现有实例增加权限。文本统计保留在普通 Run 的基础工具集中。

网页搜索参数为 `query`、可选 `count`（1–10，默认 5），返回标题、URL 与摘要。查询会发往 Brave Search，费用由搜索服务提供方计收；未配置密钥时返回 `web_search_not_configured`。密钥来自服务进程环境，不写入插件快照、数据库或前端响应。

网页读取参数为 `url`，只接受公开地址的 80/443 端口，拒绝 URL 凭据、本地/私网地址和重定向。DNS 验证后固定解析地址，禁用环境代理；DNS 超时 5 秒，请求超时 20 秒，响应最多 1 MiB。只返回 UTF-8 文本或 JSON，HTML 返回源码。网络工具支持取消；网页和搜索结果均标记为不可信内容。

```text
plugins/
├── Cargo.toml
├── src/                 # 注册表、配置状态、网络边界
├── text/mod.rs
├── web-fetch/mod.rs
└── web-search/mod.rs
```

内置工具不依赖平台 SQL、Worker 或 HTTP Server。组织协作工具仍在平台应用模块中，避免复制事务逻辑。`crates/harness3-plugins` 保留进程插件宿主与底层兼容接口；其协议/SDK 位于各自 crate，教学示例在 `examples/plugins`。进程包能力见[进程插件文档](../docs/process-plugins.zh-CN.md)。
