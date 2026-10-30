use harness3_platform::{
    domain::*,
    messaging::{MessageAction, MessageInput, MessageQuery, Sender},
    store::Store,
};

use serde_json::{Value, json};
fn seed(store: &Store, bound: bool, count: usize, key: &str) -> Vec<String> {
    store.publish_definition(json!({"name":if bound {"messenger"} else {"worker"},"version":"1","purpose":"work","objective":"evidence","instructions":"process task data","protocol":"demo","plugins":if bound {json!([{"id":"platform.messaging","version":"1.0.0"}])}else{json!([])}})).unwrap();
    let batch = store
        .spawn(
            &Spawn {
                preset_name: if bound { "messenger" } else { "worker" }.into(),
                preset_version: "1".into(),
                count,
            },
            key,
        )
        .unwrap();
    batch["instance_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().into())
        .collect()
}
fn input(text: &str) -> MessageInput {
    MessageInput {
        text: text.into(),
        goal_id: None,
        correlation_id: None,
        causation_id: None,
    }
}
fn send(store: &Store, to: &str, text: &str, key: &str) -> String {
    store
        .send_message(Sender::Operator, to, &input(text), key)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .into()
}
fn action(action: &str) -> MessageAction {
    MessageAction {
        action: action.into(),
        note: "checked evidence".into(),
    }
}
fn code<T>(r: PResult<T>) -> String {
    match r {
        Err(e) => e.1,
        Ok(_) => panic!("expected error"),
    }
}
#[test]
fn admission_dedup_discard_fifo_and_three_distinct_delivery_states() {
    let s = Store::memory().unwrap();
    let agents = seed(&s, false, 1, "batch");
    let a = &agents[0];
    let id = send(&s, a, "你好 🧪", "same");
    assert_eq!(id, send(&s, a, "你好 🧪", "same"));
    assert_eq!(
        code(s.send_message(Sender::Operator, a, &input("changed"), "same")),
        "idempotency_conflict"
    );
    assert_eq!(s.message(&id).unwrap()["delivery_state"], "pending");
    let discarded = send(&s, a, "discard", "drop");
    s.message_action(&discarded, &action("discard"), "drop")
        .unwrap();
    assert_eq!(
        s.message(&discarded).unwrap()["resolution"],
        "checked evidence"
    );
    let next = send(&s, a, "second", "second");
    s.reconcile_messages().unwrap();
    let first = s.message(&id).unwrap();
    assert_eq!(first["delivery_state"], "delivered");
    assert_eq!(first["inbox_state"], "queued");
    assert!(!first["ack_at"].is_null());
    assert_eq!(s.message(&next).unwrap()["inbox_state"], "pending");
    assert_eq!(
        code(s.message_action(&id, &action("discard"), "too-late")),
        "message_action_state_conflict"
    );
    let work = s.claim().unwrap().unwrap();
    assert_eq!(work["id"], first["run_id"]);
    assert!(
        work["input"]
            .as_str()
            .unwrap()
            .contains("untrusted task data")
    );
    s.reconcile_messages().unwrap();
    assert_eq!(s.message(&id).unwrap()["inbox_state"], "running");
    s.finish(
        work["id"].as_str().unwrap(),
        a,
        "completed",
        json!({"output":"done"}),
    )
    .unwrap();
    s.reconcile_messages().unwrap();
    assert_eq!(s.message(&id).unwrap()["inbox_state"], "consumed");
    assert_eq!(s.message(&next).unwrap()["inbox_state"], "queued");
    let page = s
        .messages(&MessageQuery {
            limit: Some(1),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(page["has_more"], true);
    let second = s
        .messages(&MessageQuery {
            after: page["next_cursor"].as_i64().unwrap(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(second["messages"].as_array().unwrap().len(), 2);
    assert!(
        s.messages(&MessageQuery {
            direction: Some("outbox".into()),
            ..Default::default()
        })
        .is_err()
    );
}
#[test]
fn suspended_backlog_does_not_starve_ready_recipients_and_terminal_identity_is_not_retargeted() {
    let s = Store::memory().unwrap();
    let a = seed(&s, false, 3, "batch");
    s.transition(&a[0], "suspend").unwrap();
    for i in 0..40 {
        send(&s, &a[0], "waiting", &i.to_string());
    }
    let ready = send(&s, &a[1], "ready", "ready");
    let terminal = send(&s, &a[2], "retire", "retire");
    s.transition(&a[2], "terminate").unwrap();
    for _ in 0..3 {
        s.reconcile_messages().unwrap();
    }
    assert_eq!(s.message(&ready).unwrap()["inbox_state"], "queued");
    assert_eq!(
        s.message(&terminal).unwrap()["delivery_state"],
        "undeliverable"
    );
    assert_eq!(s.list("runs").unwrap().len(), 1);
    s.transition(&a[0], "resume").unwrap();
    s.reconcile_messages().unwrap();
    assert_eq!(s.list("runs").unwrap().len(), 2);
    assert_eq!(
        code(s.send_message(Sender::Operator, &a[2], &input("new"), "new")),
        "message_recipient_terminal"
    );
}
#[tokio::test]
async fn restart_marks_unknown_blocks_next_message_and_resolution_never_replays() {
    let path = std::env::temp_dir().join(format!("messages-{}.db", uuid::Uuid::new_v4()));
    let s = Store::open(&path).unwrap();
    let a = seed(&s, false, 1, "batch");
    let first = send(&s, &a[0], "first", "1");
    let next = send(&s, &a[0], "next", "2");
    s.reconcile_messages().unwrap();
    let run = s.claim().unwrap().unwrap();
    drop(s);
    let s = Store::open(&path).unwrap();
    harness3_worker::recover(&s).await.unwrap();
    assert_eq!(s.message(&first).unwrap()["inbox_state"], "unknown");
    assert_eq!(s.message(&next).unwrap()["inbox_state"], "pending");
    let resolved = s
        .message_action(&first, &action("resolve"), "resolution")
        .unwrap();
    assert_eq!(
        resolved,
        s.message_action(&first, &action("resolve"), "resolution")
            .unwrap()
    );
    s.reconcile_messages().unwrap();
    assert_eq!(s.message(&first).unwrap()["run_id"], run["id"]);
    assert_eq!(s.message(&next).unwrap()["inbox_state"], "queued");
    assert_eq!(s.list("runs").unwrap().len(), 2);
    drop(s);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn completed_run_with_unknown_tool_effect_requires_manual_resolution() {
    let s = Store::memory().unwrap();
    let a = seed(&s, false, 1, "batch");
    let first = send(&s, &a[0], "one", "1");
    let next = send(&s, &a[0], "two", "2");
    s.reconcile_messages().unwrap();
    let run = s.claim().unwrap().unwrap();
    s.finish(
        run["id"].as_str().unwrap(),
        &a[0],
        "completed",
        json!({"observations":[{"output":{"content":{"error":"plugin_outcome_unknown"}}}]}),
    )
    .unwrap();
    s.reconcile_messages().unwrap();
    assert_eq!(s.message(&first).unwrap()["inbox_state"], "unknown");
    assert_eq!(s.message(&next).unwrap()["inbox_state"], "pending");
}
#[test]
fn agent_sender_capability_scope_context_quota_and_retries_are_host_owned() {
    let s = Store::memory().unwrap();
    let a = seed(&s, true, 2, "same");
    let outside = seed(&s, false, 1, "outside");
    s.enqueue(
        &a[0],
        &RunInput {
            text: "send".into(),
        },
        "r",
    )
    .unwrap();
    let run = s.claim().unwrap().unwrap();
    let r = run["id"].as_str().unwrap();
    assert_eq!(
        code(s.send_message(Sender::AgentRun(r), &outside[0], &input("x"), "outside")),
        "message_recipient_out_of_scope"
    );
    let mut forged = input("x");
    forged.goal_id = Some("forged".into());
    assert_eq!(
        code(s.send_message(Sender::AgentRun(r), &a[1], &forged, "forged")),
        "agent_message_context_is_host_owned"
    );
    for i in 0..8 {
        s.send_message(Sender::AgentRun(r), &a[1], &input("x"), &i.to_string())
            .unwrap();
    }
    assert_eq!(
        code(s.send_message(Sender::AgentRun(r), &a[1], &input("x"), "ninth")),
        "message_run_quota_exceeded"
    );
    let messages = s
        .messages(&MessageQuery {
            instance_id: Some(a[0].clone()),
            direction: Some("outbox".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(messages["messages"][0]["sender_instance_id"], a[0]);
    assert_eq!(messages["messages"][0]["source_run_id"], r);
    s.finish(r, &a[0], "completed", json!({})).unwrap();
    assert!(
        s.send_message(Sender::AgentRun(r), &a[1], &input("x"), "0")
            .is_ok()
    );
    assert_eq!(
        code(s.send_message(Sender::AgentRun(r), &a[1], &input("x"), "new")),
        "message_source_run_not_running"
    );
    s.enqueue(
        &outside[0],
        &RunInput {
            text: "send".into(),
        },
        "r2",
    )
    .unwrap();
    let r = s.claim().unwrap().unwrap();
    assert_eq!(
        code(s.send_message(
            Sender::AgentRun(r["id"].as_str().unwrap()),
            &outside[0],
            &input("x"),
            "unbound"
        )),
        "messaging_capability_not_bound"
    );
}
#[test]
fn payload_backpressure_hop_and_correlation_limits() {
    let s = Store::memory().unwrap();
    let a = seed(&s, false, 3, "batch");
    assert_eq!(
        code(s.send_message(Sender::Operator, &a[0], &input(&"中".repeat(2667)), "large")),
        "invalid_message"
    );
    let mut previous = send(&s, &a[0], "root", "root");
    for n in 1..=8 {
        let mut v = input("chain");
        v.causation_id = Some(previous.clone());
        let receipt = s
            .send_message(Sender::Operator, &a[0], &v, &n.to_string())
            .unwrap();
        previous = receipt["id"].as_str().unwrap().into();
        assert_eq!(s.message(&previous).unwrap()["depth"], n);
    }
    let mut v = input("too deep");
    v.causation_id = Some(previous);
    assert_eq!(
        code(s.send_message(Sender::Operator, &a[0], &v, "deep")),
        "message_hop_limit"
    );
    for n in 9..64 {
        send(&s, &a[0], "fill", &n.to_string());
    }
    assert_eq!(
        code(s.send_message(Sender::Operator, &a[0], &input("overflow"), "overflow")),
        "message_backlog_full"
    );
    // Free each admission immediately to exercise the separate correlation lifetime cap.
    for n in 0..128 {
        let mut v = input("bounded chain");
        v.correlation_id = Some("fixed-root".into());
        let receipt = s
            .send_message(Sender::Operator, &a[1], &v, &n.to_string())
            .unwrap();
        s.message_action(receipt["id"].as_str().unwrap(), &action("discard"), "drop")
            .unwrap();
    }
    let mut v = input("overflow");
    v.correlation_id = Some("fixed-root".into());
    assert_eq!(
        code(s.send_message(Sender::Operator, &a[2], &v, "overflow")),
        "message_correlation_quota_exceeded"
    );
}
#[test]
fn concurrent_connections_commit_one_message_and_one_receipt() {
    let path =
        std::env::temp_dir().join(format!("messages-concurrent-{}.db", uuid::Uuid::new_v4()));
    let s = Store::open(&path).unwrap();
    let a = seed(&s, false, 1, "batch");
    drop(s);
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let p = path.clone();
            let a = a[0].clone();
            std::thread::spawn(move || {
                let s = Store::open(&p).unwrap();
                for _ in 0..30 {
                    match s.send_message(Sender::Operator, &a, &input("same"), "same") {
                        Ok(v) => return v,
                        Err(_) => std::thread::sleep(std::time::Duration::from_millis(10)),
                    }
                }
                panic!("retry exhausted")
            })
        })
        .collect();
    let results: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    assert!(results.iter().all(|v| v == &results[0]));
    let s = Store::open(&path).unwrap();
    assert_eq!(s.recent_messages().unwrap().len(), 1);
    s.reconcile_messages().unwrap();
    s.reconcile_messages().unwrap();
    assert_eq!(s.list("runs").unwrap().len(), 1);
    drop(s);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn builtin_plugin_identity_cannot_be_overridden_by_prompt_package() {
    let s = Store::memory().unwrap();
    assert_eq!(
        code(s.install(Bundle {
            id: "platform.messaging".into(),
            version: "99".into(),
            description: "override".into(),
            instructions: "x".into()
        })),
        "plugin_kind_conflict"
    );
}

#[tokio::test]
async fn responses_worker_sends_as_current_run_and_recipient_consumes_durable_envelope() {
    use axum::{Json, Router, routing::post};
    use harness3::{CancellationToken, session::SessionStore};
    use harness3_platform::models::{Models, Provider};
    use harness3_worker as runtime;
    use std::{collections::HashMap, sync::Arc, time::Duration};
    use tokio::sync::Mutex;
    let s = Store::memory().unwrap();
    let a = seed(&s, true, 2, "batch");
    // Both have the capability but the recipient is a Demo instance in this immutable fixture.
    s.publish_definition(json!({"name":"source","version":"1","extends":{"name":"messenger","version":"1"},"protocol":"openai-responses"})).unwrap();
    let source = s
        .spawn(
            &Spawn {
                preset_name: "source".into(),
                preset_version: "1".into(),
                count: 1,
            },
            "source",
        )
        .unwrap()["instance_ids"][0]
        .as_str()
        .unwrap()
        .to_owned();
    // An active Goal authorizes the cross-fleet handoff without trusting model-provided identity.
    s.create_goal(&serde_json::from_value(json!({"title":"handoff","objective":"cooperate","acceptance":"evidence","tasks":[{"key":"source","instance_id":source,"instructions":"send","depends_on":[]},{"key":"target","instance_id":a[1],"instructions":"wait","depends_on":["source"]}]})).unwrap(),"goal").unwrap();
    s.reconcile_goals().unwrap();
    let target = a[1].clone();
    let service=Router::new().route("/responses",post(move |Json(body):Json<Value>|{let target=target.clone();async move {
        if body["input"].as_array().unwrap().iter().any(|i|i["type"]=="function_call_output") {
            Json(json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"sent durable handoff"}]}]}))
        }else {
            let tool=body["tools"].as_array().unwrap().iter().find(|t|t["description"].as_str().unwrap().starts_with("Send task data")).unwrap();
            Json(json!({"status":"completed","output":[{"type":"function_call","call_id":"send-1","name":tool["name"],"arguments":json!({"recipient_id":target,"text":"检查交接凭据","key":"handoff"}).to_string()}]}))
        }
    }}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move { axum::serve(listener, service).await.unwrap() });
    let models = Models {
        responses: Some(Arc::new(
            Provider::new(
                Protocol::OpenaiResponses,
                "fixture".into(),
                format!("http://127.0.0.1:{port}"),
                String::new(),
            )
            .unwrap(),
        )),
        ..Models::default()
    };
    let stop = CancellationToken::new();
    let worker = tokio::spawn(runtime::worker_loop(
        s.clone(),
        models,
        Arc::new(Mutex::new(HashMap::new())),
        stop.clone(),
    ));
    let message = tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if let Some(m) = s.recent_messages().unwrap().first()
                && m["inbox_state"] == "consumed"
            {
                break m.clone();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    stop.cancel();
    worker.await.unwrap();
    server.abort();
    assert_eq!(message["sender_instance_id"], source);
    assert_eq!(message["recipient_id"], a[1]);
    assert_eq!(message["sender_kind"], "agent");
    assert_eq!(
        s.run(message["source_run_id"].as_str().unwrap()).unwrap()["state"],
        "completed"
    );
    let history = s.load(&a[1]).await.unwrap();
    assert!(
        serde_json::to_string(&history)
            .unwrap()
            .contains("检查交接凭据")
    );
}
