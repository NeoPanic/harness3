use harness3_platform::{
    collaboration::{Author, ContextInput},
    domain::*,
    store::Store,
};
use serde_json::{Value, json};
fn setup(bound: bool) -> (Store, String) {
    let s = Store::memory().unwrap();
    s.publish_definition(json!({"name":"team","version":"1","purpose":"shared evidence","objective":"compare","instructions":"Read and reconcile shared findings.","protocol":"demo","plugins":if bound{json!([{"id":"platform.context","version":"1.0.0"},{"id":"platform.messaging","version":"1.0.0"}])}else{json!([])}})).unwrap();
    let fleet = s
        .spawn(
            &Spawn {
                preset_name: "team".into(),
                preset_version: "1".into(),
                count: 3,
            },
            "agents",
        )
        .unwrap();
    let goal = s
        .create_goal(
            &GoalPlan {
                verification: None,
                title: "joint findings".into(),
                objective: "compare evidence".into(),
                acceptance: "check facts and questions".into(),
                tasks: (0..3)
                    .map(|i| TaskPlan {
                        key: format!("task-{i}"),
                        instance_id: fleet["instance_ids"][i].as_str().unwrap().into(),
                        instructions: format!("work {i}"),
                        depends_on: if i == 2 {
                            vec!["task-0".into(), "task-1".into()]
                        } else {
                            vec![]
                        },
                        ..Default::default()
                    })
                    .collect(),
            },
            "goal",
        )
        .unwrap();
    (s, goal["id"].as_str().unwrap().into())
}
fn entry(key: &str, revision: i64) -> ContextInput {
    ContextInput {
        key: key.into(),
        expected_revision: revision,
        title: "Shared finding".into(),
        body: "evidence with uncertainty".into(),
        kind: "observation".into(),
        status: "active".into(),
        artifact_ids: vec![],
    }
}
fn finish(s: &Store, w: &Value) {
    s.finish(
        w["id"].as_str().unwrap(),
        w["instance_id"].as_str().unwrap(),
        "completed",
        json!({"output":"a settled contribution"}),
    )
    .unwrap();
}
#[test]
fn shared_writers_use_host_identity_and_revision_conflicts_preserve_both_histories() {
    let (s, g) = setup(true);
    let first = s
        .publish_context(Author::Operator, Some(&g), &entry("finding", 0), "initial")
        .unwrap();
    s.reconcile_goals().unwrap();
    let a = s.claim().unwrap().unwrap();
    let b = s.claim().unwrap().unwrap();
    let ar = a["id"].as_str().unwrap();
    let br = b["id"].as_str().unwrap();
    assert_eq!(s.run_context(ar).unwrap()["revision"], 1);
    let receipt = s
        .publish_context(Author::AgentRun(ar), None, &entry("finding", 1), "update")
        .unwrap();
    assert_eq!(receipt["revision"], 2);
    assert_eq!(
        s.publish_context(Author::AgentRun(ar), None, &entry("finding", 1), "update")
            .unwrap(),
        receipt
    );
    assert_eq!(
        s.publish_context(Author::AgentRun(br), None, &entry("finding", 1), "racing")
            .unwrap_err()
            .1,
        "context_revision_conflict"
    );
    let board = s.run_context(br).unwrap();
    assert_eq!(board["entries"][0]["source_instance_id"], a["instance_id"]);
    assert_eq!(board["entries"][0]["source_run_id"], a["id"]);
    assert_eq!(board["entries"][0]["source_state"], "running");
    let history = s.context_history(&g, "finding").unwrap();
    assert_eq!(history["versions"][0]["author_kind"], "operator");
    assert_eq!(history["versions"][1]["author_kind"], "agent");
    assert_eq!(history["versions"].as_array().unwrap().len(), 2);
    assert_eq!(first["revision"], 1);
    finish(&s, &a);
    assert_eq!(
        s.run_context(ar).unwrap_err().1,
        "context_run_not_authorized"
    );
    assert!(
        s.publish_context(Author::AgentRun(ar), None, &entry("late", 0), "late")
            .is_err()
    );
    assert_eq!(
        s.run_context(br).unwrap()["entries"][0]["source_state"],
        "completed"
    );
}
#[test]
fn capability_scope_and_artifact_provenance_are_enforced() {
    let (s, _g) = setup(false);
    s.reconcile_goals().unwrap();
    let w = s.claim().unwrap().unwrap();
    assert_eq!(
        s.run_context(w["id"].as_str().unwrap()).unwrap_err().1,
        "context_capability_required"
    );
    let (s, g) = setup(true);
    s.reconcile_goals().unwrap();
    let a = s.claim().unwrap().unwrap();
    let b = s.claim().unwrap().unwrap();
    let ar = a["id"].as_str().unwrap();
    assert_eq!(
        s.publish_context(Author::AgentRun(ar), Some(&g), &entry("scope", 0), "scope")
            .unwrap_err()
            .1,
        "context_scope_is_host_owned"
    );
    let external = s
        .spawn(
            &Spawn {
                preset_name: "team".into(),
                preset_version: "1".into(),
                count: 1,
            },
            "outside",
        )
        .unwrap();
    let run = s
        .enqueue(
            external["instance_ids"][0].as_str().unwrap(),
            &RunInput {
                text: "ordinary".into(),
            },
            "outside",
        )
        .unwrap();
    let claimed = s.claim().unwrap().unwrap();
    assert_eq!(claimed["id"], run["id"]);
    assert_eq!(
        s.run_context(claimed["id"].as_str().unwrap())
            .unwrap_err()
            .1,
        "context_run_not_authorized"
    );
    finish(&s, &claimed);
    let mut invalid = entry("evidence", 0);
    invalid.artifact_ids = vec!["artifact_missing".into()];
    assert_eq!(
        s.publish_context(Author::AgentRun(ar), None, &invalid, "invalid-ref")
            .unwrap_err()
            .1,
        "context_evidence_scope"
    );
    finish(&s, &a);
    s.reconcile_goals().unwrap();
    let artifact = s.goal(&g).unwrap()["tasks"][0]["artifact"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    invalid.artifact_ids = vec![artifact];
    assert!(
        s.publish_context(
            Author::AgentRun(b["id"].as_str().unwrap()),
            None,
            &invalid,
            "valid-ref"
        )
        .is_ok()
    );
    assert_eq!(
        s.goal_context(&g).unwrap()["entries"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn dispatch_snapshots_and_review_context_are_fixed_and_stale_reviews_fail() {
    let (s, g) = setup(true);
    s.publish_context(Author::Operator, Some(&g), &entry("scope", 0), "v1")
        .unwrap();
    s.reconcile_goals().unwrap();
    let a = s.claim().unwrap().unwrap();
    let b = s.claim().unwrap().unwrap();
    let source = a["id"].as_str().unwrap();
    let snap = s.run(source).unwrap()["shared_context"].clone();
    assert_eq!(snap["revision"], 1);
    assert!(
        !s.run(source).unwrap()["task_input"]
            .as_str()
            .unwrap()
            .contains("Shared Goal context at dispatch")
    );
    assert!(
        a["input"]
            .as_str()
            .unwrap()
            .contains("Shared Goal context at dispatch")
    );
    s.publish_context(Author::Operator, Some(&g), &entry("scope", 1), "v2")
        .unwrap();
    assert_eq!(s.run(source).unwrap()["shared_context"], snap);
    finish(&s, &a);
    finish(&s, &b);
    s.reconcile_goals().unwrap();
    let summary = s.claim().unwrap().unwrap();
    assert_eq!(
        s.run(summary["id"].as_str().unwrap()).unwrap()["shared_context"]["revision"],
        2
    );
    finish(&s, &summary);
    s.reconcile_goals().unwrap();
    s.publish_context(Author::Operator, Some(&g), &entry("scope", 2), "v3")
        .unwrap();
    assert_eq!(
        s.accept_goal_context(&g, "stale", 1, 2).unwrap_err().1,
        "context_revision_conflict"
    );
    assert_eq!(s.goal(&g).unwrap()["state"], "awaiting-review");
    s.accept_goal_context(&g, "checked context and artifacts", 1, 3)
        .unwrap();
    let accepted = s.goal(&g).unwrap();
    assert_eq!(accepted["rounds"][0]["shared_context"]["revision"], 3);
    assert_eq!(
        s.publish_context(Author::Operator, Some(&g), &entry("scope", 3), "after")
            .unwrap_err()
            .1,
        "context_goal_read_only"
    );
    assert_eq!(
        s.goal(&g).unwrap()["rounds"][0]["shared_context"],
        accepted["shared_context"]
    );
}
#[test]
fn quotas_invalid_inputs_and_withdrawal_are_atomic_and_preserve_versions() {
    let (s, g) = setup(true);
    s.reconcile_goals().unwrap();
    let a = s.claim().unwrap().unwrap();
    let run = a["id"].as_str().unwrap();
    for i in 0..8 {
        s.publish_context(
            Author::AgentRun(run),
            None,
            &entry(&format!("entry-{i}"), 0),
            &format!("key-{i}"),
        )
        .unwrap();
    }
    assert_eq!(
        s.publish_context(Author::AgentRun(run), None, &entry("over", 0), "over")
            .unwrap_err()
            .1,
        "context_run_quota_exceeded"
    );
    assert_eq!(s.goal_context(&g).unwrap()["revision"], 8);
    let mut withdrawn = entry("entry-0", 1);
    withdrawn.status = "withdrawn".into();
    withdrawn.body = "The original claim lacks evidence.".into();
    s.publish_context(Author::Operator, Some(&g), &withdrawn, "withdraw")
        .unwrap();
    assert_eq!(
        s.context_history(&g, "entry-0").unwrap()["versions"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let mut invalid = entry("bad", 0);
    invalid.kind = "system-instruction".into();
    assert_eq!(
        s.publish_context(Author::Operator, Some(&g), &invalid, "bad")
            .unwrap_err()
            .1,
        "invalid_shared_context"
    );
    assert_eq!(s.goal_context(&g).unwrap()["revision"], 9);
    for i in 8..32 {
        s.publish_context(
            Author::Operator,
            Some(&g),
            &entry(&format!("entry-{i}"), 0),
            &format!("operator-{i}"),
        )
        .unwrap();
    }
    assert_eq!(
        s.publish_context(
            Author::Operator,
            Some(&g),
            &entry("over-keys", 0),
            "over-keys"
        )
        .unwrap_err()
        .1,
        "context_capacity_exceeded"
    );
    let (s, g) = setup(false);
    for i in 0..128 {
        s.publish_context(
            Author::Operator,
            Some(&g),
            &entry("same", i),
            &format!("key-{i}"),
        )
        .unwrap();
    }
    assert_eq!(
        s.publish_context(Author::Operator, Some(&g), &entry("different", 0), "last")
            .unwrap_err()
            .1,
        "context_capacity_exceeded"
    );
    assert_eq!(s.goal_context(&g).unwrap()["revision"], 128);
}
#[test]
fn failed_source_remains_visible_as_unsettled_evidence() {
    let (s, g) = setup(true);
    s.reconcile_goals().unwrap();
    let a = s.claim().unwrap().unwrap();
    let b = s.claim().unwrap().unwrap();
    let ar = a["id"].as_str().unwrap();
    s.publish_context(
        Author::AgentRun(ar),
        None,
        &entry("provisional", 0),
        "draft",
    )
    .unwrap();
    s.finish(ar,a["instance_id"].as_str().unwrap(),"completed",json!({"output":"not reliable","observations":[{"output":{"content":{"outcome_unknown":true}}}]})).unwrap();
    let board = s.run_context(b["id"].as_str().unwrap()).unwrap();
    assert_eq!(board["entries"][0]["source_state"], "interrupted");
    assert_eq!(board["entries"][0]["source_run_id"], ar);
    assert_eq!(
        s.goal_context(&g).unwrap()["entries"][0]["body"],
        "evidence with uncertainty"
    );
}
#[tokio::test]
async fn demo_agents_publish_independent_findings_and_synthesizer_reads_their_shared_board() {
    use harness3::CancellationToken;
    use harness3_platform::{models::Models, runtime};
    use std::{collections::HashMap, sync::Arc, time::Duration};
    use tokio::sync::Mutex;
    let (s, g) = setup(true);
    let stop = CancellationToken::new();
    let worker = tokio::spawn(runtime::worker_loop(
        s.clone(),
        Models::default(),
        Arc::new(Mutex::new(HashMap::new())),
        stop.clone(),
    ));
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if s.goal(&g).unwrap()["state"] == "awaiting-review" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
    stop.cancel();
    worker.await.unwrap();
    let goal = s.goal(&g).unwrap();
    assert_eq!(
        goal["shared_context"]["entries"].as_array().unwrap().len(),
        3
    );
    assert_eq!(goal["tasks"][2]["run"]["shared_context"]["revision"], 2);
    assert!(
        goal["tasks"][2]["run"]["report"]["output"]
            .as_str()
            .unwrap()
            .contains("共 3 条记录")
    );
    assert!(goal["tasks"].as_array().unwrap().iter().all(|t| {
        t["run"]["report"]["observations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["call"]["name"] == "platform.context.publish")
    }));
    s.accept_goal_context(&g, "Demo verified", 1, 3).unwrap();
}

#[test]
fn byte_capacity_rolls_back_history_receipts_and_events() {
    let (s, g) = setup(true);
    for i in 0..5 {
        let mut input = entry(&format!("large-{i}"), 0);
        input.body = "数".repeat(1300);
        s.publish_context(Author::Operator, Some(&g), &input, &format!("large-{i}"))
            .unwrap();
    }
    let before = s.goal_context(&g).unwrap();
    let mut input = entry("overflow", 0);
    input.body = "数".repeat(1300);
    assert_eq!(
        s.publish_context(Author::Operator, Some(&g), &input, "overflow")
            .unwrap_err()
            .1,
        "context_capacity_exceeded"
    );
    assert_eq!(s.goal_context(&g).unwrap(), before);
    assert!(
        s.context_history(&g, "overflow").unwrap()["versions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    input.body = "fits".into();
    assert_eq!(
        s.publish_context(Author::Operator, Some(&g), &input, "overflow")
            .unwrap()["board_revision"],
        6
    );
}

#[test]
fn persisted_board_and_dispatch_snapshot_survive_reopen_and_crash_recovery() {
    let root = std::env::temp_dir().join(format!("harness3-context-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let db = root.join("platform.sqlite");
    let s = Store::open(&db).unwrap();
    s.publish_definition(json!({"name":"persisted","version":"1","purpose":"shared","objective":"evidence","instructions":"inspect","protocol":"demo","plugins":[{"id":"platform.context","version":"1.0.0"}]})).unwrap();
    let fleet = s
        .spawn(
            &Spawn {
                preset_name: "persisted".into(),
                preset_version: "1".into(),
                count: 1,
            },
            "fleet",
        )
        .unwrap();
    let goal = s
        .create_goal(
            &GoalPlan {
                verification: None,
                title: "durable context".into(),
                objective: "compare".into(),
                acceptance: "review".into(),
                tasks: vec![TaskPlan {
                    key: "report".into(),
                    instance_id: fleet["instance_ids"][0].as_str().unwrap().into(),
                    instructions: "report".into(),
                    depends_on: vec![],
                    ..Default::default()
                }],
            },
            "goal",
        )
        .unwrap();
    let g = goal["id"].as_str().unwrap();
    s.publish_context(Author::Operator, Some(g), &entry("scope", 0), "scope")
        .unwrap();
    s.reconcile_goals().unwrap();
    let run = s.claim().unwrap().unwrap();
    let id = run["id"].as_str().unwrap();
    let snap = s.run(id).unwrap()["shared_context"].clone();
    let receipt = s
        .publish_context(
            Author::AgentRun(id),
            None,
            &entry("live-finding", 0),
            "finding",
        )
        .unwrap();
    drop(s);
    let s = Store::open(&db).unwrap();
    assert_eq!(s.run(id).unwrap()["shared_context"], snap);
    assert_eq!(
        s.publish_context(
            Author::AgentRun(id),
            None,
            &entry("live-finding", 0),
            "finding"
        )
        .unwrap(),
        receipt
    );
    s.recover_startup().unwrap();
    let board = s.goal_context(g).unwrap();
    assert_eq!(board["revision"], 2);
    assert_eq!(
        board["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["key"] == "live-finding")
            .unwrap()["source_state"],
        "interrupted"
    );
    assert_eq!(s.run(id).unwrap()["shared_context"], snap);
    drop(s);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn foreign_artifacts_and_legacy_unknown_sources_cannot_pass_review() {
    let (s, g) = setup(true);
    s.reconcile_goals().unwrap();
    let a = s.claim().unwrap().unwrap();
    let b = s.claim().unwrap().unwrap();
    let ar = a["id"].as_str().unwrap();
    s.publish_context(Author::AgentRun(ar), None, &entry("source", 0), "source")
        .unwrap();
    finish(&s, &a);
    finish(&s, &b);
    s.reconcile_goals().unwrap();
    let summary = s.claim().unwrap().unwrap();
    finish(&s, &summary);
    s.reconcile_goals().unwrap();
    let fleet = s
        .spawn(
            &Spawn {
                preset_name: "team".into(),
                preset_version: "1".into(),
                count: 1,
            },
            "foreign-fleet",
        )
        .unwrap();
    let other = s
        .create_goal(
            &GoalPlan {
                verification: None,
                title: "other goal".into(),
                objective: "other".into(),
                acceptance: "other".into(),
                tasks: vec![TaskPlan {
                    key: "report".into(),
                    instance_id: fleet["instance_ids"][0].as_str().unwrap().into(),
                    instructions: "report".into(),
                    depends_on: vec![],
                    ..Default::default()
                }],
            },
            "other-goal",
        )
        .unwrap();
    s.reconcile_goals().unwrap();
    let run = s.claim().unwrap().unwrap();
    finish(&s, &run);
    s.reconcile_goals().unwrap();
    let mut input = entry("foreign-ref", 0);
    input.artifact_ids = vec![
        s.goal(other["id"].as_str().unwrap()).unwrap()["tasks"][0]["artifact"]["id"]
            .as_str()
            .unwrap()
            .into(),
    ];
    assert_eq!(
        s.publish_context(Author::Operator, Some(&g), &input, "foreign")
            .unwrap_err()
            .1,
        "context_evidence_scope"
    );
    s.transaction(|tx| {
        tx.execute(
            "UPDATE runs SET report=? WHERE id=?",
            rusqlite::params![
                json!({"output":"legacy success","outcome_unknown":true}).to_string(),
                ar
            ],
        )?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        s.accept_goal_context(&g, "must reject unsafe source", 1, 1)
            .unwrap_err()
            .1,
        "context_unsettled_evidence"
    );
    assert!(s.goal(&g).unwrap()["rounds"][0]["shared_context"].is_null());
    assert_eq!(s.goal(&g).unwrap()["state"], "awaiting-review");
    assert_eq!(
        s.install(Bundle {
            id: "platform.context".into(),
            version: "1.0.0".into(),
            description: "spoof".into(),
            instructions: "spoof".into()
        })
        .unwrap_err()
        .1,
        "plugin_kind_conflict"
    );
}
