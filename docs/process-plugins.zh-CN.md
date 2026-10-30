# 本地可执行插件

本地进程插件提供外部工具，支持在线安装、能力发现、固定版本绑定、逐调用独立进程、健康检查、进度、取消与禁用。声明式提示词包使用 `/api/v1/plugins`；进程插件按以下包协议安装，不使用外部 MCP loader。

## 启动与安装

```sh
cargo run --locked -p harness3-cli -- up --demo --allow-process-plugins
# 或在已有 Responses 配置启动参数后追加 --allow-process-plugins
./target/debug/harness3 plugins install-process --source-dir /absolute/path/to/harness3/examples/plugins/text-inspector
./target/debug/harness3 plugins list
./target/debug/harness3 plugins show-process local.text 1.0.0
./target/debug/harness3 plugins action-process local.text 1.0.0 check
```

`--allow-process-plugins` 是本地操作者对执行自己信任代码的显式选择，默认关闭。安装检查会启动入口进程。插件以 daemon 的 OS 用户权限运行，环境变量清空后仅设置固定 PATH 和 LANG；平台不把模型密钥、平台 token、用户 HOME 或其他继承环境交给插件，但这不能阻止同一 OS 用户权限的代码主动读取本机文件、访问网络或启动其他程序。当前机制不适合不可信代码或多租户托管。

daemon 与插件之间没有 shell；启动包内声明的入口。Unix 使用独立进程组，结束时杀死组内剩余进程并回收直接子进程。主动脱离进程组的子进程仍需 OS 沙箱治理。平台同时允许最多 8 个检查/调用进程；获取槽位最多等待 2 秒，满载返回 `plugin_process_capacity_busy`。

Python 示例使用 `/usr/bin/python3`，不需要第三方包。其他语言可构建独立可执行文件并把它列入包。解释器、系统动态库及 OS 本身不在包摘要保证范围内。

## 包与固定绑定

参考 [manifest.json](../examples/plugins/text-inspector/manifest.json)。关键字段：

| 字段 | 契约 |
| --- | --- |
| api_version | 固定 `harness3.plugins/v1` |
| id / version | 命名空间 ID（必须含点）、不可变版本；完整工具名为 `id.name` |
| runtime | `harness3-plugin-rpc/v1` 与包内入口相对路径 |
| files | 最多 32 个相对文件路径及对应小写 SHA-256；不列 manifest.json 自身 |
| tools | 1–16 个 Tool；声明输入/输出 schema、mutates、parallel_safe |
| timeout_ms | 单次工具调用 100–30,000 ms；握手另限 5 秒 |
| instructions | 绑定该包时加入母体运行指令的文本 |

包内禁止绝对路径、`.` / `..`、符号链接和特殊文件；声明文件合计最多 8 MiB，manifest 最多 128 KiB。安装先读取并校验字节，再写入 `.harness3/plugins/<digest>/`；缓存只包含 manifest 与明确列出的文件，未列出的源文件不参与运行。包摘要对规范化 manifest 计算，manifest 固定每个文件的摘要。缓存损坏或丢失会拒绝执行，需要操作者修复/重新部署缓存，不自动退回未验证源路径。

安装成功后，发布 Preset 时用 `plugin: {id, version}` 或 `plugins` 数组引用。平台将完整 manifest、工具 schema、文件摘要与包摘要物化到 `plugin_snapshot`，既有实例和 Session 不随安装新版本变化。母体可使用 plugins 数组绑定多个提示词包与进程包；同 ID 只允许一个版本，工具重名会拒绝发布。继承、参数与多个绑定规则见 [母体组合](presets.zh-CN.md)。提示词包与进程包不能占用相同 ID。

每次工具调用前再次核验缓存摘要、版本和禁用状态。每次调用有独立进程与工作目录，同一包多个版本可并存。新版本健康检查失败不会修改已安装版本；切换/回滚通过发布引用目标版本的新 Preset 并创建新实例完成，不修改旧 Session 或撤销外部效果。

## Agent 使用路径

配置好 Responses 模型后：

