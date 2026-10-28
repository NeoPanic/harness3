use harness3_platform::{
    claims::{ClaimInput, ReleaseInput},
    domain::*,
    store::Store,
};
use serde_json::{Value, json};
fn seed(s: &Store) -> Vec<String> {
    s.publish(Preset {
        name: "worker".into(),
        version: "1".into(),
        purpose: "test".into(),
        objective: "evidence".into(),
        instructions: "inspect".into(),
        protocol: Protocol::Demo,
        plugin: None,
    })
    .unwrap();
    s.spawn(
        &Spawn {
            preset_name: "worker".into(),
            preset_version: "1".into(),
            count: 4,
        },
        "fleet",
    )
    .unwrap()["instance_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().into())
        .collect()
}
fn task(key: &str, instance: &str, manual: bool, candidates: Vec<String>) -> TaskPlan {
    TaskPlan {
        key: key.into(),
        instance_id: instance.into(),
        instructions: format!("inspect {key}"),
        manual_claim: manual,
        candidates,
        ..Default::default()
    }
}
fn plan(tasks: Vec<TaskPlan>) -> GoalPlan {
    GoalPlan {
        verification: None,
        title: "candidate work".into(),
        objective: "joint evidence".into(),
        acceptance: "inspect provenance".into(),
        tasks,
    }
}
fn create(s: &Store, tasks: Vec<TaskPlan>) -> String {
    s.create_goal(&plan(tasks), "goal").unwrap()["id"]
        .as_str()
        .unwrap()
        .into()
}
fn claim(instance: &str, generation: i64) -> ClaimInput {
    ClaimInput {
        expected_round: 1,
        expected_generation: generation,
        instance_id: instance.into(),
    }
}
fn release(generation: i64) -> ReleaseInput {
    ReleaseInput {
        expected_round: 1,
        expected_generation: generation,
        note: "Reassign before execution".into(),
    }
}
fn finish(s: &Store, run: &Value) {
    s.finish(
        run["id"].as_str().unwrap(),
        run["instance_id"].as_str().unwrap(),
        "completed",
        json!({"output":"settled evidence"}),
    )
    .unwrap();
}
#[test]
fn automatic_pool_skips_suspended_primary_and_records_actual_owner() {
    let s = Store::memory().unwrap();
    let a = seed(&s);
    s.transition(&a[0], "suspend").unwrap();
    let g = create(&s, vec![task("report", &a[0], false, vec![a[1].clone()])]);
    s.reconcile_goals().unwrap();
    let t = s.task(&g, "report").unwrap();
    assert_eq!(t["instance_id"], a[1]);
    assert_eq!(t["attempts"][0]["instance_id"], a[1]);
    assert_eq!(t["attempts"][0]["origin"], "coordinator");
    assert_eq!(t["claim"]["generation"], 1);
    let run = s.claim().unwrap().unwrap();
    assert_eq!(run["instance_id"], a[1]);
    finish(&s, &run);
    s.reconcile_goals().unwrap();
    assert_eq!(
        s.task(&g, "report").unwrap()["attempts"][0]["state"],
        "completed"
    );
}
#[test]
fn competing_manual_claims_commit_one_attempt_and_idempotent_receipt() {
    let s = Store::memory().unwrap();
    let a = seed(&s);
    let g = create(&s, vec![task("report", &a[0], true, vec![a[1].clone()])]);
    s.reconcile_goals().unwrap();
    assert!(s.claim().unwrap().is_none());
    assert_eq!(
        s.claim_task(&g, "report", &claim(&a[2], 0), "outside")
            .unwrap_err()
            .1,
        "task_candidate_not_allowed"
    );
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|i| {
            let s = s.clone();
            let g = g.clone();
            let instance = a[i].clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                let input = claim(&instance, 0);
                let key = format!("racer-{i}");
                (s.claim_task(&g, "report", &input, &key), input, key)
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.0.is_ok()).count(), 1);
    let (receipt, input, key) = results.iter().find(|r| r.0.is_ok()).unwrap();
    assert_eq!(
        s.claim_task(&g, "report", input, key).unwrap(),
        *receipt.as_ref().unwrap()
    );
    assert_eq!(
        s.task(&g, "report").unwrap()["attempts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(s.list("runs").unwrap().len(), 1);
    let mut changed = input.clone();
    changed.expected_generation = 1;
    assert_eq!(
        s.claim_task(&g, "report", &changed, key).unwrap_err().1,
        "idempotency_conflict"
    );
}
#[test]
fn queued_release_preserves_history_and_late_owner_cannot_release_or_settle_new_work() {
    let s = Store::memory().unwrap();
    let a = seed(&s);
    let g = create(&s, vec![task("report", &a[0], true, vec![a[1].clone()])]);
    let first = s
        .claim_task(&g, "report", &claim(&a[0], 0), "first")
        .unwrap();
    let released = s
        .release_task(&g, "report", &release(1), "release")
        .unwrap();
    assert_eq!(
        s.release_task(&g, "report", &release(1), "release")
            .unwrap(),
        released
    );
    s.reconcile_goals().unwrap();
    assert!(s.claim().unwrap().is_none());
    assert_eq!(
        s.run(first["run_id"].as_str().unwrap()).unwrap()["state"],
        "cancelled"
    );
    let next = s
        .claim_task(&g, "report", &claim(&a[1], 1), "next")
        .unwrap();
    assert_eq!(next["generation"], 2);
    assert_eq!(
        s.release_task(&g, "report", &release(1), "stale")
            .unwrap_err()
            .1,
        "task_generation_conflict"
    );
    let run = s.claim().unwrap().unwrap();
    assert_eq!(run["id"], next["run_id"]);
    assert_eq!(
        s.release_task(&g, "report", &release(2), "started")
            .unwrap_err()
            .1,
        "task_already_started"
    );
    assert_eq!(
        s.finish(
            run["id"].as_str().unwrap(),
            &a[0],
            "completed",
            json!({"output":"wrong owner"})
        )
        .unwrap_err()
        .1,
        "run_not_owned"
    );
    assert!(
        s.finish(
            first["run_id"].as_str().unwrap(),
            &a[0],
            "completed",
            json!({"output":"late"})
        )
        .is_err()
    );
    finish(&s, &run);
    s.reconcile_goals().unwrap();
    let t = s.task(&g, "report").unwrap();
    assert_eq!(t["attempts"][0]["state"], "completed");
    assert_eq!(t["attempts"][1]["state"], "released");
    assert_eq!(t["attempts"][1]["release_note"], release(1).note);
}
#[test]
fn manual_claim_cannot_bypass_dependencies_cancellation_or_round_binding() {
    let s = Store::memory().unwrap();
    let a = seed(&s);
    let mut downstream = task("summary", &a[1], true, vec![]);
    downstream.depends_on = vec!["source".into()];
    let g = create(&s, vec![task("source", &a[0], false, vec![]), downstream]);
    assert_eq!(
        s.claim_task(&g, "summary", &claim(&a[1], 0), "too-early")
            .unwrap_err()
            .1,
        "task_not_ready"
    );
    assert!(s.list("runs").unwrap().is_empty());
    s.reconcile_goals().unwrap();
    let run = s.claim().unwrap().unwrap();
    finish(&s, &run);
    let mut stale = claim(&a[1], 0);
    stale.expected_round = 2;
    assert_eq!(
        s.claim_task(&g, "summary", &stale, "round").unwrap_err().1,
        "goal_round_conflict"
    );
    s.claim_task(&g, "summary", &claim(&a[1], 0), "summary")
        .unwrap();
    let summary = s.claim().unwrap().unwrap();
    assert!(
        summary["input"]
            .as_str()
            .unwrap()
            .contains(run["id"].as_str().unwrap())
    );
    s.cancel_goal(&g).unwrap();
    assert_eq!(
        s.release_task(&g, "summary", &release(1), "cancelled")
            .unwrap_err()
            .1,
        "goal_not_running"
    );
    s.finish(
        summary["id"].as_str().unwrap(),
        &a[1],
        "cancelled",
        json!({"error":{"code":"cancelled"}}),
    )
    .unwrap();
    s.reconcile_goals().unwrap();
    assert_eq!(s.goal(&g).unwrap()["state"], "cancelled");
}
#[test]
fn release_is_bounded_and_rejected_claims_do_not_leave_runs_or_busy_instances() {
    let s = Store::memory().unwrap();
    let a = seed(&s);
    let g = create(&s, vec![task("report", &a[0], true, vec![])]);
    for generation in 0..8 {
        s.claim_task(
            &g,
            "report",
            &claim(&a[0], generation),
            &format!("claim-{generation}"),
        )
        .unwrap();
        s.release_task(
            &g,
            "report",
            &release(generation + 1),
            &format!("release-{generation}"),
        )
        .unwrap();
    }
    assert_eq!(
        s.claim_task(&g, "report", &claim(&a[0], 8), "over")
            .unwrap_err()
            .1,
        "task_attempt_limit"
    );
    assert_eq!(s.list("runs").unwrap().len(), 8);
    assert_eq!(
        s.task(&g, "report").unwrap()["attempts"]
            .as_array()
            .unwrap()
            .len(),
        8
    );
    assert!(s.claim().unwrap().is_none());
    assert!(
        s.enqueue(
            &a[0],
            &RunInput {
                text: "still available".into()
            },
            "normal"
        )
        .is_ok()
    );
}
#[test]
fn candidate_validation_and_legacy_fingerprints_remain_atomic() {
    let s = Store::memory().unwrap();
    let a = seed(&s);
    let mut p = plan(vec![task("report", &a[0], false, vec![a[0].clone()])]);
    assert!(s.create_goal(&p, "goal").is_err());
    assert!(s.list("goals").unwrap().is_empty());
    p.tasks[0].candidates = vec!["agent_missing".into()];
    assert_eq!(
        s.create_goal(&p, "goal").unwrap_err().1,
        "instance_not_found"
    );
    assert!(s.list("goals").unwrap().is_empty());
    p.tasks[0].candidates.clear();
    let old = json!({"title":p.title,"objective":p.objective,"acceptance":p.acceptance,"tasks":[{"key":"report","instance_id":a[0],"instructions":"inspect report","depends_on":[]}]});
    assert_eq!(serde_json::to_value(&p).unwrap(), old);
    let first = s.create_goal(&p, "goal").unwrap();
    assert_eq!(s.create_goal(&p, "goal").unwrap(), first);
    p.tasks[0].candidates = vec![a[1].clone()];
    assert_eq!(
        s.create_goal(&p, "goal").unwrap_err().1,
        "idempotency_conflict"
    );
}
#[test]
fn migration_backfills_attempts_and_reopen_preserves_release_and_unknown_result() {
    let root = std::env::temp_dir().join(format!("harness3-attempts-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("platform.sqlite");
    let s = Store::open(&path).unwrap();
    let a = seed(&s);
    let g = create(&s, vec![task("report", &a[0], true, vec![a[1].clone()])]);
    s.claim_task(&g, "report", &claim(&a[0], 0), "first")
        .unwrap();
    let run = s.claim().unwrap().unwrap();
    finish(&s, &run);
    s.reconcile_goals().unwrap();
    drop(s);
    let c = rusqlite::Connection::open(&path).unwrap();
    c.execute_batch(
        "DROP TABLE task_claim_policies; DROP TABLE task_attempts; PRAGMA user_version=9;",
    )
    .unwrap();
    drop(c);
    let s = Store::open(&path).unwrap();
    let t = s.task(&g, "report").unwrap();
    assert_eq!(t["attempts"][0]["origin"], "legacy");
    assert_eq!(t["attempts"][0]["run_id"], run["id"]);
    assert_eq!(t["attempts"].as_array().unwrap().len(), 1);
    let other = s
        .create_goal(&plan(vec![task("pending", &a[1], true, vec![])]), "other")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    s.claim_task(&other, "pending", &claim(&a[1], 0), "first")
        .unwrap();
    s.release_task(&other, "pending", &release(1), "release")
        .unwrap();
    s.claim_task(&other, "pending", &claim(&a[1], 1), "second")
        .unwrap();
    let unknown = s.claim().unwrap().unwrap();
    drop(s);
    let s = Store::open(&path).unwrap();
    s.recover_startup().unwrap();
    s.reconcile_goals().unwrap();
    let t = s.task(&other, "pending").unwrap();
    assert_eq!(t["attempts"][0]["state"], "interrupted");
    assert_eq!(t["attempts"][0]["run_id"], unknown["id"]);
    assert_eq!(t["attempts"][1]["state"], "released");
    assert_eq!(
        s.claim_task(&other, "pending", &claim(&a[1], 2), "unsafe")
            .unwrap_err()
            .1,
        "goal_not_running"
    );
    drop(s);
    std::fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn actual_workers_share_candidate_pool_and_synthesize_distinct_contributions() {
    use harness3::CancellationToken;
    use harness3_platform::{models::Models, runtime};
    use std::{collections::HashMap, sync::Arc, time::Duration};
    use tokio::sync::Mutex;
    let s = Store::memory().unwrap();
    let a = seed(&s);
    let mut summary = task("summary", &a[2], false, vec![]);
    summary.depends_on = vec!["a".into(), "b".into()];
    let g = create(
        &s,
        vec![
            task("a", &a[0], false, vec![a[1].clone()]),
            task("b", &a[0], false, vec![a[1].clone()]),
            summary,
        ],
    );
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
    assert_ne!(
        goal["tasks"][0]["instance_id"],
        goal["tasks"][1]["instance_id"]
    );
    assert!(
        goal["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|t| t["attempts"].as_array().unwrap().len() == 1
                && t["attempts"][0]["state"] == "completed")
    );
    assert!(
        goal["tasks"][2]["run"]["input"]
            .as_str()
            .unwrap()
            .contains(goal["tasks"][0]["run_id"].as_str().unwrap())
    );
}

#[test]
fn independent_database_connections_reuse_one_claim_receipt() {
    let root = std::env::temp_dir().join(format!(
        "harness3-claims-concurrent-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("platform.sqlite");
    let s = Store::open(&path).unwrap();
    let a = seed(&s);
    let g = create(&s, vec![task("report", &a[0], true, vec![])]);
    drop(s);
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let path = path.clone();
            let g = g.clone();
            let instance = a[0].clone();
            std::thread::spawn(move || {
                let s = Store::open(&path).unwrap();
                for _ in 0..50 {
                    match s.claim_task(&g, "report", &claim(&instance, 0), "same") {
                        Ok(receipt) => return receipt,
                        Err(e) if e.1 == "storage_error" => {
                            std::thread::sleep(std::time::Duration::from_millis(10))
                        }
                        Err(e) => panic!("unexpected claim failure: {e}"),
                    }
                }
                panic!("claim retry exhausted")
            })
        })
        .collect();
    let receipts: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert!(receipts.iter().all(|r| r == &receipts[0]));
    let s = Store::open(&path).unwrap();
    assert_eq!(
        s.task(&g, "report").unwrap()["attempts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(s.list("runs").unwrap().len(), 1);
    drop(s);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn rework_uses_new_generation_and_retains_all_round_attempts() {
    use harness3_platform::artifacts::{Assignment, ReworkInput};
    let s = Store::memory().unwrap();
    let a = seed(&s);
    let g = create(&s, vec![task("report", &a[0], true, vec![a[1].clone()])]);
    s.claim_task(&g, "report", &claim(&a[0], 0), "first")
        .unwrap();
    s.release_task(&g, "report", &release(1), "release")
        .unwrap();
    s.claim_task(&g, "report", &claim(&a[1], 1), "second")
        .unwrap();
    let first = s.claim().unwrap().unwrap();
    finish(&s, &first);
    s.reconcile_goals().unwrap();
    s.rework_goal(
        &g,
        &ReworkInput {
            task_keys: vec![],
            expected_round: 1,
            note: "new round".into(),
            assignments: vec![Assignment {
                key: "report".into(),
                instance_id: a[2].clone(),
            }],
        },
        "rework",
    )
    .unwrap();
    assert_eq!(
        s.claim_task(&g, "report", &claim(&a[0], 2), "old-round")
            .unwrap_err()
            .1,
        "goal_round_conflict"
    );
    s.reconcile_goals().unwrap();
    let run = s.claim().unwrap().unwrap();
    assert_eq!(run["instance_id"], a[2]);
    finish(&s, &run);
    s.reconcile_goals().unwrap();
    let t = s.task(&g, "report").unwrap();
    assert_eq!(t["attempts"].as_array().unwrap().len(), 3);
    assert_eq!(t["attempts"][0]["round"], 2);
    assert_eq!(t["attempts"][0]["generation"], 3);
    assert_eq!(t["claim"]["candidates"], json!([a[2]]));
    assert_eq!(t["claim"]["mode"], "automatic");
}
