use harness3_platform::{
    domain::*,
    research::{Plan, ResearchInput, ResearchTask},
    store::Store,
};

use harness3_worker as runtime;
use serde_json::{Value, json};
fn store() -> Store {
    let s = Store::memory().unwrap();
    seed(&s);
    s
}
fn seed(s: &Store) {
    s.publish_definition(json!({"name":"researcher","version":"1","purpose":"analyze","objective":"evidence","instructions":"Use only supplied material and available tools. Never claim retrieval without evidence.","protocol":"demo"})).unwrap();
}
fn input() -> ResearchInput {
    ResearchInput {
        verification: None,
        delegation: None,
        title: "Bounded study".into(),
        objective: "Compare two approaches".into(),
        acceptance: "Evidence and limits".into(),
        preset_name: "researcher".into(),
        preset_version: "1".into(),
        researchers: 2,
        parameters: Default::default(),
    }
}
fn plan() -> Plan {
    Plan {
        tasks: vec![
            ResearchTask {
                title: "Architecture".into(),
                instructions: "Analyze architecture".into(),
            },
            ResearchTask {
                title: "Reliability".into(),
                instructions: "Analyze reliability".into(),
            },
        ],
        synthesis: "Reconcile the independent evidence".into(),
    }
}
fn code<T>(r: PResult<T>) -> String {
    match r {
        Err(e) => e.1,
        Ok(_) => panic!("expected error"),
    }
}
fn start(s: &Store) -> (Value, Value) {
    let receipt = s.create_research(&input(), "job").unwrap();
    s.reconcile_fleets().unwrap();
    s.reconcile_research().unwrap();
    let run = s.claim().unwrap().unwrap();
    (receipt, run)
}
fn finish(s: &Store, run: &Value, output: &str) {
    s.finish(
        run["id"].as_str().unwrap(),
        run["instance_id"].as_str().unwrap(),
        "completed",
        json!({"output":output}),
    )
    .unwrap();
}
#[test]
fn atomic_admission_idempotency_parameter_validation_and_reserved_capacity() {
    let s = store();
    let receipt = s.create_research(&input(), "same").unwrap();
    assert_eq!(receipt, s.create_research(&input(), "same").unwrap());
    assert_eq!(s.capacity().unwrap()["reserved"], 4);
    let mut changed = input();
    changed.objective = "changed".into();
    assert_eq!(
        code(s.create_research(&changed, "same")),
        "idempotency_conflict"
    );
    assert_eq!(s.fleets().unwrap().len(), 1);
    changed.researchers = 7;
    assert_eq!(code(s.create_research(&changed, "bad")), "invalid_research");
    changed = input();
    changed.parameters.insert("unknown".into(), "x".into());
    assert_eq!(
        code(s.create_research(&changed, "params")),
        "parameters_not_declared"
    );
    assert_eq!(s.researches().unwrap().len(), 1);
    let s = store();
    let mut big = input();
    big.researchers = 6;
    for n in 0..8 {
        s.create_research(&big, &n.to_string()).unwrap();
    }
    assert_eq!(s.capacity().unwrap()["reserved"], 64);
    assert_eq!(
        code(s.create_research(&big, "overflow")),
        "research_quota_exceeded"
    );
    assert_eq!(s.fleets().unwrap().len(), 8);
}
#[test]
fn planner_identity_exact_task_count_and_immutable_plan_are_enforced() {
    let s = store();
    let (receipt, run) = start(&s);
    let id = receipt["id"].as_str().unwrap();
    let r = run["id"].as_str().unwrap();
    assert_eq!(s.research(id).unwrap()["state"], "planning");
    assert_eq!(s.list("runs").unwrap().len(), 1);
    assert_eq!(
        code(s.submit_research_plan("other-run", &plan())),
        "research_planner_not_authorized"
    );
    let mut bad = plan();
    bad.tasks.pop();
    assert_eq!(
        code(s.submit_research_plan(r, &bad)),
        "invalid_research_plan"
    );
    bad = plan();
    bad.tasks[1].title = bad.tasks[0].title.clone();
    assert_eq!(
        code(s.submit_research_plan(r, &bad)),
        "invalid_research_plan"
    );
    let receipt = s.submit_research_plan(r, &plan()).unwrap();
    assert_eq!(receipt, s.submit_research_plan(r, &plan()).unwrap());
    bad = plan();
    bad.synthesis = "changed".into();
    assert_eq!(
        code(s.submit_research_plan(r, &bad)),
        "research_plan_immutable"
    );
    s.reconcile_research().unwrap();
    assert!(s.research(id).unwrap()["goal_id"].is_null());
}
#[test]
fn independent_derived_agents_fan_in_evidence_acceptance_and_retirement() {
    let s = store();
    let (receipt, planner) = start(&s);
    let id = receipt["id"].as_str().unwrap();
    s.submit_research_plan(planner["id"].as_str().unwrap(), &plan())
        .unwrap();
    finish(&s, &planner, "planned");
    for _ in 0..3 {
        s.reconcile_research().unwrap();
    }
    let j = s.research(id).unwrap();
    let goal = j["goal_id"].as_str().unwrap();
    assert_eq!(s.list("goals").unwrap().len(), 1);
    assert_eq!(j["members"].as_array().unwrap().len(), 4);
    for member in j["members"].as_array().unwrap().iter().skip(1) {
        assert_eq!(member["parent_instance_id"], planner["instance_id"]);
    }
    s.reconcile_goals().unwrap();
    let a = s.claim().unwrap().unwrap();
    let b = s.claim().unwrap().unwrap();
    assert_ne!(a["instance_id"], b["instance_id"]);
    assert_ne!(a["instance_id"], planner["instance_id"]);
    assert!(s.claim().unwrap().is_none());
    finish(&s, &a, "architecture evidence");
    s.reconcile_goals().unwrap();
    assert!(s.claim().unwrap().is_none());
    finish(&s, &b, "reliability evidence");
    s.reconcile_goals().unwrap();
    let summary = s.claim().unwrap().unwrap();
    let text = summary["input"].as_str().unwrap();
    assert!(text.contains(a["id"].as_str().unwrap()));
    assert!(text.contains(b["id"].as_str().unwrap()));
    assert!(text.contains("architecture evidence"));
    assert!(text.contains("reliability evidence"));
    assert_ne!(summary["instance_id"], a["instance_id"]);
    assert_ne!(summary["instance_id"], b["instance_id"]);
    finish(&s, &summary, "synthesized report");
    s.reconcile_goals().unwrap();
    s.reconcile_research().unwrap();
    s.reconcile_fleets().unwrap();
    assert_eq!(s.research(id).unwrap()["state"], "awaiting-review");
    assert_eq!(s.capacity().unwrap()["reserved"], 0);
    assert!(
        s.list("instances")
            .unwrap()
            .iter()
            .all(|a| a["state"] == "terminated")
    );
    s.accept_goal(goal, "accepted evidence").unwrap();
    s.reconcile_research().unwrap();
    assert_eq!(s.research(id).unwrap()["state"], "accepted");
}
#[tokio::test]
async fn interrupted_planning_never_silently_dispatches_saved_plan_after_restart() {
    let path = std::env::temp_dir().join(format!("research-{}.db", uuid::Uuid::new_v4()));
    let s = Store::open(&path).unwrap();
    seed(&s);
    let (receipt, planner) = start(&s);
    s.submit_research_plan(planner["id"].as_str().unwrap(), &plan())
        .unwrap();
    drop(s);
    let s = Store::open(&path).unwrap();
    runtime::recover(&s).await.unwrap();
    s.reconcile_research().unwrap();
    let j = s.research(receipt["id"].as_str().unwrap()).unwrap();
    assert_eq!(j["state"], "failed");
    assert!(j["goal_id"].is_null());
    assert_eq!(s.list("runs").unwrap().len(), 1);
    assert_eq!(s.capacity().unwrap()["reserved"], 0);
    drop(s);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn cancellation_before_provisioning_and_during_planning_releases_reserved_slots() {
    let s = store();
    let receipt = s.create_research(&input(), "cancel").unwrap();
    let id = receipt["id"].as_str().unwrap();
    s.cancel_research(id).unwrap();
    s.reconcile_fleets().unwrap();
    s.reconcile_research().unwrap();
    assert_eq!(s.research(id).unwrap()["state"], "cancelled");
    assert_eq!(s.capacity().unwrap()["reserved"], 0);
    assert!(s.list("instances").unwrap().is_empty());
    let s = store();
    let (receipt, run) = start(&s);
    let id = receipt["id"].as_str().unwrap();
    s.cancel_research(id).unwrap();
    assert!(s.cancel_requested(run["id"].as_str().unwrap()).unwrap());
    assert_eq!(
        code(s.submit_research_plan(run["id"].as_str().unwrap(), &plan())),
        "research_not_planning"
    );
    s.finish(
        run["id"].as_str().unwrap(),
        run["instance_id"].as_str().unwrap(),
        "cancelled",
        json!({"error":{"code":"cancelled"}}),
    )
    .unwrap();
    s.reconcile_research().unwrap();
    s.reconcile_lifecycle().unwrap();
    assert_eq!(s.research(id).unwrap()["state"], "cancelled");
    assert_eq!(s.capacity().unwrap()["reserved"], 0);
}
#[test]
fn deadline_and_failed_planning_do_not_spawn_unbounded_or_orphan_work() {
    let s = store();
    let receipt = s.create_research(&input(), "expiry").unwrap();
    let id = receipt["id"].as_str().unwrap();
    s.transaction(|tx| {
        tx.execute("UPDATE research_jobs SET expires_at=0 WHERE id=?", [id])?;
        Ok(())
    })
    .unwrap();
    s.reconcile_research().unwrap();
    s.reconcile_fleets().unwrap();
    s.reconcile_research().unwrap();
    assert_eq!(s.research(id).unwrap()["state"], "cancelled");
    assert!(s.list("runs").unwrap().is_empty());
    let s = store();
    let (receipt, run) = start(&s);
    finish(&s, &run, "I did the work myself and did not submit a plan");
    s.reconcile_research().unwrap();
    let job = s.research(receipt["id"].as_str().unwrap()).unwrap();
    assert_eq!(job["state"], "failed");
    assert!(job["goal_id"].is_null());
    assert_eq!(s.capacity().unwrap()["reserved"], 0);
}
#[tokio::test]
async fn real_demo_worker_runs_planner_multiple_agents_and_synthesis() {
    use harness3::CancellationToken;
    use harness3_platform::models::Models;
    use std::{collections::HashMap, sync::Arc, time::Duration};
    use tokio::sync::Mutex;
    let s = store();
    let receipt = s.create_research(&input(), "demo").unwrap();
    let id = receipt["id"].as_str().unwrap();
    let stop = CancellationToken::new();
    let worker = tokio::spawn(runtime::worker_loop(
        s.clone(),
        Models::default(),
        Arc::new(Mutex::new(HashMap::new())),
        stop.clone(),
    ));
    let job = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let j = s.research(id).unwrap();
            if ["awaiting-review", "failed", "cancelled"].contains(&j["state"].as_str().unwrap()) {
                break j;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    stop.cancel();
    worker.await.unwrap();
    assert_eq!(job["state"], "awaiting-review", "{job}");
    assert_eq!(job["goal"]["tasks"].as_array().unwrap().len(), 3);
    assert_eq!(s.list("runs").unwrap().len(), 4);
    let source = job["planner_run_id"].as_str().unwrap();
    assert!(
        s.run(source).unwrap()["report"]["observations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|o| o["call"]["name"] == "platform.research.plan")
    );
}

#[test]
fn unknown_research_tool_effect_never_enters_successful_synthesis() {
    let s = store();
    let (receipt, planner) = start(&s);
    let id = receipt["id"].as_str().unwrap();
    s.submit_research_plan(planner["id"].as_str().unwrap(), &plan())
        .unwrap();
    finish(&s, &planner, "planned");
    s.reconcile_research().unwrap();
    s.reconcile_goals().unwrap();
    let a = s.claim().unwrap().unwrap();
    let b = s.claim().unwrap().unwrap();
    s.finish(a["id"].as_str().unwrap(),a["instance_id"].as_str().unwrap(),"completed",json!({"output":"claimed success","observations":[{"output":{"content":{"error":"plugin_outcome_unknown"}}}]})).unwrap();
    finish(&s, &b, "known result");
    s.reconcile_goals().unwrap();
    s.reconcile_research().unwrap();
    let job = s.research(id).unwrap();
    assert_eq!(job["state"], "failed");
    assert!(
        job["goal"]["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["key"] == "synthesis" && t["state"] == "blocked")
    );
    assert!(s.claim().unwrap().is_none());
}

#[test]
fn research_rework_replaces_retired_team_and_keeps_the_goal_and_evidence_history() {
    let s = store();
    let (receipt, planner) = start(&s);
    let id = receipt["id"].as_str().unwrap();
    s.submit_research_plan(planner["id"].as_str().unwrap(), &plan())
        .unwrap();
    finish(&s, &planner, "planned");
    s.reconcile_research().unwrap();
    let goal = s.research(id).unwrap()["goal_id"]
        .as_str()
        .unwrap()
        .to_owned();
    for _ in 0..3 {
        s.reconcile_goals().unwrap();
        let run = s.claim().unwrap().unwrap();
        finish(&s, &run, "first evidence");
    }
    s.reconcile_goals().unwrap();
    s.reconcile_research().unwrap();
    s.reconcile_fleets().unwrap();
    let before = s.research(id).unwrap();
    assert!(
        before["members"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["state"] == "terminated")
    );
    s.rework_goal(
        &goal,
        &harness3_platform::artifacts::ReworkInput {
            task_keys: vec![],
            expected_round: 1,
            note: "Compare conflicts and retain uncertainty".into(),
            assignments: vec![],
        },
        "research-round2",
    )
    .unwrap();
    assert_eq!(s.research(id).unwrap()["state"], "running");
    s.reconcile_fleets().unwrap();
    s.reconcile_goals().unwrap();
    let first = s.claim().unwrap().unwrap();
    let second = s.claim().unwrap().unwrap();
    assert!(s.claim().unwrap().is_none());
    for r in [&first, &second] {
        assert!(
            !before["members"]
                .as_array()
                .unwrap()
                .iter()
                .any(|m| m["instance_id"] == r["instance_id"])
        );
        assert!(r["input"].as_str().unwrap().contains("first evidence"));
        finish(&s, r, "revised evidence");
    }
    s.reconcile_goals().unwrap();
    let synth = s.claim().unwrap().unwrap();
    let input = synth["input"].as_str().unwrap();
    assert!(
        input.contains(first["id"].as_str().unwrap())
            && input.contains(second["id"].as_str().unwrap())
    );
    assert!(input.contains("artifact_"));
    finish(&s, &synth, "revised synthesis");
    s.reconcile_goals().unwrap();
    s.reconcile_research().unwrap();
    s.reconcile_fleets().unwrap();
    let job = s.research(id).unwrap();
    assert_eq!(job["goal_id"], goal);
    assert_eq!(job["goal"]["round"], 2);
    assert_eq!(job["goal"]["artifacts"].as_array().unwrap().len(), 6);
    assert_eq!(job["members"].as_array().unwrap().len(), 7);
    assert!(
        job["members"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["state"] == "terminated")
    );
    assert_eq!(s.capacity().unwrap()["reserved"], 0);
}
