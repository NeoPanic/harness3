# Harness³ runtime contracts

Detailed behavior and integration boundaries for the Rust runtime. Start with the [README](../README.md), [offline example](../examples/quickstart.rs), or [中文导读](runtime-guide.zh-CN.md).

## Composition

| Capability | Contract |
| --- | --- |
| Models | `Model`: portable request/reply, optional provider Token usage and opaque continuation; reports live deltas into `LiveOutput` |
| Inputs | `Source`: decode a channel event; networking belongs to the host |
| Presets | `AgentProfile`: instructions, visible tools and selected skills |
| Tools | `Tool`: validate, execute, return a truthful result |
| Prompts | `PromptSection`: ordered, context-aware contributions; optional `InteractionPrinciples` |
| Skills/resources | `skills::Skill`, `FileBundle`: reloadable instructions and bounded resource reads |
| Services | `Registry::provide/service`: named, type-checked capabilities |
| Capability installation | `Registry::register`: transactional registration into a registry snapshot |
| Scheduled work | `work::WorkStore`: generic lease seam, implemented by the host |
| Execution | `AgentDriver`: replaceable loop; `Harness` implements it |
| Conversations | `SessionStore`: revision-fenced checkpoints, messages, request snapshots and receipts |
| Developer Trace | `Harness::trace_events` collects in process; `TraceSink` receives the events |
| Observability | `Observer`: ordered run events, including tool arguments and outcomes |
| Interception | `Policy`: ordered request/call rewrite or denial; post-commit checks |
| Delivery | `Transport`: explicit host-authorized outbound message and external receipt |

The simplest host registers a source, model, optional tools/skills/prompts and a task, then calls `Harness::run_session`. `Harness::run` intentionally creates an isolated ephemeral conversation. A host that needs its own loop implements `AgentDriver`; channel identity and session-key policy belong to the host.

Consecutive authorized read calls in one model reply execute in batches of at most four when their implementations opt in with `Tool::parallel_safe()`. The default is sequential: `mutates=false` alone is not sufficient because reads can share browser state or extraction caches. Mutating tools, non-opted-in tools and skill/tool loading are ordering barriers; a registry with execution policies remains sequential to preserve rewrite and post-receipt semantics. Each batch's dispatches are checkpointed before execution, then receipts are checkpointed individually in completion order and correlated by call ID. Cancellation, deadlines, checkpoint errors and observer errors drain started calls before stopping; no new batch starts after a run-level failure. Concurrent futures stay in the caller's task so host task-local context is preserved. No persistent format change is required.

For progressive tools, register `Vec<String>` under the `progressive-tools` service: these few entry tools are immediately available, filtered by task authority. The full tool catalog stays out of the system prompt. `skill_load` returns guidance, loads the bundle's `initial_tools` (at most eight declared tools, plus its resource reader), and lists its remaining authorized tools. `tool_load({})` discovers the full authorized catalog; `tool_load({"names":[...]})` loads selected schemas. Successful load receipts preserve schemas across turns; newly loaded tools become callable on the next model request.

Installation functions passed to `Registry::register` must only register owned capabilities; registration is transactional, so a failing install leaves the previous registry intact. This does not roll back external effects or mutations inside shared objects. Rust `Arc`/`Drop` provide resource ownership; the runtime never unloads native code under a running future.

## Conversation and recovery contract

- User messages, complete assistant replies (including tool calls/continuation), and every tool result are persisted. A normal assistant question followed by a normal user message is a multi-turn interaction; no clarification state machine is required.
- Each admitted request records its prompt, tool schemas, model registration key and history range. `Session::request_input` reconstructs the exact harness-level request. Model adapters own SDK-specific translation; the harness owns history compaction.
- Tools receive errors as model-visible observations, including optional explanations and structured details. Unknown/disallowed calls cannot execute. Malformed tool JSON and empty model replies each allow one corrective generation within the existing round/deadline budget; completed tool effects are retained. Ordinary model/network failures have no generic automatic retry.
- A delivery is scoped by session plus source/event ID. A duplicate settled delivery returns its report; changed content under that ID conflicts. A new intentional attempt needs a new event ID. This prevents re-running writes while preserving the original outcome.
- Stores must atomically compare revisions. A session with a live owner rejects another run. The host must establish exclusive ownership and prove an old run is stopped before calling `recover_interrupted`; a timestamp alone is insufficient.
- `Session.events` holds immutable checkpoint deltas: turns, admitted requests, effective tool dispatches, state and receipts. `replay(cursor)` has no side effects; `fork(cursor)` accepts settled boundaries and starts at revision zero. `compact(through, summary)` also supports host-supplied summaries at settled boundaries. Compaction only changes future projections, retaining original events and exact historical requests.
- Stores call `seal()` and atomically validate the old event prefix with `validate_append()`. Stores may persist the log and projection in one checkpoint; per-event storage rows are not required by the contract.
- Recovery preserves known receipts, marks missing tool results as outcome-unknown, and never automatically replays an uncertain operation. Exactly-once external effects require tool-side idempotency/transactions. The runtime cannot make an arbitrary remote write atomic with its checkpoint.
- Cancellation and deadlines signal tools cooperatively, but started tool futures are drained rather than dropped. Tool implementations must honor cancellation and settle; arbitrary in-process code cannot be forcibly killed safely.
- A model call that is cancelled or times out keeps the text it had already streamed: those deltas become the assistant's turn (no continuation, no tool call) when the run settles. Fragments are accumulated in memory, not checkpointed individually; a process crash before settlement can lose them. The run itself still fails and is never reported as success. Adapters must report through `LiveOutput` only what they would be willing to persist; a host that streams credentials must scrub before reporting.