```sh
./target/debug/harness3 presets publish --file examples/clients/process-preset.json
./target/debug/harness3 fleets create --file examples/clients/process-fleet.json --key external-inspector-1
./target/debug/harness3 fleets show fleet_REPLACE
# 等待实例就绪，input.txt 写入想统计的原文
./target/debug/harness3 runs submit agent_REPLACE --file input.txt --key external-input-1
./target/debug/harness3 runs show run_REPLACE
```

Web 的“插件与能力”可完成安装、检查契约、健康检查、启用/禁用；“母体 Presets”可选择进程包版本；Run 的请求快照、工具回执和运行事件可检查实际调用。`demo-inspector` 固定调用内置 `text.inspect`，不能用它证明模型会选择外部工具。

## RPC 与生命周期

传输为 UTF-8、每行一个 JSON-RPC 形状的对象，单帧最多 128 KiB。插件 stdout 只用于协议，stderr 不转发进会话或服务日志。请求与回复以相同字符串 id 关联；同时出现 result/error、错误 id、超大输出、未知通知和超过 64 条进度通知都会拒绝。

宿主依次发送：

1. `initialize`，参数 `{protocol: "harness3-plugin-rpc/v1"}`；回复必须包含同一协议、插件 id/version 和与 manifest 完全匹配的 tools。
2. 安装/检查时发送 `health`，要求 `{healthy: true}`。普通调用在握手后发送 `tools/call`。
3. `tools/call` 包含 `name`、`arguments`、唯一 invocation_id、deadline_ms、由宿主生成的 run_id/event_id/source/task 上下文。该上下文用于关联，不是平台管理凭证。
4. 工具回复 `{content: <满足 output_schema 的值>, changed: <bool>}`。只读工具不能宣称 changed=true。
5. `shutdown` 释放资源；最多等待 250 ms 后强制清理进程组。

插件可发送无 id 的 `progress` 通知，params 为 `{invocation_id, message}`，message 最多 2,000 UTF-8 字节。宿主把进度及调用开始/结算记录为平台事件。参数摘要、版本、包摘要和 invocation_id 与回执关联，完整模型工具参数与输出仍由 Harness 保存。

取消或超时时，宿主发送无 id 的 `cancel` 通知并最多再等待 250 ms 的原调用回复。如果收到可验证成功回执，保留这个已知结果；否则强制清理，并将调用标记 `plugin_outcome_unknown`。断连、协议失败、工具 error 或不合法输出在调用发出后同样按结果未知处理。没有调用前失败则标记 not_called。平台不会自动重试任何插件工具；即使声明只读，也不能把断连解释为未执行。

禁用先提交数据库状态，阻止新调用，然后取消已接纳的活动调用。重新启用必须再次通过摘要与健康检查。安装、启用和对外 MCP 写工具授权相互独立；MCP 默认只读，使用进程管理工具还需要 MCP `--allow-write` 和 daemon `--allow-process-plugins`。尚无按主体/工具/对象收窄的 Grant。

## schema 子集与作者 SDK

工具 schema 支持 object/array/string/integer/number/boolean/null、properties、required、明确的 additionalProperties、items、enum、minimum/maximum、minLength/maxLength、minItems/maxItems，以及 description/title。每项 schema 最多 512 节点、16 层、每个 object 最多 64 个属性。输入必须是 object。`$ref`、组合 schema、pattern、format 及所有其他关键词会拒绝安装，不能静默忽略。字节与帧上限独立于字符串的 Unicode 长度约束。

Rust 作者可依赖 `crates/harness3-plugin-sdk`，使用 `serve(manifest, handler)`；示例位于 `crates/harness3-plugin-sdk/examples/text.rs`。`serve_io` 支持自定义输入/输出流做作者契约测试。SDK 是同步参考实现，长调用不能在 handler 执行期间读取取消；需自行实现协作取消协议或接受宿主的硬期限。发布前应测试健康失败、非法参数、输出 schema、断连、超时与副作用未知结果。不存在“安全重放”或平台沙箱保证。
