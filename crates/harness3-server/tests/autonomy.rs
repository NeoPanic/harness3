use harness3_platform::{
    autonomy::{AppendInput, NextInput},
    domain::*,
    store::Store,
};

use serde_json::{Value, json};
fn tick(s: &Store) {
    s.reconcile_fleets().unwrap();
    s.reconcile_goals().unwrap();
}
fn fixture(next: bool) -> (Store, String, Value) {
    let s = Store::memory().unwrap();
    s.publish_definition(json!({"name":"autonomous","version":"1","purpose":"organize useful next work","objective":"complete evidence","instructions":"inspect task board","protocol":"demo","plugins":[{"id":"platform.tasks","version":"1.0.0"}]})).unwrap();
    let agent = s
        .spawn(
            &Spawn {
                preset_name: "autonomous".into(),
                preset_version: "1".into(),
                count: 1,
            },
            "agent",
        )
        .unwrap()["instance_ids"][0]
        .as_str()
        .unwrap()
        .to_owned();
    let mut tasks = vec![TaskPlan {
        key: "source".into(),
        instance_id: agent.clone(),
        instructions: "inspect initial evidence".into(),
        ..Default::default()
    }];
    if next {
        tasks.push(TaskPlan {
            key: "next".into(),
            instance_id: agent,
            depends_on: vec!["source".into()],
            instructions: "inspect next evidence".into(),
            manual_claim: true,
            ..Default::default()
        });
    }
    let g = s
        .create_goal(
            &GoalPlan {
                title: "autonomous team".into(),
                objective: "fill evidence gaps".into(),
                acceptance: "provenance is visible".into(),
                tasks,
                verification: None,
            },
            "goal",
        )
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    tick(&s);
    let source = s.claim().unwrap().unwrap();
    (s, g, source)
}
fn rid(r: &Value) -> &str {
    r["id"].as_str().unwrap()
}
fn finish(s: &Store, r: &Value, state: &str) {
    s.finish(
        rid(r),
        r["instance_id"].as_str().unwrap(),
        state,
        json!({"output":"supported evidence"}),
    )
    .unwrap();
}
fn append(key: &str) -> AppendInput {
    AppendInput {
        key: key.into(),
        instructions: "investigate one missing source".into(),
        depends_on: vec![],
        preset_name: "autonomous".into(),
        preset_version: "1".into(),
        parameters: Default::default(),
    }
}
#[test]
fn additional_task_waits_for_success_and_uses_a_fresh_instance() {
    let (s, g, source) = fixture(false);
    let request = append("follow-up");
    let receipt = s
        .request_append_task(rid(&source), &request, "append")
        .unwrap();
    assert_eq!(
        s.request_append_task(rid(&source), &request, "append")
            .unwrap(),
        receipt
    );
    tick(&s);
    assert_eq!(s.goal(&g).unwrap()["tasks"].as_array().unwrap().len(), 1);
    assert!(s.claim().unwrap().is_none());
    finish(&s, &source, "completed");
    tick(&s);
    assert_eq!(s.goal(&g).unwrap()["state"], "running");
    assert_eq!(s.goal(&g).unwrap()["tasks"].as_array().unwrap().len(), 2);
    tick(&s);
    let follow = s.claim().unwrap().unwrap();
    assert_ne!(follow["instance_id"], source["instance_id"]);
    assert!(follow["input"].as_str().unwrap().contains(rid(&source)));
    let goal = s.goal(&g).unwrap();
    assert_eq!(goal["tasks"][1]["depends_on"], json!(["source"]));
    let intent = &s.goal_autonomy(&g).unwrap()["intents"][0];
    assert_eq!(intent["state"], "appended");
    assert_eq!(intent["result"]["task_key"], "follow-up");
    finish(&s, &follow, "completed");
    tick(&s);
    assert_eq!(s.goal(&g).unwrap()["state"], "awaiting-review");
    assert!(s.claim().unwrap().is_none());
}
#[test]
fn source_failure_cannot_activate_provisional_graph_changes() {
    for state in ["failed", "interrupted", "cancelled"] {
        let (s, g, source) = fixture(false);
        s.request_append_task(rid(&source), &append("follow-up"), "append")
            .unwrap();
        finish(&s, &source, state);
        tick(&s);
        assert_eq!(s.goal(&g).unwrap()["tasks"].as_array().unwrap().len(), 1);
        assert_eq!(
            s.goal_autonomy(&g).unwrap()["intents"][0]["state"],
            "failed"
        );
        assert!(s.claim().unwrap().is_none());
    }
}
#[test]
fn agent_requests_next_candidate_assignment_then_finishes_its_current_work() {
    let (s, g, source) = fixture(true);
    let board = s.task_board(rid(&source)).unwrap();
    assert_eq!(board["source_task"], "source");
    assert_eq!(board["tasks"].as_array().unwrap().len(), 2);
    let input = NextInput {
        task_key: "next".into(),
        expected_generation: 0,
    };
    s.request_next_task(rid(&source), &input, "take-next")
        .unwrap();
    s.reconcile_autonomy().unwrap();
    assert!(s.claim().unwrap().is_none());
    finish(&s, &source, "completed");
    tick(&s);
    s.reconcile_autonomy().unwrap();
    let next = s.claim().unwrap().unwrap();
    assert_eq!(next["instance_id"], source["instance_id"]);
    assert_ne!(next["id"], source["id"]);
    assert_eq!(
        s.goal_autonomy(&g).unwrap()["intents"][0]["state"],
        "claimed"
    );
    s.transaction(|tx| {
        tx.execute(
            "UPDATE autonomy_intents SET state='pending',finished_at=NULL WHERE goal_id=?",
            [&g],
        )?;
        Ok(())
    })
    .unwrap();
    s.reconcile_autonomy().unwrap();
    assert_eq!(
        s.goal_autonomy(&g).unwrap()["intents"][0]["state"],
        "claimed"
    );
    assert!(s.claim().unwrap().is_none());
    finish(&s, &next, "completed");
    tick(&s);
    assert_eq!(s.goal(&g).unwrap()["state"], "awaiting-review");
}
#[test]
fn invalid_append_requests_do_not_mutate_the_existing_task_graph() {
    let (s, g, source) = fixture(false);
    let mut request = append("follow-up");
    request.depends_on = vec!["absent".into()];
    assert_eq!(
        s.request_append_task(rid(&source), &request, "append")
            .unwrap_err()
            .1,
        "invalid_append_dependencies"
    );
    request.depends_on.clear();
    let receipt = s
        .request_append_task(rid(&source), &request, "append")
        .unwrap();
    assert_eq!(
        s.request_append_task(rid(&source), &request, "append")
            .unwrap(),
        receipt
    );
    request.instructions = "changed after acceptance".into();
    assert_eq!(
        s.request_append_task(rid(&source), &request, "append")
            .unwrap_err()
            .1,
        "idempotency_conflict"
    );
    assert_eq!(
        s.request_append_task(rid(&source), &request, "different-command")
            .unwrap_err()
            .1,
        "task_key_conflict"
    );
    assert_eq!(s.goal(&g).unwrap()["tasks"].as_array().unwrap().len(), 1);
}