Memory stores are supplied for embeddings/tests. Durable stores implement the same traits; durable adapters belong to the host, not this crate. Checkpoints carry `Session::format_version`; a store that meets a newer value must refuse it (`Session::ensure_supported`) rather than reinterpret fields it does not know, which is what makes a shared cloud store safe to roll forward.

Policies run in registration order. `before_step` can rewrite a request but cannot add unauthorized tool schemas; `before_tool` can rewrite a call but must preserve its ID. The original call and effective dispatch are both retained. `after_tool` receives an immutable, already-durable receipt: returning an error stops subsequent work without denying an effect that already happened. Tool-side validation remains mandatory.

`Registry::deliver` is explicit and never called automatically after a model reply. The host claims its delivery ledger and binds credentials/recipient context in a `Transport` adapter first. Unknown sends are not retried by the framework; recovery and durable delivery state belong to that host ledger.

## Scope

Native Rust plugin composition, instruction/resource bundles and automatic semantic context compaction are supported. Dynamic native library unloading, Node/Cordis plugin execution, MCP transports, provider SDKs and application-specific approval UIs are separate adapters/capabilities. Adding a normal tool or skill does not require changing the loop.

## Context budget

Every model round checks `used_tokens / window_tokens`. The latest matching model reply's `usage.input + usage.output` is the baseline; cached input is already included, and earlier calls are not accumulated. New content and growing instructions/schemas are estimated at serialized UTF-8 bytes / 3. Without matching usage (first request, changed model, or freshly compacted history), the full input uses that estimate. This is an occupancy heuristic, not exact tokenization or billing. `context_usage` traces record the count, percentage and effective configuration.

`Model::context_budget()` provides model defaults, with `Harness::with_context_budget` as an explicit host override. Defaults trigger at 80%, target 60%, allow an 8192-token summary, reserve 8192 output tokens and retain the two most recent complete exchanges. Unknown models default to a 128,000-token window. Host model adapters supply their actual context limits without changing the runtime.

Hosts configure `ContextBudget` through the model or `Harness::with_context_budget`. The trigger and target must satisfy `0 < target_percent < trigger_percent < 100`; output reservation must cover normal replies and compaction. Retention may prevent reaching the target. If the remaining input still fits, execution continues; truncated summaries are rejected. Environment variables and settings UI are host concerns.


The same registered model summarizes older complete exchanges, including during a long tool loop. Parallel calls and their receipts stay together. The current user request is retained verbatim; retained native continuations are untouched. Summaries merge prior summaries, preserve known outcomes and uncertainty, and are labeled untrusted historical context rather than fresh instructions or authorization.

Internal requests have `purpose="compaction"`, are checkpointed before dispatch, pass through `before_step` policies with no tools, and use the same model/budget wrapper, cancellation and run deadline. Adapters must honor `LiveOutput::is_internal()` and `max_output_tokens()` and never publish these outputs to chat observers. Summary usage contributes to billed run totals, not to the next conversation's occupancy baseline; compaction trace events record boundaries, counts, reply and errors. Valid summaries are revision-fenced in the session checkpoint and survive reopen without changing chat history.

Failed, cancelled, empty, oversized or tool-calling summaries are rejected without replacing the prior projection. No provider retry, tool execution or destructive truncation is performed. If the current request, schemas, a single indivisible exchange, or the retained suffix cannot fit, the run returns `context_limit`; a summary that fails to reduce input returns `compaction_no_progress`. The host can choose a larger appropriate budget or fetch smaller tool results. Raw checkpoint storage still grows with history; this feature bounds model context, not archival storage.

## Developer Trace

Trace is owned by the harness: every run records its ordered events in a bounded in-process buffer, so no plugin, sink or host setup is needed to read recent events with `Harness::trace_events`. Older events are evicted when the buffer fills. Attach a `TraceSink` (`Registry::trace`) when the host must persist or forward them; recording stays best-effort with a bounded sink deadline, and diagnostic failures never invalidate business receipts. Conversation checkpoints remain authoritative regardless of trace persistence.

Events are handed over verbatim — model input, tool arguments and tool output. The harness ships no viewer and no storage: a host renders, stores or ships them however it likes, and owns escaping untrusted payloads in whatever medium it chooses, plus retention and locality. Model adapters must keep credentials out of `Model::metadata` and their returned continuations; hosts must filter sensitive content before emitting it.

Trace events may carry a generic correlation ID to group host scheduling/budget events with model execution. The standalone package still has no SQLite, channel, person or calendar dependency.
