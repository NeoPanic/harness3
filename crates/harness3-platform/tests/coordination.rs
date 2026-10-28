use harness3::CancellationToken;
use harness3_platform::{
    api::{App, router},
    domain::*,
    models::Models,
    runtime,
    store::Store,
};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use tower::ServiceExt;

fn setup() -> (Store, GoalPlan) {
    let store = Store::memory().unwrap();
    store
        .publish(Preset {
            name: "worker".into(),
            version: "1".into(),
            purpose: "test".into(),
            objective: "test".into(),
            instructions: "Inspect text.".into(),
            protocol: Protocol::Demo,
            plugin: None,
        })
        .unwrap();
    let fleet = store
        .spawn(
            &Spawn {
                preset_name: "worker".into(),
                preset_version: "1".into(),
                count: 3,
            },
            "fleet",
        )
        .unwrap();
    let tasks = (0..3)
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
        .collect();
    (
        store,
        GoalPlan {
            verification: None,
            title: "test goal".into(),
            objective: "collect and combine evidence".into(),
            acceptance: "human checks the evidence".into(),
            tasks,
        },
    )
}
fn create(store: &Store, plan: &GoalPlan) -> String {
    store.create_goal(plan, "goal").unwrap()["id"]
        .as_str()
        .unwrap()
        .into()
}
fn settle(store: &Store, work: &Value, state: &str, output: &str) {
    store
        .finish(
            work["id"].as_str().unwrap(),
            work["instance_id"].as_str().unwrap(),
            state,
            if state == "completed" {
                json!({"output":output})
            } else {
                json!({"error":{"code":state}})
            },
        )
        .unwrap();
}

#[test]
fn invalid_graphs_and_rejected_plans_leave_no_partial_state() {
    let (store, mut plan) = setup();
    plan.tasks[0].depends_on.push("task-2".into());
    assert_eq!(
        store.create_goal(&plan, "bad").unwrap_err().1,
        "invalid_task_graph"
    );
    plan.tasks[0].depends_on.clear();
    plan.tasks[2].depends_on.push("task-0".into());
    assert!(store.create_goal(&plan, "bad").is_err());
    plan.tasks[2].depends_on.pop();
    plan.tasks[1].key = "task-0".into();
    assert!(store.create_goal(&plan, "bad").is_err());
    plan.tasks[1].key = "task-1".into();
    plan.tasks[2].instance_id = "missing".into();
    assert_eq!(store.create_goal(&plan, "bad").unwrap_err().0, 404);
    assert!(store.list("goals").unwrap().is_empty());
    assert!(store.list("runs").unwrap().is_empty());
}

#[test]
fn fan_out_fan_in_is_durable_idempotent_and_requires_review() {
    let (store, plan) = setup();
    let accepted = store.create_goal(&plan, "goal").unwrap();
    let id = accepted["id"].as_str().unwrap();
    store.reconcile_goals().unwrap();
    assert_eq!(store.create_goal(&plan, "goal").unwrap(), accepted);
    let mut changed = plan.clone();
    changed.title.push('x');
    assert_eq!(store.create_goal(&changed, "goal").unwrap_err().0, 409);
    assert_eq!(store.accept_goal(id, "premature").unwrap_err().0, 409);
    let first = store.claim().unwrap().unwrap();
    let second = store.claim().unwrap().unwrap();
    assert!(store.claim().unwrap().is_none());
    settle(&store, &first, "completed", "evidence A");
    store.reconcile_goals().unwrap();
    assert!(store.claim().unwrap().is_none());
    settle(&store, &second, "completed", "证据 B 🧪");
    store.reconcile_goals().unwrap();
    let reducer = store.claim().unwrap().unwrap();
    let text = reducer["input"].as_str().unwrap();
    assert!(text.contains("evidence A") && text.contains("证据 B 🧪"));
    assert!(text.contains(first["id"].as_str().unwrap()));
    assert!(text.contains(second["id"].as_str().unwrap()));
    settle(&store, &reducer, "completed", "combined");
    store.reconcile_goals().unwrap();
    assert_eq!(store.goal(id).unwrap()["state"], "awaiting-review");
    assert_eq!(
        store.accept_goal(id, "evidence checked").unwrap()["state"],
        "accepted"
    );
    assert_eq!(
        store.accept_goal(id, "evidence checked").unwrap()["state"],
        "accepted"
    );
    assert!(store.accept_goal(id, "changed review").is_err());
    assert_eq!(
        store.goal(id).unwrap()["tasks"][2]["run"]["report"]["output"],
        "combined"
    );
}

