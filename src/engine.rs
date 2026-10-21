//! Bounded model/tool execution and durable session settlement.
use crate::*;
use serde_json::json;
use std::{
    collections::{BTreeSet, VecDeque},
    future::Future,
    task::Poll,
    time::Instant,
};

pub struct Harness {
    registry: Registry,
    limits: Limits,
    trace: trace::Trace,
    context_budget: Option<context::ContextBudget>,
}
impl Harness {
    fn tool_not_allowed(task: &AgentProfile, name: &str) -> Error {
        let message = if task.skills.iter().any(|skill| skill == name) {
            format!(
                "这是技能名，不是工具名。请先调用 skill_load({})，收到成功回执后再调用其提供的工具。",
                json!({"name":name})
            )
        } else if task.tools.iter().any(|tool| tool == name) {
            format!(
                "工具尚未加载。请先调用 tool_load({})，收到成功回执后再调用该工具。",
                json!({"names":[name]})
            )
        } else {
            "该名称不在当前任务的授权工具中。请调用 tool_load({}) 查看目录，再用 tool_load 加载目录中的工具；技能须用 skill_load 加载。".to_owned()
        };
        Error::new("tool_not_allowed").explain(message, Value::Null)
    }
    pub fn new(registry: Registry, limits: Limits) -> Self {
        Self {
            registry,
            limits,
            trace: trace::Trace::default(),
            context_budget: None,
        }
    }
    pub fn with_context_budget(mut self, budget: context::ContextBudget) -> Self {
        self.context_budget = Some(budget);
        self
    }
    /// Every event this harness produced, newest last. Trace is owned by the
    /// harness, so it needs no capability or host setup.
    pub fn trace_events(&self) -> Vec<trace::TraceEvent> {
        self.trace.events()
    }
    async fn emit(&self, ctx: &Context, sequence: &mut u32, kind: &str, data: Value) -> Result<()> {
        let event = Event {
            sequence: *sequence,
            kind: kind.into(),
            data,
        };
        *sequence += 1;
        self.trace(ctx, kind, event.data.clone()).await;
        for observer in &self.registry.observers {
            observer.on_event(ctx, &event).await?;
        }
        Ok(())
    }
    async fn trace(&self, ctx: &Context, kind: &str, data: Value) {
        let event = trace::TraceEvent::new(ctx, kind, data);
        self.trace.push(event.clone());
        if let Some(sink) = &self.registry.trace {
            let _ = tokio::time::timeout(Duration::from_secs(2), sink.record(event)).await;
        }
    }
    /// Execute an isolated turn. Use `run_session` for a continuing conversation.
    pub async fn run(
        &self,
        event: InboundEvent,
        model: &str,
        cancel: CancellationToken,
    ) -> Result<Report> {
        self.run_session(
            &session::MemorySessionStore::default(),
            "ephemeral",
            event,
            model,
            cancel,
        )
        .await
    }
    pub async fn run_session(
        &self,
        store: &dyn session::SessionStore,
        session_id: &str,
        event: InboundEvent,
        model: &str,
        cancel: CancellationToken,
    ) -> Result<Report> {
        if session_id.is_empty() || session_id.len() > 1024 || event.id.is_empty() {
            return Err(Error::new("input_limit"));
        }
        let request = self
            .registry
            .sources
            .get(&event.source)
            .ok_or_else(|| Error::new("unknown_source"))?
            .decode(&event)?;
        if request.text.trim().is_empty() {
            return Err(Error::new("empty_input"));
        }
        let task = self
            .registry
            .tasks
            .get(&request.task)
            .ok_or_else(|| Error::new("unknown_task"))?;
        let model_id = model.to_owned();
        let model = self
            .registry
            .models
            .get(model)
            .ok_or_else(|| Error::new("unknown_model"))?;
        let budget = self
            .context_budget
            .clone()
            .unwrap_or_else(|| model.context_budget());
        budget.validate()?;
        let model_metadata = model.metadata();
        let mut tools = task
            .tools
            .iter()
            .map(|n| {
                self.registry
                    .tools
                    .get(n)
                    .map(|t| t.spec())
                    .ok_or_else(|| Error::new("unknown_tool"))
            })
            .collect::<Result<Vec<_>>>()?;
        let progressive = self.registry.has_service("progressive-tools");
        let available_tools = tools.clone();
        if progressive {
            tools.clear();
            let initial = self.registry.service::<Vec<String>>("progressive-tools")?;
            tools.extend(
                available_tools
                    .iter()
                    .filter(|t| initial.contains(&t.name))
                    .cloned(),
            );
            tools.push(ToolSpec {
                name: "tool_load".into(),
                description: "按名称加载工具定义，每次最多 8 项。优先用 skill_load 读取相关技能并加载常用工具；仅当技能目录没有对应技能或仍缺少工具时使用。传 {} 可查看当前任务的完整授权目录，不执行业务操作。".into(),
                parameters: json!({"type":"object","properties":{"names":{"type":"array","minItems":1,"maxItems":8,"items":{"type":"string"}}},"additionalProperties":false}),
                mutates: false,
            });
        }
        if task
            .required_tool
            .as_ref()
            .is_some_and(|n| !task.tools.contains(n))
        {
            return Err(Error::new("invalid_task"));
        }
        let ctx = Context {
            run_id: uuid::Uuid::new_v4().to_string(),
            source: event.source,
            event_id: event.id,
            task: request.task,
            original_text: request.text.clone(),
            metadata: event.payload.get("context").cloned().unwrap_or(Value::Null),
        };
        let delivery = serde_json::to_string(&(&ctx.source, &ctx.event_id))
            .map_err(|_| Error::new("protocol"))?;
        let mut session = store.load(session_id).await?;
        session.ensure_supported()?;
        if session.active_run.is_some() {
            return Err(Error::new("session_busy"));
        }
        let fingerprint = event.payload.to_string();
        // A retry may follow a lost host acknowledgement. Replay any successful
        // attempt before allocating a new delivery, including successful retries.
        let retry = event.payload.get("retry").and_then(Value::as_bool) == Some(true);
        let prefix = format!("{delivery}#");
        if retry
            && let Some((_, report)) = session.completed.iter().find(|(key, report)| {
                (*key == &delivery || key.starts_with(&prefix)) && report.error.is_none()
            })
        {
            return Ok(report.clone());
        }
        let delivery = if retry && session.inputs.contains_key(&delivery) {
            let attempt = session
                .inputs
                .keys()
                .filter_map(|key| key.strip_prefix(&prefix)?.parse::<usize>().ok())
                .max()
                .unwrap_or(0)
                + 1;
            format!("{prefix}{attempt}")
        } else {
            delivery
        };
        if session
            .inputs
            .get(&delivery)
            .is_some_and(|old| old != &fingerprint)
        {
            return Err(Error::new("delivery_conflict"));
        }
        if let Some(report) = session.completed.get(&delivery) {
            return Ok(report.clone());
        }
        session.active_run = Some(ctx.run_id.clone());
        session.active_delivery = Some(delivery.clone());
        session.active_start = session.turns.len();
        session.inputs.insert(delivery.clone(), fingerprint);
        session.turns.push(Turn::User(request.text));
        if let Some(context) = event.payload.get("model_context").filter(|v| !v.is_null()) {
            session.turns.push(Turn::Context(context.clone()));
        }
        session.checkpoint(store, session_id).await?;
        let mut report = Report {
            run_id: ctx.run_id.clone(),
            output: None,
            error: None,
            observations: vec![],
            usage: None,
            generation_ms: 0,
            execution_ms: None,
        };
        let mut sequence = 0;
        let start = Instant::now();
        let mut count = 0;
        let mut seen = BTreeSet::new();
        let mut repair_tool_json = false;
        let mut tool_json_repair_attempted = false;
        let mut repair_empty_response = false;
        let mut empty_response_repair_attempted = false;
        // Only stop consecutive, identical contract failures. Other results
        // may provide new evidence or reflect transient failures, so they reset
        // this narrow guard and remain subject to the normal execution limits.
        let mut stalled_call: Option<(String, usize)> = None;
        // Token accounting across every model round of this turn: summed usage
        // plus the time actually spent inside model calls (not tool work).
        let mut saw_usage = false;
        let mut total_input = 0u64;
        let mut total_output = 0u64;
        let mut total_cached = Some(0u64);
        let mut generation = Duration::ZERO;
        // Deltas of the model call in flight, kept so an interrupted call still
        // contributes the text the user already saw.
        let live = LiveOutput::default();
        let mut live_unreported = false;
        let result = async {
            self.emit(&ctx, &mut sequence, "started", json!({"task":task.name,"session_id":session_id,"input":ctx.original_text})).await?;
            let mut instructions = task.instructions.clone();
            let mut sections = self.registry.prompts.values().collect::<Vec<_>>();
            sections.sort_by_key(|section| (section.order(), section.name()));
            for section in sections {
                let rendered = tokio::select! { biased;
                    _ = cancel.cancelled() => return Err(Error::new("cancelled")),
                    r = tokio::time::timeout(self.remaining(start)?, section.render(&ctx, cancel.clone())) => r.map_err(|_|Error::new("timeout"))??
                };
                instructions.push_str(&format!("\n\n{}", rendered));
            }

            let mut loaded_skills = Self::visible_skill_names(&session);
            if !task.skills.is_empty() {
                let catalog = task.skills.iter().map(|name| {
                    self.registry.skills.get(name).map(|s| s.info()).ok_or_else(|| Error::new("unknown_skill"))
                }).collect::<Result<Vec<_>>>()?;
                instructions.push_str("\n\n可用技能（以下名称用于 skill_load，不是业务工具）：当前工具定义中已有的查询工具可直接使用；需要其他操作时，先加载对应技能的说明和常用工具。会话中已加载的说明和工具直接复用。缺少工具且不知道名称时，调用 tool_load({}) 查看授权目录；已知名称时，调用 tool_load({\"names\":[\"工具名\"]}) 加载定义。闲聊无需加载技能。\n技能名称与用途：");
                for skill in catalog {
                    instructions.push_str(&format!("\n{}: {}", skill.name, skill.description));
                }
                tools.push(ToolSpec { name:"skill_load".into(),description:"读取相关技能，同时加载该技能的常用工具并返回其余工具目录，下一轮可直接使用；已有说明和工具无需重复加载。".into(),parameters:json!({"type":"object","properties":{"name":{"type":"string","enum":task.skills}},"required":["name"],"additionalProperties":false}),mutates:false });
            }
            // Tools this session loaded in an earlier turn stay loaded. A block
            // that resets to the starting set every turn would force the model to
            // re-load what it already has and would move the provider's cached
            // prefix at the tools boundary, discarding the whole history behind it.
            let already_loaded = Self::loaded_tool_names(&session);
            for name in already_loaded {
                if let Some(spec) = available_tools.iter().find(|t| t.name == name)
                    && !tools.iter().any(|loaded| loaded.name == spec.name) {
                    tools.push(spec.clone());
                }
            }
            for round in 0..self.limits.rounds {
                if cancel.is_cancelled() { return Err(Error::new("cancelled")); }
                self.remaining(start)?;
                let projected = session.projected_input(&instructions, &tools)?;
                let mut used = budget.used_tokens(&session, &projected, &model_id, &model_metadata)?;
                self.trace(&ctx,"context_usage",json!({"used_tokens":used,"window_tokens":budget.window_tokens,"used_percent":used as f64 * 100.0 / budget.window_tokens as f64,"budget":budget})).await;
                if used >= budget.trigger_tokens() {
                    while used > budget.target_tokens() {
                        let before = used;
                        let Some((through, mut input)) = budget.plan(&session)? else {
                            if before <= budget.input_limit() { break; }
                            return Err(Error::new("context_limit"));
                        };
                        for policy in &self.registry.policies {
                            tokio::select! { biased;
                                _=cancel.cancelled()=>return Err(Error::new("cancelled")),
                                r=tokio::time::timeout(self.remaining(start)?, policy.before_step(&ctx, &mut input))=>r.map_err(|_|Error::new("timeout"))??
                            }
                        }
                        if !input.tools.is_empty() { return Err(Error::new("policy_tool_not_allowed")); }
                        if context::estimated_tokens(&input)? > budget.input_limit() { return Err(Error::new("context_limit")); }
                        session.requests.push(session::RequestRecord {
                            purpose:Some("compaction".into()), run_id:ctx.run_id.clone(), model:model_id.clone(),
                            system:input.system.clone(), tools:vec![], history_start:0, history_end:through,
                            model_metadata:model.metadata(), compaction:session.compaction.clone(), override_input:Some(input.clone()),
                        });
                        session.checkpoint(store, session_id).await?;
                        self.trace(&ctx,"compaction_started",json!({"through":through,"before_tokens":before,"input":input})).await;
                        let internal = LiveOutput::internal().with_max_output_tokens(budget.max_summary_tokens as u64);
                        internal.set_metadata(Self::call_metadata(&session, &[], "compaction")?);
                        let began = Instant::now();
                        let outcome = tokio::select! { biased;
                            _=cancel.cancelled()=>Err(Error::new("cancelled")),
                            r=tokio::time::timeout(self.remaining(start)?, model.complete(input,&internal,cancel.clone()))=>r.map_err(|_|Error::new("timeout")).and_then(|r|r)
                        };
                        generation += began.elapsed();
                        self.trace(&ctx,"compaction_reply",json!({"reply":outcome.as_ref().ok(),"error":outcome.as_ref().err()})).await;
                        let reply = outcome?;
                        if let Some(tokens) = &reply.usage {
                            saw_usage = true;
                            total_input += tokens.input;
                            total_output += tokens.output;
                            total_cached = total_cached.zip(tokens.cached_input).map(|(total, cached)| total + cached);
                        } else { total_cached = None; }
                        if cancel.is_cancelled() { return Err(Error::new("cancelled")); }
                        let proposed = budget.proposed(through, &reply)?;
                        let next = session::Session::project(&session.turns, Some(&proposed), &instructions, &tools)?;
                        let after = context::estimated_tokens(&next)?;
                        if after >= before { return Err(Error::new("compaction_no_progress")); }
                        let previous = session.compaction.replace(proposed);
                        if let Err(error) = session.checkpoint(store, session_id).await {
                            session.compaction = previous;
                            return Err(error);
                        }
                        self.trace(&ctx,"compaction_completed",json!({"through":through,"before_tokens":before,"after_tokens_estimate":after})).await;
                        loaded_skills = Self::visible_skill_names(&session);
                        used = after;
                    }
                }
                // The wait before the first token is the longest silent stretch
                // of a turn; announce it so the surface can say what is happening.
                self.emit(&ctx,&mut sequence,"model_started",json!({"round":round+1})).await?;
                session.requests.push(session::RequestRecord {
                    purpose:None,
                    run_id:ctx.run_id.clone(), model:model_id.clone(), system:instructions.clone(),tools:tools.clone(),
                    history_start:0, history_end:session.turns.len(), model_metadata:model.metadata(), compaction:session.compaction.clone(), override_input:None,
                });
                let mut input = session.request_input(session.requests.len()-1)?;
                let original = serde_json::to_value(&input).map_err(|_|Error::new("protocol"))?;
                if repair_tool_json {
                    input.system.push_str("\n上一轮响应的工具参数不是合法 JSON，该响应中的工具均未执行。请重新生成当前步骤；工具参数必须是严格 JSON，字符串正确转义，不使用注释、尾逗号或未定义值。沿用此前成功工具回执，不重复已经完成的操作。");
                }
                if repair_empty_response {
                    input.system.push_str("\n上一轮没有返回可展示的文本或工具调用。请根据已有上下文返回答复，或调用完成任务所需的工具；此前成功的操作已完成，不要重复执行。若无法完成，明确说明原因，不要返回空响应。");
                }
                for policy in &self.registry.policies {
                    let decision = tokio::select! { biased;
                        _=cancel.cancelled()=>Err(Error::new("cancelled")),
                        r=tokio::time::timeout(self.remaining(start)?, policy.before_step(&ctx, &mut input))=>r.map_err(|_|Error::new("timeout")).and_then(|r|r)
                    };
                    self.trace(&ctx,"policy_before_step",json!({"policy":policy.name(),"error":decision.as_ref().err()})).await;
                    if let Err(error) = decision { session.requests.pop(); return Err(error); }
                }
                // Policies cannot expand the registered tool/schema authority.
                if input.tools.iter().any(|t| !tools.iter().any(|allowed| serde_json::to_value(allowed).ok() == serde_json::to_value(t).ok())) {
                    session.requests.pop();
                    return Err(Error::new("policy_tool_not_allowed"));
                }
                if budget.used_tokens(&session, &input, &model_id, &model_metadata)? > budget.input_limit() {
                    session.requests.pop();
                    return Err(Error::new("context_limit"));
                }
                if serde_json::to_value(&input).map_err(|_|Error::new("protocol"))? != original {
                    session.requests.last_mut().unwrap().override_input = Some(input);
                }
                session.checkpoint(store,session_id).await?;
                let input = session.request_input(session.requests.len()-1)?;
                live.set_metadata(Self::call_metadata(&session, &input.tools, "agent")?);
                self.trace(&ctx,"model_request",json!({"model":model.metadata(),"input":input})).await;
                let remaining = self.remaining(start)?;
                let model_started = Instant::now();
                let admitted_tools = input.tools.clone();
                live.reset();
                live_unreported = true;
                let outcome = tokio::select! { biased; _=cancel.cancelled()=>return Err(Error::new("cancelled")), r=tokio::time::timeout(remaining,model.complete(input,&live,cancel.clone()))=>r.map_err(|_| Error::new("timeout"))? };
                generation += model_started.elapsed();
                let reply = match outcome {
                    Err(error) if error.code == "malformed_tool_json" && !tool_json_repair_attempted && round + 1 < self.limits.rounds => {
                        // No reply has been admitted or dispatched yet. Retry only
                        // generation, under the same run deadline and round budget.
                        self.trace(&ctx, "model_response_rejected", json!({"error":error,"retry":true})).await;
                        repair_tool_json = true;
                        tool_json_repair_attempted = true;
                        live_unreported = false;
                        continue;
                    }
                    other => other?,
                };
                repair_tool_json = false;
                if let Some(tokens) = &reply.usage {
                    saw_usage = true;
                    total_input += tokens.input;
                    total_output += tokens.output;
                    total_cached = total_cached.zip(tokens.cached_input).map(|(total, cached)| total + cached);
                } else { total_cached = None; }
                self.trace(&ctx,"model_reply",serde_json::to_value(&reply).map_err(|_|Error::new("protocol"))?).await;
                if reply.calls.is_empty() && reply.text.trim().is_empty() {
                    live_unreported = false;
                    let retry = !empty_response_repair_attempted && round + 1 < self.limits.rounds;
                    self.trace(&ctx, "model_response_rejected", json!({"error":{"code":"empty_response"},"retry":retry})).await;
                    if !retry { return Err(Error::new("empty_response")); }
                    repair_empty_response = true;
                    empty_response_repair_attempted = true;
                    continue;
                }
                repair_empty_response = false;
                // Validate correlation before storing a provider reply. Malformed
                // call IDs cannot be paired into a valid subsequent request.
                for call in &reply.calls {
                    if call.id.is_empty() || !seen.insert(call.id.clone()) { return Err(Error::new("duplicate_call_id")); }
                }
                session.turns.push(Turn::Assistant(reply.clone()));
                live_unreported = false;
                session.checkpoint(store, session_id).await?;
                // Reasoning is carried on the completed event so a later replay
                // shows what the model was thinking, not just what it called.
                let mut completed = json!({"tools":reply.calls.iter().map(|c| &c.name).collect::<Vec<_>>(),"text":reply.text});
                if let Some(reasoning) = &reply.reasoning {
                    completed["reasoning"] = json!(reasoning);
                }
                self.emit(&ctx,&mut sequence,"model_completed",completed).await?;
                if reply.calls.is_empty() {
                    if task.required_tool.as_ref().is_some_and(|n| !report.observations.iter().any(|o| &o.call.name == n && o.output.changed)) { return Err(Error::new("required_tool_not_completed")); }
                    report.output = Some(reply.text);
                    return Ok(());
                }
                let mut repeated_failure = false;
                let mut calls = VecDeque::from(reply.calls);
                let mut pending = Vec::new();
                let batch_cancel = cancel.child_token();
                let mut batch_error = None;
                while !calls.is_empty() || !pending.is_empty() {
                    if pending.is_empty() {
                        // ponytail: fixed batches of four avoid unbounded fan-out;
                        // a sliding window can replace this if stragglers matter.
                        let capacity = self.limits.tool_calls.saturating_sub(count).min(4) as usize;
                        let batch: Vec<_> = calls.iter().take(capacity)
                            .take_while(|call| self.parallel_allowed(call, &admitted_tools, task))
                            .cloned()
                            .collect();
                        if batch.len() > 1 {
                            // Admit the entire batch before polling any tool. A
                            // failed checkpoint/observer here starts no effects.
                            for call in &batch {
                                if cancel.is_cancelled() { return Err(Error::new("cancelled")); }
                                self.remaining(start)?;
                                self.emit(&ctx,&mut sequence,"tool_started",json!({"id":call.id,"name":call.name,"arguments":call.arguments,"mutates":false})).await?;
                                session.dispatches.push(session::ToolDispatch { run_id:ctx.run_id.clone(), original:call.clone(), effective:call.clone(), denied:None });
                            }
                            session.checkpoint(store,session_id).await?;
                            for call in batch {
                                calls.pop_front();
                                count += 1;
                                pending.push(Box::pin(self.execute_call(&ctx, call, start, batch_cancel.clone())));
                            }
                        }
                    }
                    let (original_call, call, output) = if !pending.is_empty() {
                        // Poll in this task: host task-local scope stays intact.
                        // Persist each completed receipt without waiting for a
                        // slower peer, and never drop peers on a handled error.
                        let (index, (call, output)) = std::future::poll_fn(|cx| {
                            for (index, future) in pending.iter_mut().enumerate() {
                                if let Poll::Ready(result) = future.as_mut().poll(cx) {
                                    return Poll::Ready((index, result));
                                }
                            }
                            Poll::Pending
                        }).await;
                        drop(pending.swap_remove(index));
                        (call.clone(), call, output)
                    } else {
                        let call = calls.pop_front().expect("a pending call or tool future");
                        let original_call = call.clone();
                        let mut call = call;
                        let mut denied = None;
                        for policy in &self.registry.policies {
                            let decision = tokio::select! { biased;
                                _=cancel.cancelled()=>Err(Error::new("cancelled")),
                                r=tokio::time::timeout(self.remaining(start)?, policy.before_tool(&ctx, &mut call))=>r.map_err(|_|Error::new("timeout")).and_then(|r|r)
                            };
                            self.trace(&ctx,"policy_before_tool",json!({"policy":policy.name(),"original":original_call,"effective":call,"error":decision.as_ref().err()})).await;
                            if let Err(error) = decision { denied = Some(error); break; }
                        }
                        if call.id != original_call.id { denied = Some(Error::new("policy_call_id_changed")); }
                        count += 1;
                        if count > self.limits.tool_calls { return Err(Error::new("tool_limit")); }
                        if cancel.is_cancelled() { return Err(Error::new("cancelled")); }
                        self.remaining(start)?;
                        self.emit(&ctx,&mut sequence,"tool_started",json!({"id":call.id,"name":call.name,"arguments":call.arguments,"mutates":admitted_tools.iter().find(|t|t.name==call.name).map(|t|t.mutates)})).await?;
                        if !admitted_tools.iter().any(|t| t.name == call.name) {
                            denied = Some(Self::tool_not_allowed(task, &call.name));
                        }
                        session.dispatches.push(session::ToolDispatch { run_id:ctx.run_id.clone(), original:original_call.clone(), effective:call.clone(), denied:denied.clone() });
                        session.checkpoint(store,session_id).await?;
                        let output = if let Some(error) = denied {
                            ToolOutput { changed:false, content:json!({"error":error.code,"message":error.message}) }
                        } else if progressive && call.name == "tool_load" {
                            let names = call.arguments.get("names").and_then(Value::as_array);
                            if call.arguments.as_object().is_some_and(|o| o.is_empty()) {
                                ToolOutput { content:json!({"available":Self::tool_catalog(&available_tools)}), changed:false }
                            } else if call.arguments.as_object().is_none_or(|o| o.len()!=1) || names.is_none_or(|names| names.is_empty() || names.len()>8 || names.iter().any(|n| n.as_str().is_none_or(|n| !task.tools.iter().any(|t|t==n)))) {
                                let invalid_name = names.and_then(|names| names.iter().filter_map(Value::as_str).find(|name| !task.tools.iter().any(|tool| tool == name)));
                                let error = Self::tool_not_allowed(task, invalid_name.unwrap_or(""));
                                ToolOutput { content:json!({"error":error.code,"message":error.message,"hint":"本批次未加载任何工具。names 必须包含 1–8 个授权工具名；查看目录用 tool_load({})。"}), changed:false }
                            } else {
                                let selected: Vec<_> = available_tools.iter().filter(|t| names.unwrap().iter().any(|n| n.as_str()==Some(t.name.as_str()))).cloned().collect();
                                for spec in &selected {
                                    if !tools.iter().any(|t| t.name==spec.name) { tools.push(spec.clone()); }
                                }
                                ToolOutput {content:json!({"loaded":selected.iter().map(|t| &t.name).collect::<Vec<_>>()}),changed:false}
                            }
                        } else if call.name == "skill_load" && !task.skills.is_empty() {
                            let name=call.arguments.get("name").and_then(Value::as_str).unwrap_or("");
                            if !task.skills.iter().any(|allowed| allowed==name) {
                                ToolOutput{content:json!({"error":"unknown_skill","message":"Choose a skill from the available catalog."}),changed:false}
                            } else if loaded_skills.contains(name) {
                                ToolOutput{content:json!({"name":name,"already_loaded":true}),changed:false}
                            } else {
                                let provider=self.registry.skills.get(name).ok_or_else(||Error::new("unknown_skill"))?;
                                let loaded=tokio::select!{biased;_=cancel.cancelled()=>return Err(Error::new("cancelled")),r=tokio::time::timeout(self.remaining(start)?,provider.load(cancel.clone()))=>r.map_err(|_|Error::new("timeout"))??};
                                let initial = provider.initial_tools();
                                let selected: Vec<_> = available_tools.iter().filter(|t| initial.contains(&t.name)).cloned().collect();
                                for spec in &selected {
                                    if !tools.iter().any(|t| t.name == spec.name) { tools.push(spec.clone()); }
                                }
                                let related = provider.tools();
                                let remaining: Vec<_> = available_tools.iter().filter(|t| related.contains(&t.name) && !tools.iter().any(|loaded| loaded.name == t.name)).cloned().collect();
                                loaded_skills.insert(name.to_owned());
                                self.emit(&ctx,&mut sequence,"skill_loaded",json!({"name":loaded.info.name,"revision":loaded.revision})).await?;
                                ToolOutput{content:json!({"name":loaded.info.name,"instructions":loaded.instructions,"loaded":selected.iter().map(|t| &t.name).collect::<Vec<_>>(),"available":Self::tool_catalog(&remaining)}),changed:false}
                            }
                        } else if !task.tools.contains(&call.name) || (progressive && !tools.iter().any(|t|t.name==call.name)) {
                            let error = Self::tool_not_allowed(task, &call.name);
                            ToolOutput { content: json!({"error":error.code,"message":error.message}), changed:false }
                        } else {
                            self.execute_call(&ctx, call.clone(), start, cancel.clone()).await.1
                        };
                        (original_call, call, output)
                    };
                    let contract_error = output.content.get("error").and_then(Value::as_str)
                        .is_some_and(|code| matches!(code.split(':').next().unwrap_or(code).trim(),
                            "invalid_arguments" | "tool_not_allowed" | "unknown_skill"));
                    let redundant_load = call.name == "skill_load"
                        && output.content.get("already_loaded").and_then(Value::as_bool) == Some(true);
                    if !output.changed && (contract_error || redundant_load) {
                        let key = json!([call.name, call.arguments, output.content]).to_string();
                        let attempts = match &stalled_call {
                            Some((previous, attempts)) if previous == &key => attempts + 1,
                            _ => 1,
                        };
                        repeated_failure = attempts >= 3;
                        stalled_call = Some((key, attempts));
                    } else {
                        stalled_call = None;
                        repeated_failure = false;
                    }
                    let observation = Observation { call: original_call, output };
                    report.observations.push(observation.clone());
                    session.turns.push(Turn::Tools(vec![observation.clone()]));
                    let recorded = async {
                        session.checkpoint(store,session_id).await?;
                        for policy in &self.registry.policies {
                            tokio::select! { biased;
                                _=cancel.cancelled()=>return Err(Error::new("cancelled")),
                                r=tokio::time::timeout(self.remaining(start)?, policy.after_tool(&ctx, &observation))=>r.map_err(|_|Error::new("timeout"))??
                            }
                        }
                        self.emit(&ctx,&mut sequence,"tool_completed",serde_json::to_value(&observation).map_err(|_| Error::new("protocol"))?).await?;
                        Ok::<_, Error>(())
                    }.await;
                    if let Err(error) = recorded {
                        batch_error.get_or_insert(error);
                        batch_cancel.cancel();
                    }
                    if pending.is_empty() && let Some(error) = batch_error.take() {
                        return Err(error);
                    }
                }
                // Settle every call in the batch before stopping, so persisted
                // history remains replayable and completed effects retain receipts.
                if repeated_failure {
                    return Err(Error::new("tool_limit").explain("连续三次相同调用均存在参数或工具契约错误，或重复加载已加载技能，本轮已停止；已成功操作保留，其余目标尚未完成。", Value::Null));
                }
            }
            Err(Error::new("round_limit"))
        }.await;
        report.error = result.err();
        if saw_usage {
            report.usage = Some(TokenUsage {
                input: total_input,
                output: total_output,
                cached_input: total_cached,
            });
        }
        report.generation_ms = u64::try_from(generation.as_millis()).unwrap_or(u64::MAX);
        session.settle_pending("not_completed_outcome_unknown");
        // An interrupted model call still streamed text the user may already
        // have read. Keep that prefix as the assistant's turn so the next
        // request neither contradicts nor forgets it; the failure stays in the
        // report and no tool call is invented. Settling first keeps every tool
        // result adjacent to the call it answers.
        if live_unreported && let Some(partial) = live.unfinished() {
            session.turns.push(Turn::Assistant(partial));
        }
        report.execution_ms = Some(u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX));
        session.active_run = None;
        session.active_delivery = None;
        session.completed.insert(delivery, report.clone());
        if let Err(error) = session.checkpoint(store, session_id).await {
            report.error.get_or_insert(error);
        }
        if let Err(error) = self
            .emit(
                &ctx,
                &mut sequence,
                "finished",
                json!({"error":report.error,"output":report.output}),
            )
            .await
        {
            report.error.get_or_insert(error);
        }
        Ok(report)
    }
    fn parallel_allowed(&self, call: &Call, admitted: &[ToolSpec], task: &AgentProfile) -> bool {
        // Policies may depend on the previous receipt or rewrite a read into a
        // write. Preserve their sequential contract rather than speculate.
        self.registry.policies.is_empty()
            && !matches!(call.name.as_str(), "skill_load" | "tool_load")
            && task.tools.contains(&call.name)
            && admitted
                .iter()
                .any(|spec| spec.name == call.name && !spec.mutates)
            && self
                .registry
                .tools
                .get(&call.name)
                .is_some_and(|tool| !tool.spec().mutates && tool.parallel_safe())
    }
    async fn execute_call(
        &self,
        ctx: &Context,
        call: Call,
        start: Instant,
        cancel: CancellationToken,
    ) -> (Call, ToolOutput) {
        let result = async {
            if cancel.is_cancelled() {
                return Err(Error::new("cancelled"));
            }
            let tool = self
                .registry
                .tools
                .get(&call.name)
                .ok_or_else(|| Error::new("unknown_tool"))?;
            let child = cancel.child_token();
            let deadline = self.remaining(start)?;
            let future = tool.execute(ctx, call.arguments.clone(), child.clone());
            tokio::pin!(future);
            // A deadline signals cancellation but still drains started tools.
            tokio::select! {
                r = &mut future => r,
                _ = tokio::time::sleep(deadline) => { child.cancel(); future.await }
            }
        }
        .await;
        let output = match result {
            Ok(output) => output,
            Err(error) => ToolOutput {
                content: serde_json::json!({"error":error.code,"message":error.message,"details":error.details}),
                changed: false,
            },
        };
        (call, output)
    }
    fn remaining(&self, start: Instant) -> Result<Duration> {
        self.limits
            .timeout
            .checked_sub(start.elapsed())
            .ok_or_else(|| Error::new("timeout"))
    }
    fn tool_catalog(tools: &[ToolSpec]) -> String {
        tools
            .iter()
            .map(|t| {
                format!(
                    "{} [{}]: {}",
                    t.name,
                    if t.mutates { "write" } else { "read" },
                    t.description
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    fn visible_skill_names(session: &session::Session) -> BTreeSet<String> {
        let start = session.compaction.as_ref().map_or(0, |c| c.through);
        session.turns[start..]
            .iter()
            .filter_map(|turn| {
                if let Turn::Tools(items) = turn {
                    Some(items)
                } else {
                    None
                }
            })
            .flatten()
            .filter(|item| {
                item.call.name == "skill_load"
                    && item
                        .output
                        .content
                        .get("instructions")
                        .and_then(Value::as_str)
                        .is_some()
            })
            .filter_map(|item| {
                item.output
                    .content
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .collect()
    }
    fn call_metadata(
        session: &session::Session,
        tools: &[ToolSpec],
        purpose: &str,
    ) -> Result<Value> {
        use sha2::{Digest, Sha256};
        let previous = session
            .requests
            .iter()
            .rev()
            .skip(1)
            .find(|r| r.purpose.is_none());
        let through = session.compaction.as_ref().map_or(0, |c| c.through);
        let previous_tools = previous.map(|r| {
            r.override_input
                .as_ref()
                .map_or(r.tools.as_slice(), |i| i.tools.as_slice())
        });
        let added = previous_tools.map_or(tools.len(), |old| {
            tools
                .iter()
                .filter(|tool| !old.iter().any(|old| old.name == tool.name))
                .count()
        });
        let encoded = serde_json::to_vec(tools).map_err(|_| Error::new("protocol"))?;
        let changed = previous_tools
            .is_some_and(|tools| serde_json::to_vec(tools).ok().as_ref() != Some(&encoded));
        let phase = if purpose == "compaction" {
            "compaction"
        } else if previous.is_none() {
            "cold_start"
        } else if previous
            .unwrap()
            .compaction
            .as_ref()
            .map_or(0, |c| c.through)
            != through
        {
            "post_compaction"
        } else if changed {
            "tools_changed"
        } else {
            "continuation"
        };
        let compaction_generation = session
            .events
            .iter()
            .filter_map(|e| e.compaction.as_ref().map(|c| c.through))
            .collect::<BTreeSet<_>>()
            .len();
        Ok(
            json!({"purpose":purpose,"cache_phase":phase,"tools_added":added,"tools_changed":changed,
            "tool_count":tools.len(),"tool_schema_bytes":encoded.len(),"tool_fingerprint":format!("{:x}",Sha256::digest(&encoded)),
            "compaction_generation":compaction_generation,"compaction_through":through}),
        )
    }
    /// Tool names successfully loaded through tool_load or skill_load. Derived from
    /// history, so no extra checkpoint field and no risk of the two disagreeing.
    fn loaded_tool_names(session: &session::Session) -> Vec<String> {
        let mut names = Vec::new();
        for turn in &session.turns {
            let Turn::Tools(results) = turn else {
                continue;
            };
            for result in results {
                if matches!(result.call.name.as_str(), "tool_load" | "skill_load")
                    && result.output.content.get("error").is_none()
                    && let Some(list) = result.output.content["loaded"].as_array()
                {
                    for name in list.iter().filter_map(|n| n.as_str()) {
                        if !names.iter().any(|old| old == name) {
                            names.push(name.to_owned());
                        }
                    }
                }
            }
        }
        names
    }
}
