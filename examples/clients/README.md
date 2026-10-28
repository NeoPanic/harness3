# Local platform clients

Start `harness3 up --demo` from the repository root. CLI and MCP connect to that daemon; neither starts a second daemon or accesses SQLite directly.

```sh
./target/debug/harness3 fleets create --file examples/clients/fleet.json --key local-demo-1
./target/debug/harness3 fleets list
./target/debug/harness3 fleets show fleet_REPLACE
./target/debug/harness3 operations show operation_REPLACE
./target/debug/harness3 instances list
```

Inspect the same IDs in Web at `http://127.0.0.1:7333/#fleets`. Creation returns an acceptance receipt; the operation must complete before all requested instances exist. Retry the same key and content after a timeout. Use a new key for a new command.

Replace the two absolute paths in `mcp.json` before copying it to your MCP client's configuration. The data directory must match the running daemon. No key belongs in the JSON: the adapter reads the daemon's local token file. Start with read-only access. If you want this client to manage fleets, execute tasks and publish Presets, add `--allow-write` to the args array. This grants the local operator's broad write authority for this adapter process; scoped grants are not implemented yet.

The outward MCP adapter exposes tools over stdio. It does not load third-party MCP tools into Agents. Remote MCP plugin loading, MCP resources, Streamable HTTP, concurrent tool calls and protocol cancellation are not implemented. Cancel a Run or Goal through its explicit tool instead. HTTP timeouts are bounded at 10 seconds. No automatic write retries or `--wait` are implemented.

Local process Tool packages have a separate daemon opt-in. See [the plugin guide](../../docs/process-plugins.zh-CN.md); `process-preset.json` and `process-fleet.json` bind the supplied executable reference package.

母体继承、多插件与参数示例：`composed-preset.json` / `parameterized-fleet.json`，依赖与操作顺序见 [母体组合指南](../../docs/presets.zh-CN.md)。

`delegated-research.json` 可通过 `harness3 research create --file examples/clients/delegated-research.json --key demo-delegated-research` 发起离线动态委派；查询方式与普通调研一致。角色参数和策略契约见[动态委派](../../docs/dynamic-delegation.zh-CN.md)。