#[test]
fn recovery_keeps_unknown_outcomes_and_blocks_descendants() {
    let (store, plan) = setup();
    let id = create(&store, &plan);
    store.reconcile_goals().unwrap();
    let first = store.claim().unwrap().unwrap();
    let second = store.claim().unwrap().unwrap();
    settle(&store, &second, "completed", "known evidence");
    store.recover_startup().unwrap();
    store.reconcile_goals().unwrap();
    let goal = store.goal(&id).unwrap();
    assert_eq!(goal["state"], "failed");
    assert_eq!(goal["tasks"][0]["state"], "interrupted");
    assert_eq!(goal["tasks"][2]["error"], "dependency_failed");
    assert!(store.claim().unwrap().is_none());
    assert_eq!(
        store.run(first["id"].as_str().unwrap()).unwrap()["state"],
        "interrupted"
    );
}

#[test]
fn cancellation_drains_started_work_and_prevents_downstream_dispatch() {
    let (store, plan) = setup();
    let id = create(&store, &plan);
    store.reconcile_goals().unwrap();
    let first = store.claim().unwrap().unwrap();
    let cancelled = store.cancel_goal(&id).unwrap();
    assert_eq!(cancelled["state"], "cancelling");
    assert_eq!(cancelled["run_ids"][0], first["id"]);
    assert!(
        store
            .cancel_requested(first["id"].as_str().unwrap())
            .unwrap()
    );
    assert!(store.claim().unwrap().is_none());
    assert_eq!(
        store
            .list("instances")
            .unwrap()
            .iter()
            .filter(|i| i["state"] == "busy")
            .count(),
        1
    );
    settle(
        &store,
        &first,
        "completed",
        "completed before cancellation took effect",
    );
    store.reconcile_goals().unwrap();
    let goal = store.goal(&id).unwrap();
    assert_eq!(goal["state"], "cancelled");
    assert_eq!(goal["tasks"][0]["state"], "completed");
    assert_eq!(goal["tasks"][1]["state"], "cancelled");
    assert_eq!(goal["tasks"][2]["state"], "cancelled");
    assert!(store.cancel_goal(&id).is_ok());
    assert!(
        store
            .list("instances")
            .unwrap()
            .iter()
            .all(|i| i["state"] == "ready")
    );
}

#[test]
fn shared_agents_are_serialized_and_suspension_does_not_fail_work() {
    let (store, mut plan) = setup();
    let agent = plan.tasks[0].instance_id.clone();
    for task in &mut plan.tasks {
        task.instance_id = agent.clone();
    }
    store.transition(&agent, "suspend").unwrap();
    let id = create(&store, &plan);
    store.reconcile_goals().unwrap();
    assert!(store.claim().unwrap().is_none());
    store.transition(&agent, "resume").unwrap();
    for _ in 0..3 {
        store.reconcile_goals().unwrap();
        let work = store.claim().unwrap().unwrap();
        assert!(store.claim().unwrap().is_none());
        assert!(store.transition(&agent, "terminate").is_err());
        settle(&store, &work, "completed", "done");
    }
    store.reconcile_goals().unwrap();
    assert_eq!(store.goal(&id).unwrap()["state"], "awaiting-review");
}

#[test]
fn oversized_evidence_is_explicit_and_never_silently_truncated() {
    let (store, plan) = setup();
    let id = create(&store, &plan);
    store.reconcile_goals().unwrap();
    for _ in 0..2 {
        let work = store.claim().unwrap().unwrap();
        settle(&store, &work, "completed", &"x".repeat(40000));
    }
    store.reconcile_goals().unwrap();
    assert_eq!(
        store.goal(&id).unwrap()["tasks"][2]["error"],
        "dependency_context_too_large"
    );
    assert_eq!(store.goal(&id).unwrap()["state"], "failed");
    assert!(store.claim().unwrap().is_none());
}

