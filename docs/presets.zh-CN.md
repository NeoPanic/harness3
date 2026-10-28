# 母体组合与实例参数

已发布母体是不可变快照，支持固定版本单继承、多个插件绑定、严格参数 schema 与创建时展开。API、CLI、MCP 和 Web 使用相同发布与 Agent 池契约；控制接口继续使用 Fleet / fleets 名称。母体按本地操作者明确选择的插件运行，不进行按主体的权限收窄，也不在线升级已有实例。

## 定义与发布

独立定义要求 name、version、purpose、objective、instructions、protocol。衍生定义只要求 name、version、extends；文本与协议省略时沿用父级，显式文本必须非空。父级必须已经发布，按 name/version 固定；最多 16 层，禁止引用自身。子级保留完整 lineage 与原始 publication，运行使用物化后的配置，不在执行时重新读取父级。

```json
{
  "name": "topic-inspector",
  "version": "1.0.0",
  "extends": {"name": "researcher", "version": "1.0.0"},
  "objective": "检查 {{topic}}，只依据工具回执报告统计结果",
  "plugins": [
    {"id": "local.evidence-guide", "version": "1.0.0"},
    {"id": "local.text", "version": "1.0.0"}
  ],
  "parameters_schema": {
    "type": "object",
    "properties": {"topic": {"type": "string", "minLength": 1, "maxLength": 200}},
    "required": ["topic"],
    "additionalProperties": false
  },
  "parameter_defaults": {"topic": "Harness³"}
}
```

安装示例包后发布：

```sh
./target/debug/harness3 presets publish --file examples/clients/composed-preset.json
./target/debug/harness3 fleets create --file examples/clients/parameterized-fleet.json --key topic-inspector-1
```

示例依赖已发布 `researcher@1.0.0`、`local.evidence-guide@1.0.0` 和 `local.text@1.0.0`；目录中缺失任一依赖时发布会明确失败。进程包安装与调用需要 daemon `--allow-process-plugins`，详见 [进程插件](process-plugins.zh-CN.md)。无需供应商账号时，可把父级替换成 `demo-inspector@1.0.0` 验证控制面；Demo 固定调用内置工具，不能用其输出证明外部工具执行。

## 继承与能力锁

- `plugins` 省略时继承父级绑定；显式数组整组替换，`[]` 清空。原有单个 `plugin` 引用继续可用，`plugin: null` 清空；不能同时提交 plugin 和 plugins。
- 最多 16 个包，同一 ID 只能绑定一个版本，所有工具必须无重名（包括内置 text.inspect）；总工具数最多 64，包指令合计最多 64,000 UTF-8 字节。包顺序固定，按顺序拼接指令。
- 发布固定每个包版本、内容摘要、工具契约和指令。一个绑定仍使用兼容字段 plugin_snapshot，多个绑定使用 plugin_snapshots；读取工具应同时支持这两种形式。提示词包快照同时保存内容摘要。
- 新发布需引用已启用进程版本；之后禁用仍会阻止该版本被旧实例调用。相同原始定义重发返回既有发布回执，不受之后的启停影响；改动原始定义必须更换版本。已发布定义不会静默改写。
- 发布仅锁定已安装的进程包记录；实际调用仍重新验证缓存摘要。包缓存损坏不会被新母体掩盖。

## 参数 schema、默认值与模板

parameters_schema 使用与插件协议相同的严格有界 JSON Schema 子集，根必须为 object 且 additionalProperties=false，最多 32 个顶级参数。支持的关键词与拒绝规则见 [schema 子集](process-plugins.zh-CN.md#schema-子集与作者-sdk)。参数名遵循平台名称字符规则。schema 不接受 default 关键词，默认值单独放在 parameter_defaults。

参数 schema 与默认值分别按整字段继承/替换。子级替换 schema 后必须保证继承的默认值仍合法；要清空默认值，显式提交 `{}`。默认值逐项校验；required 可由创建时输入补齐。未声明参数时，不允许创建者传入非空参数对象。

用途、目标与系统指令支持 `{{name}}`（名称两侧空格忽略）。引用必须在 schema 中声明，并为 required 或具有默认值；不支持表达式、嵌套引用、文件读取或环境变量。未配对的模板分隔符会拒绝发布。模板保留字符 `{{` / `}}` 不用于普通文本转义。字符串原样替换，其他 JSON 值使用 JSON 文本；对插入内容不再次展开。插件指令保持包内原文，不作模板替换。

同一 Agent 池内的实例共享参数。创建时先合并默认值与显式输入、校验完整 schema，再展开并校验最终文本长度；失败的整个事务回滚，不产生 Agent 池、Operation 或容量预留。实际参数（含默认值）最多 16 KiB JSON；最终用途/目标/指令仍受 2,000/8,000/32,000 UTF-8 字节限制。参数是传给模型的任务配置，不是可信系统权限，也不应承载密钥。

```json
{
  "preset_name": "topic-inspector",
  "preset_version": "1.0.0",
  "count": 2,
  "mode": "finite",
  "parameters": {"topic": "Harness³"}
}
```

同一 Agent 池的扩容、maintain 补建和重启续作复用已保存的最终快照，不重新合并默认值或展开模板。修改参数需要创建新的 Agent 池。参数参与创建请求的幂等指纹；同 key 不同参数返回冲突。对外 Agent 池 API 支持显式 parameters；同步 Rust `Store.spawn` 使用默认值，必填值缺失时拒绝。

## Web 与入口

“母体 Presets → 发布母体”可选择父级、覆盖文本与协议、选择多个插件、定义文本/数字/布尔参数。“创建实例”随所选母体展示参数控件，空值省略并使用默认值；复杂 object/array 参数可输入 JSON。详情显示继承来源、全部能力摘要及参数 schema，实例详情另显示实际参数与展开后的目标。

CLI 的 presets publish / fleets create 使用上述 JSON。MCP presets_publish / fleets_create 接受相同对象；写工具仍需 --allow-write。HTTP 发布使用 `POST /api/v1/presets`；创建 Agent 池使用 `POST /api/v1/agent-fleets`，返回 202 接纳回执与异步 Operation。

当前没有细粒度权限系统，不能据此宣称子级能力已经通过按主体的授权收窄；控制面只执行本地操作者显式提交的绑定替换。Web 参数编辑器只创建扁平标量 schema，API 可发布协议允许的嵌套 schema；UI 默认值留空代表未设置，需要默认空字符串时使用 API/CLI。
