use super::*;
pub struct DemoModel;
#[async_trait::async_trait]
impl Model for DemoModel {
    fn metadata(&self) -> Value {
        json!({"protocol":"demo","deterministic":true})
    }
    async fn complete(
        &self,
        input: ModelInput,
        _live: &LiveOutput,
        cancel: CancellationToken,
    ) -> Result<ModelReply> {
        // A small interruptible delay makes the real execution phases inspectable.
        tokio::select! {_=cancel.cancelled()=>return Err(Error::new("cancelled")),_=tokio::time::sleep(Duration::from_millis(500))=>{}}
        if input.tools.iter().any(|s| s.name == "platform.board.claim") {
            return demo_board(&input);
        }
        if let Some(spec) = input
            .tools
            .iter()
            .find(|s| s.name == "platform.verification.report")
        {
            let previous = if let Some(Turn::Tools(items)) = input.turns.last() {
                items.first()
            } else {
                None
            };
            let ids=spec.parameters["properties"]["checks"]["items"]["properties"]["artifact_ids"]["items"]["enum"].as_array().unwrap();
            let keys =
                spec.parameters["properties"]["checks"]["items"]["properties"]["key"]["enum"]
                    .as_array()
                    .unwrap();
            let evidence_pages: Vec<_> = input
                .turns
                .iter()
                .rev()
                .take_while(|turn| matches!(turn, Turn::Tools(_)))
                .filter_map(|turn| {
                    if let Turn::Tools(items) = turn {
                        Some(items)
                    } else {
                        None
                    }
                })
                .flatten()
                .collect();
            let (text, calls) = match previous {
                Some(o) if o.call.name == "platform.verification.report" => (
                    "[确定性演示] 已提交逐项验证回执；只验证平台读证据与评审流程，不判断语义质量。"
                        .into(),
                    vec![],
                ),
                Some(o) if o.call.name == "platform.verification.evidence" => (
                    String::new(),
                    if evidence_pages
                        .iter()
                        .any(|o| o.output.content["complete"] == false)
                    {
                        evidence_pages.iter().filter(|o|o.output.content["complete"]==false).map(|o|Call {
                            id: format!("demo-evidence-{}-{}",o.output.content["artifact_id"].as_str().unwrap(),o.output.content["next_offset"]),
                            name:"platform.verification.evidence".into(),
                            arguments:json!({"artifact_id":o.output.content["artifact_id"],"offset":o.output.content["next_offset"],"limit":12000}),
                        }).collect()
                    } else {
                        vec![Call {
                            id: "demo-verdict".into(),
                            name: spec.name.clone(),
                            arguments: json!({"summary":"确定性演示：已读取冻结产物，验证控制链路，不证明内容正确。","checks":keys.iter().map(|key|json!({"key":key,"status":"passed","reason":"Demo 仅确认产物可读取，语义正确性需要真实模型或人工核查。","artifact_ids":ids})).collect::<Vec<_>>()}),
                        }]
                    },
                ),
                _ => (
                    String::new(),
                    ids.iter()
                        .enumerate()
                        .map(|(n, id)| Call {
                            id: format!("demo-evidence-{n}"),
                            name: "platform.verification.evidence".into(),
                            arguments: json!({"artifact_id":id,"offset":0,"limit":12000}),
                        })
                        .collect(),
                ),
            };
            return Ok(ModelReply {
                text,
                calls,
                usage: None,
                continuation: Value::Null,
                reasoning: None,
            });
        }
        if let Some(spec) = input
            .tools
            .iter()
            .find(|s| s.name == "platform.research.plan")
        {
            let finished = matches!(input.turns.last(), Some(Turn::Tools(_)));
            let count = spec.parameters["properties"]["tasks"]["minItems"]
                .as_u64()
                .unwrap() as usize;
            return Ok(ModelReply {
                text: if finished {
                    "[确定性演示] 分工计划已提交；研究任务由平台派发。".into()
                } else {
                    String::new()
                },
                calls: if finished {
                    vec![]
                } else {
                    vec![Call {
                        id: "demo-plan".into(),
                        name: spec.name.clone(),
                        arguments: json!({"tasks":(0..count).map(|i|json!({"title":format!("演示维度 {}",i+1),"instructions":format!("确定性演示：检查第 {} 个维度的输入文本，报告工具统计；这不是语义调研。",i+1)})).collect::<Vec<_>>(),"synthesis":"确定性演示：检查上游贡献文本；不宣称执行真实调研。"}),
                    }]
                },
                usage: None,
                continuation: Value::Null,
                reasoning: None,
            });
        }
        if input
            .tools
            .iter()
            .any(|s| s.name == "platform.workspace.write")
        {
            return demo_team(&input);
        }
        if input
            .tools
            .iter()
            .any(|s| s.name == "platform.delegation.read")
        {
            let previous = if let Some(Turn::Tools(items)) = input.turns.last() {
                items.first()
            } else {
                None
            };
            let (text, calls) = match previous {
                Some(o) if o.call.name == "platform.delegation.submit" => (
                    "[确定性演示] 已提交分工，结束本次规划执行；原任务等待子 Agent 贡献后继续。"
                        .into(),
                    vec![],
                ),
                Some(o)
                    if o.call.name == "platform.delegation.read"
                        && o.output.content["can_delegate"] == true =>
                {
                    let count = o.output.content["remaining_children"]
                        .as_u64()
                        .unwrap_or(0)
                        .min(2);
                    let role = o.output.content["roles"][0]["name"]
                        .as_str()
                        .ok_or_else(|| Error::new("missing_delegation_role"))?;
                    (
                        String::new(),
                        vec![Call {
                            id: "demo-delegate".into(),
                            name: "platform.delegation.submit".into(),
                            arguments: json!({"tasks":(0..count).map(|i|json!({"role":role,"title":format!("演示子任务 {}",i+1),"instructions":"确定性演示：按允许深度继续分工，抵达上限后完成本项检查；不进行真实研究。"})).collect::<Vec<_>>(),"continuation":"检查所有子任务贡献及 Run 来源，完成原任务；不得将规划交接文本作为最终成果。"}),
                        }],
                    )
                }
                Some(o) if o.call.name == "platform.delegation.read" => (
                    format!(
                        "[确定性演示] 任务 {} 在深度 {} 完成。委派状态：{}。本次执行验证真实调度、依赖贡献和原 Agent 续作，不进行语义研究。",
                        o.output.content["task_key"].as_str().unwrap_or("当前任务"),
                        o.output.content["depth"].as_u64().unwrap_or(0),
                        o.output.content["reason"].as_str().unwrap_or("not_enabled")
                    ),
                    vec![],
                ),
                _ => (
                    String::new(),
                    vec![Call {
                        id: "demo-delegation-read".into(),
                        name: "platform.delegation.read".into(),
                        arguments: json!({}),
                    }],
                ),
            };
            return Ok(ModelReply {
                text,
                calls,
                usage: None,
                continuation: Value::Null,
                reasoning: None,
            });
        }
        if input
            .tools
            .iter()
            .any(|s| s.name == "platform.context.read")
        {
            let previous = if let Some(Turn::Tools(observations)) = input.turns.last() {
                observations.first()
            } else {
                None
            };
            let (text, calls) = match previous {
                Some(o)
                    if o.call.name == "platform.context.read"
                        && o.call.id == "demo-context-refresh" =>
                {
                    (
                        format!(
                            "[确定性演示 / Demo] 已通过共享上下文工具发布独立记录，并再次读取目标共享区。当前版本 {}，共 {} 条记录。该演示验证多实例读写链路，不执行语义研究。",
                            o.output.content["revision"],
                            o.output.content["entries"].as_array().map_or(0, Vec::len)
                        ),
                        vec![],
                    )
                }
                Some(o) if o.call.name == "platform.context.read" => {
                    let run = o.output.content["source_run_id"]
                        .as_str()
                        .ok_or_else(|| Error::new("context_scope_unavailable"))?;
                    (
                        String::new(),
                        vec![Call {
                            id: "demo-context-publish".into(),
                            name: "platform.context.publish".into(),
                            arguments: json!({"key":format!("demo.{run}"),"expected_revision":0,"title":"确定性演示贡献","body":"此记录由当前任务的实际 Agent 发布，用于验证共享上下文；不代表语义研究或事实验证。","kind":"observation","command_key":"demo-publish"}),
                        }],
                    )
                }
                Some(o) if o.call.name == "platform.context.publish" => (
                    String::new(),
                    vec![Call {
                        id: "demo-context-refresh".into(),
                        name: "platform.context.read".into(),
                        arguments: json!({}),
                    }],
                ),
                _ => (
                    String::new(),
                    vec![Call {
                        id: "demo-context-read".into(),
                        name: "platform.context.read".into(),
                        arguments: json!({}),
                    }],
                ),
            };
            return Ok(ModelReply {
                text,
                calls,
                usage: None,
                continuation: Value::Null,
                reasoning: None,
            });
        }
        let (text, calls) = if let Some(Turn::Tools(observations)) = input.turns.last() {
            let evidence = observations
                .first()
                .ok_or_else(|| Error::new("missing_tool_result"))?;
            (
                format!(
                    "[确定性演示 / Demo]\n已通过 text.inspect 检查输入。\n\n字符数：{}\n行数：{}\nUTF-8 字节数：{}\n\n这是一次真实 Harness 工具循环；演示模型不执行语义推理；任务依赖调度与贡献传递由平台负责。",
                    evidence.output.content["characters"],
                    evidence.output.content["lines"],
                    evidence.output.content["bytes"]
                ),
                vec![],
            )
        } else {
            (
                String::new(),
                vec![Call {
                    id: "inspect-1".into(),
                    name: "text.inspect".into(),
                    arguments: json!({"text":input.turns.iter().rev().find_map(|turn|if let Turn::User(text)=turn{Some(text.as_str())}else{None}).unwrap_or(&input.text)}),
                }],
            )
        };
        Ok(ModelReply {
            text,
            calls,
            usage: None,
            continuation: Value::Null,
            reasoning: None,
        })
    }
}
fn demo_team(input: &ModelInput) -> Result<ModelReply> {
    let previous = match input.turns.last() {
        Some(Turn::Tools(items)) => items.first(),
        _ => None,
    };
    let has = |name: &str| input.tools.iter().any(|s| s.name == name);
    let call = |id: &str, name: &str, arguments: Value| Call {
        id: id.into(),
        name: name.into(),
        arguments,
    };
    let write = || {
        let evidence = input
            .turns
            .iter()
            .filter_map(|turn| match turn {
                Turn::Tools(items) => Some(
                    items
                        .iter()
                        .map(|o| json!({"tool":o.call.name,"receipt":o.output.content}))
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .flatten()
            .collect::<Vec<_>>();
        call(
            "demo-file-write",
            "platform.workspace.write",
            json!({"path":"evidence.md","content":format!("# 确定性团队演示\n\n本文件验证真实工具、同目标文件共享和来源追踪，不执行语义研究。\n\n## 任务输入\n{}\n\n## 已读取回执\n{}",input.text,serde_json::to_string_pretty(&evidence).unwrap_or_default()),"command_key":"demo-write"}),
        )
    };
    let (text, calls) = match previous.map(|o| o.call.name.as_str()) {
        None if has("platform.tasks.board") => (
            String::new(),
            vec![call("demo-board", "platform.tasks.board", json!({}))],
        ),
        None | Some("platform.tasks.board") if has("platform.channels.read") => (
            String::new(),
            vec![call("demo-channels", "platform.channels.read", json!({}))],
        ),
        Some("platform.channels.read") => {
            let channel = previous
                .and_then(|o| o.output.content["channels"].as_array())
                .and_then(|c| c.first());
            if let Some(channel) = channel {
                (
                    String::new(),
                    vec![call(
                        "demo-post",
                        "platform.channels.post",
                        json!({"channel_id":channel["id"],"text":"[确定性演示] 当前 Agent 已读取任务板，接下来生成并发布文件与 JSON 证据。此消息不表示真实调研完成。","wake_subscribers":false,"key":"demo-team-post"}),
                    )],
                )
            } else {
                (
                    String::new(),
                    vec![call(
                        "demo-artifacts",
                        "platform.workspace.artifacts",
                        json!({}),
                    )],
                )
            }
        }
        Some("platform.channels.post") => (
            String::new(),
            vec![call(
                "demo-artifacts",
                "platform.workspace.artifacts",
                json!({}),
            )],
        ),
        Some("platform.workspace.artifacts") => {
            let peer = previous
                .and_then(|o| o.output.content["artifacts"].as_array())
                .and_then(|items| items.iter().find(|a| a["source_state"] == "completed"));
            (
                String::new(),
                vec![if let Some(peer) = peer {
                    call(
                        "demo-peer-read",
                        "platform.workspace.read_artifact",
                        json!({"artifact_id":peer["id"]}),
                    )
                } else {
                    write()
                }],
            )
        }
        Some("platform.workspace.read_artifact") => (String::new(), vec![write()]),
        Some("platform.workspace.write") => (
            String::new(),
            vec![call(
                "demo-file-publish",
                "platform.workspace.publish",
                json!({"name":"团队演示文件","path":"evidence.md","command_key":"demo-file"}),
            )],
        ),
        Some("platform.workspace.publish")
            if previous.is_some_and(|o| o.call.id == "demo-file-publish") =>
        {
            (
                String::new(),
                vec![call(
                    "demo-json-publish",
                    "platform.workspace.publish",
                    json!({"name":"团队演示结构化记录","value":{"demo":true,"semantic_research":false,"file_artifact_id":previous.unwrap().output.content["id"]},"command_key":"demo-json"}),
                )],
            )
        }
        Some("platform.workspace.publish") if has("platform.context.read") => (
            String::new(),
            vec![call(
                "demo-team-context",
                "platform.context.read",
                json!({}),
            )],
        ),
        Some("platform.context.read") => {
            let run = previous.unwrap().output.content["source_run_id"]
                .as_str()
                .ok_or_else(|| Error::new("context_scope_unavailable"))?;
            let publications = input
                .turns
                .iter()
                .filter_map(|turn| {
                    if let Turn::Tools(items) = turn {
                        Some(items)
                    } else {
                        None
                    }
                })
                .flatten()
                .filter(|o| o.call.name == "platform.workspace.publish")
                .map(|o| o.output.content["id"].clone())
                .collect::<Vec<_>>();
            (
                String::new(),
                vec![call(
                    "demo-team-context-publish",
                    "platform.context.publish",
                    json!({"key":format!("demo.{run}"),"expected_revision":0,"title":"已发布团队演示文件与 JSON","body":format!("确定性演示发布了快照：{}。这些回执验证协作链路，不证明语义质量。",json!(publications)),"kind":"observation","command_key":"demo-team-context"}),
                )],
            )
        }
        _ if previous.is_none() => (
            String::new(),
            vec![call(
                "demo-artifacts",
                "platform.workspace.artifacts",
                json!({}),
            )],
        ),
        _ => {
            let ids = input
                .turns
                .iter()
                .filter_map(|turn| {
                    if let Turn::Tools(items) = turn {
                        Some(items)
                    } else {
                        None
                    }
                })
                .flatten()
                .filter(|o| o.call.name == "platform.workspace.publish")
                .map(|o| o.output.content["id"].as_str().unwrap_or("missing"))
                .collect::<Vec<_>>();
            (
                format!(
                    "[确定性团队演示] 已读取共享证据并发布文件和 JSON 快照：{}。频道发言、工具回执和来源 Run 可在控制台检查。本次验证平台协作链路，不执行语义研究或外部事实核验。",
                    ids.join("、")
                ),
                vec![],
            )
        }
    };
    Ok(ModelReply {
        text,
        calls,
        usage: None,
        continuation: Value::Null,
        reasoning: None,
    })
}
fn demo_board(input: &ModelInput) -> Result<ModelReply> {
    let observations: Vec<_> = input
        .turns
        .iter()
        .filter_map(|turn| match turn {
            Turn::Tools(items) => Some(items),
            _ => None,
        })
        .flatten()
        .collect();
    let read = observations
        .iter()
        .rev()
        .find(|o| o.call.name == "platform.board.read")
        .map(|o| &o.output.content);
    let claimed = observations
        .iter()
        .rev()
        .find(|o| o.call.name == "platform.board.claim")
        .map(|o| &o.output.content);
    let mut text = String::new();
    let next = if let Some(task) = claimed {
        let children: Vec<_> = read
            .and_then(|b| b["tasks"].as_array())
            .into_iter()
            .flatten()
            .filter(|t| t["parent_id"] == task["id"])
            .collect();
        if observations
            .iter()
            .any(|o| o.call.name == "platform.board.split")
        {
            text = "[确定性演示] 子任务已回投任务板，等待控制面选型和同级实例认领。".into();
            None
        } else if task["depth"] == 0 && children.is_empty() {
            Some((
                "platform.board.split",
                json!({"command_key":"demo-split","tasks":[
                    {"title":"输入结构检查","instructions":format!("统计以下输入的字符、行和字节：{}",task["instructions"].as_str().unwrap_or_default()),"acceptance":"给出 text.inspect 的实际回执"},
                    {"title":"验收文本检查","instructions":format!("统计以下验收描述的字符、行和字节：{}",task["acceptance"].as_str().unwrap_or_default()),"acceptance":"给出 text.inspect 的实际回执"}
                ]}),
            ))
        } else if let Some(inspect) = observations.iter().find(|o| o.call.name == "text.inspect") {
            text = format!(
                "[确定性演示] 工具统计：{}；已读取 {} 个子任务贡献。演示验证任务板闭环，不代表完成语义研究。",
                inspect.output.content,
                children.len()
            );
            None
        } else {
            Some((
                "text.inspect",
                json!({"text":if children.is_empty(){task["instructions"].as_str().unwrap_or_default().to_owned()}else{serde_json::to_string(&children).unwrap_or_default()}}),
            ))
        }
    } else if let Some(board) = read {
        let scope = &board["scope"];
        let eligible = board["tasks"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|t| {
                t["state"] == "pending"
                    && t["route_state"] == "provisioned"
                    && t["preset_name"] == scope["preset_name"]
                    && t["preset_version"] == scope["preset_version"]
            })
            .collect::<Vec<_>>();
        let task = eligible
            .iter()
            .find(|t| t["id"] == scope["target_task_id"])
            .or_else(|| eligible.first());
        match task {
            Some(task) => Some(("platform.board.claim", json!({"task_id":task["id"]}))),
            None => {
                text = "[确定性演示] 当前无可认领任务。".into();
                None
            }
        }
    } else {
        Some(("platform.board.read", json!({})))
    };
    Ok(ModelReply {
        text,
        calls: next
            .into_iter()
            .map(|(name, arguments)| Call {
                id: format!("demo-board-{}", observations.len()),
                name: name.into(),
                arguments,
            })
            .collect(),
        usage: None,
        continuation: Value::Null,
        reasoning: None,
    })
}