#[test]
fn legacy_schema_migrates_without_losing_instances_or_sessions() {
    let path =
        std::env::temp_dir().join(format!("harness3-legacy-{}.sqlite", uuid::Uuid::new_v4()));
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE sessions(id TEXT PRIMARY KEY,body TEXT NOT NULL); PRAGMA user_version=1;",
        )
        .unwrap();
    let session = serde_json::to_string(&harness3::session::Session::default()).unwrap();
    connection
        .execute(
            "INSERT INTO sessions(id,body) VALUES('legacy',?)",
            [&session],
        )
        .unwrap();
    drop(connection);
    let store = Store::open(&path).unwrap();
    assert!(store.list("goals").unwrap().is_empty());
    drop(store);
    let connection = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT body FROM sessions WHERE id='legacy'", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
        session
    );
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        13
    );
    connection.execute_batch("PRAGMA user_version=14;").unwrap();
    drop(connection);
    assert!(Store::open(&path).is_err());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn terminal_instance_blocks_unstarted_work_and_active_goal_quota_is_bounded() {
    let (store, plan) = setup();
    for i in 0..32 {
        store.create_goal(&plan, &format!("quota-{i}")).unwrap();
    }
    assert_eq!(
        store.create_goal(&plan, "over-quota").unwrap_err().1,
        "goal_quota_exceeded"
    );
    store
        .transition(&plan.tasks[0].instance_id, "terminate")
        .unwrap();
    store.reconcile_goals().unwrap();
    for goal in store.list("goals").unwrap() {
        let goal = store.goal(goal["id"].as_str().unwrap()).unwrap();
        assert_eq!(goal["tasks"][0]["error"], "instance_terminated");
        assert_eq!(goal["tasks"][2]["error"], "dependency_failed");
        store.cancel_goal(goal["id"].as_str().unwrap()).unwrap();
    }
    let mut changed = plan;
    changed.tasks.remove(0);
    changed.tasks[1].depends_on = vec!["task-1".into()];
    assert!(store.create_goal(&changed, "after-cancellation").is_ok());
}

#[tokio::test]
async fn real_workers_execute_a_graph_and_resume_a_persisted_plan() {
    let (store, plan) = setup();
    let path = std::env::temp_dir().join(format!("harness3-goal-{}.sqlite", uuid::Uuid::new_v4()));
    // Use the public snapshots to create the same actors in a file-backed store.
    let disk = Store::open(&path).unwrap();
    disk.publish(
        harness3_platform::presets::runtime_preset(
            &store.list("presets").unwrap()[0]["definition"],
        )
        .unwrap(),
    )
    .unwrap();
    let fleet = disk
        .spawn(
            &Spawn {
                preset_name: "worker".into(),
                preset_version: "1".into(),
                count: 3,
            },
            "batch",
        )
        .unwrap();
    let mut plan = plan;
    for (i, t) in plan.tasks.iter_mut().enumerate() {
        t.instance_id = fleet["instance_ids"][i].as_str().unwrap().into();
    }
    let id = create(&disk, &plan);
    drop(disk);
    let disk = Store::open(&path).unwrap();
    runtime::recover(&disk).await.unwrap();
    let stop = CancellationToken::new();
    let worker = tokio::spawn(runtime::worker_loop(
        disk.clone(),
        Models::default(),
        Arc::new(Mutex::new(HashMap::new())),
        stop.clone(),
    ));
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if disk.goal(&id).unwrap()["state"] == "awaiting-review" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
    stop.cancel();
    worker.await.unwrap();
    let goal = disk.goal(&id).unwrap();
    assert!(
        goal["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|t| t["run"]["report"]["observations"][0]["call"]["name"] == "text.inspect")
    );
    assert!(
        goal["tasks"][2]["run"]["input"]
            .as_str()
            .unwrap()
            .contains(goal["tasks"][0]["run_id"].as_str().unwrap())
    );
    drop(disk);
    let reopened = Store::open(&path).unwrap();
    assert_eq!(reopened.goal(&id).unwrap()["state"], "awaiting-review");
    drop(reopened);
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn goal_api_reuses_auth_and_enforces_idempotency_and_review() {
    let (store, plan) = setup();
    let app = router(App {
        plugins: harness3_platform::plugins::ProcessHost::disabled(store.clone()),
        store: store.clone(),
        token: "a".repeat(64),
        models: Models::default(),
        cancellations: Arc::new(Mutex::new(HashMap::new())),
    });
    let request = |auth: bool, key: bool| {
        let mut builder = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/goals")
            .header("Host", "127.0.0.1:7333")
            .header("Content-Type", "application/json");
        if auth {
            builder = builder.header("Authorization", format!("Bearer {}", "a".repeat(64)));
        }
        if key {
            builder = builder.header("Idempotency-Key", "api-goal");
        }
        builder
            .body(axum::body::Body::from(serde_json::to_vec(&plan).unwrap()))
            .unwrap()
    };
    assert_eq!(
        app.clone()
            .oneshot(request(false, true))
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        app.clone()
            .oneshot(request(true, false))
            .await
            .unwrap()
            .status(),
        400
    );
    assert_eq!(
        app.clone()
            .oneshot(request(true, true))
            .await
            .unwrap()
            .status(),
        202
    );
    assert_eq!(
        app.oneshot(request(true, true)).await.unwrap().status(),
        202
    );
    assert_eq!(store.list("goals").unwrap().len(), 1);
}