#[test]
fn pending_summary_waits_for_parallel_additions_and_receives_their_evidence() {
    let (s, g, source) = fixture(true);
    s.transaction(|tx| {
        tx.execute(
            "UPDATE task_claim_policies SET mode='automatic' WHERE goal_id=? AND task_key='next'",
            [&g],
        )?;
        Ok(())
    })
    .unwrap();
    s.request_append_task(rid(&source), &append("extra-a"), "extra-a")
        .unwrap();
    s.request_append_task(rid(&source), &append("extra-b"), "extra-b")
        .unwrap();
    finish(&s, &source, "completed");
    tick(&s);
    let goal = s.goal(&g).unwrap();
    let tasks = goal["tasks"].as_array().unwrap();
    let summary = tasks.iter().find(|t| t["key"] == "next").unwrap();
    assert_eq!(summary["state"], "pending");
    assert_eq!(
        summary["depends_on"],
        json!(["source", "extra-a", "extra-b"])
    );
    assert_eq!(tasks.last().unwrap()["key"], "next");
    assert!(s.claim().unwrap().is_none());
    tick(&s);
    let a = s.claim().unwrap().unwrap();
    let b = s.claim().unwrap().unwrap();
    assert!(s.claim().unwrap().is_none());
    assert_ne!(a["instance_id"], b["instance_id"]);
    for follow in [&a, &b] {
        finish(&s, follow, "completed");
    }
    tick(&s);
    let summary = s.claim().unwrap().unwrap();
    assert_eq!(summary["instance_id"], source["instance_id"]);
    assert!(summary["input"].as_str().unwrap().contains(rid(&a)));
    assert!(summary["input"].as_str().unwrap().contains(rid(&b)));
    finish(&s, &summary, "completed");
    tick(&s);
    assert_eq!(s.goal(&g).unwrap()["state"], "awaiting-review");
}
#[test]
fn explicit_dependency_on_downstream_does_not_create_a_cycle() {
    let (s, g, source) = fixture(true);
    s.transaction(|tx| {
        tx.execute(
            "UPDATE task_claim_policies SET mode='automatic' WHERE goal_id=? AND task_key='next'",
            [&g],
        )?;
        Ok(())
    })
    .unwrap();
    let mut request = append("after-summary");
    request.depends_on = vec!["next".into()];
    s.request_append_task(rid(&source), &request, "after-summary")
        .unwrap();
    finish(&s, &source, "completed");
    tick(&s);
    let goal = s.goal(&g).unwrap();
    assert_eq!(goal["tasks"][1]["key"], "next");
    assert_eq!(goal["tasks"][1]["depends_on"], json!(["source"]));
    let summary = s.claim().unwrap().unwrap();
    assert_eq!(summary["instance_id"], source["instance_id"]);
    finish(&s, &summary, "completed");
    tick(&s);
    let after = s.claim().unwrap().unwrap();
    assert!(after["input"].as_str().unwrap().contains(rid(&summary)));
    finish(&s, &after, "completed");
    tick(&s);
    assert_eq!(s.goal(&g).unwrap()["state"], "awaiting-review");
}
