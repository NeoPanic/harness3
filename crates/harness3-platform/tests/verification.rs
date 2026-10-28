use harness3_platform::{
    collaboration::{Author, ContextInput},
    domain::*,
    store::Store,
    verification::{Check, Finding, Policy, Retry, Verdict},
};
use serde_json::{Value, json};

fn tick(s: &Store) {
    s.reconcile_fleets().unwrap();
    s.reconcile_goals().unwrap();
}
fn finish(s: &Store, run: &Value, state: &str) {
    s.finish(
        run["id"].as_str().unwrap(),
        run["instance_id"].as_str().unwrap(),
        state,
        json!({"output":"证据甲🙂证据乙：a complete contribution"}),
    )
    .unwrap();
}
fn setup(reworks: usize) -> (Store, String, Value) {
    let s = Store::memory().unwrap();
    s.publish_definition(json!({"name":"review-team","version":"1","purpose":"review evidence","objective":"check findings","instructions":"inspect evidence","protocol":"demo","plugins":[]})).unwrap();
    let agent = s
        .spawn(
            &Spawn {
                preset_name: "review-team".into(),
                preset_version: "1".into(),
                count: 1,
            },
            "producer",
        )
        .unwrap();
    let g = s
        .create_goal(
            &GoalPlan {
                title: "checked collaboration".into(),
                objective: "produce a supported result".into(),
                acceptance: "evidence is sufficient".into(),
                verification: Some(Policy {
                    preset_name: "review-team".into(),
                    preset_version: "1".into(),
                    parameters: Default::default(),
                    checks: vec![Check {
                        key: "evidence".into(),
                        description: "check evidence completeness".into(),
                    }],
                    max_auto_reworks: reworks,
                }),
                tasks: vec![TaskPlan {
                    key: "research".into(),
                    instance_id: agent["instance_ids"][0].as_str().unwrap().into(),
                    instructions: "produce evidence".into(),
                    ..Default::default()
                }],
            },
            "goal",
        )
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    tick(&s);
    let producer = s.claim().unwrap().unwrap();
    assert!(
        s.verification_job(producer["id"].as_str().unwrap())
            .unwrap()
            .is_none()
    );
    finish(&s, &producer, "completed");
    tick(&s);
    assert_eq!(s.goal(&g).unwrap()["state"], "verifying");
    (s, g, producer)
}
fn reviewer(s: &Store) -> Value {
    for _ in 0..8 {
        tick(s);
        if let Some(run) = s.claim().unwrap() {
            return run;
        }
    }
    panic!("reviewer not dispatched")
}
fn artifact(s: &Store, r: &Value) -> String {
    s.verification_job(r["id"].as_str().unwrap())
        .unwrap()
        .unwrap()["manifest"][0]["id"]
        .as_str()
        .unwrap()
        .into()
}
fn verdict(status: &str, ids: Vec<String>) -> Verdict {
    Verdict {
        summary: "independent assessment".into(),
        checks: vec![Finding {
            key: "evidence".into(),
            status: status.into(),
            reason: "checked the supplied evidence".into(),
            artifact_ids: ids,
        }],
    }
}
fn read(s: &Store, r: &Value, a: &str) {
    s.verification_evidence(r["id"].as_str().unwrap(), a, 0, 12000)
        .unwrap();
}
fn context(s: &Store, g: &str) {
    s.publish_context(
        Author::Operator,
        Some(g),
        &ContextInput {
            key: "new-fact".into(),
            expected_revision: 0,
            title: "new information".into(),
            body: "additional evidence changes the review context".into(),
            kind: "observation".into(),
            status: "active".into(),
            artifact_ids: vec![],
        },
        "new-context",
    )
    .unwrap();
}
#[test]
fn independent_reviewer_requires_full_unicode_evidence_and_successful_run() {
    let (s, g, p) = setup(0);
    let r = reviewer(&s);
    let run = r["id"].as_str().unwrap();
    let a = artifact(&s, &r);
    assert_ne!(p["instance_id"], r["instance_id"]);
    assert_eq!(
        s.verification_evidence(p["id"].as_str().unwrap(), &a, 0, 10)
            .unwrap_err()
            .1,
        "verification_run_not_authorized"
    );
    assert_eq!(
        s.verification_evidence(run, "foreign", 0, 10)
            .unwrap_err()
            .1,
        "verification_artifact_out_of_scope"
    );
    let v = verdict("passed", vec![a.clone()]);
    assert_eq!(
        s.submit_verification(run, &v).unwrap_err().1,
        "verification_evidence_not_read"
    );
    let page = s.verification_evidence(run, &a, 0, 4).unwrap();
    assert_eq!(page["content"], "证据甲🙂");
    assert_eq!(
        s.submit_verification(run, &v).unwrap_err().1,
        "verification_evidence_incomplete"
    );
    s.verification_evidence(run, &a, 5, 12000).unwrap();
    assert_eq!(
        s.submit_verification(run, &v).unwrap_err().1,
        "verification_evidence_incomplete"
    );
    s.verification_evidence(run, &a, 4, 1).unwrap();
    let receipt = s.submit_verification(run, &v).unwrap();
    assert_eq!(s.submit_verification(run, &v).unwrap(), receipt);
    assert_eq!(
        s.submit_verification(run, &verdict("failed", vec![a]))
            .unwrap_err()
            .1,
        "verification_verdict_immutable"
    );
    tick(&s);
    assert_eq!(s.goal(&g).unwrap()["state"], "verifying");
    assert!(s.accept_goal(&g, "approve").is_err());
    finish(&s, &r, "completed");
    tick(&s);
    assert_eq!(
        s.goal(&g).unwrap()["verification"]["current"]["state"],
        "passed"
    );
    assert_eq!(
        s.accept_goal(&g, "checked evidence").unwrap()["state"],
        "accepted"
    );
}
#[test]
fn failed_run_cannot_make_its_submitted_pass_effective() {
    for state in ["failed", "interrupted", "cancelled"] {
        let (s, g, _) = setup(2);
        let r = reviewer(&s);
        let a = artifact(&s, &r);
        read(&s, &r, &a);
        s.submit_verification(r["id"].as_str().unwrap(), &verdict("passed", vec![a]))
            .unwrap();
        finish(&s, &r, state);
        tick(&s);
        let goal = s.goal(&g).unwrap();
        assert_eq!(goal["verification"]["current"]["state"], "failed");
        assert_eq!(goal["round"], 1);
        assert_eq!(
            s.accept_goal(&g, "approve").unwrap_err().1,
            "verification_not_passed"
        );
    }
}
#[test]
fn frozen_context_becomes_stale_and_retry_binds_new_revision() {
    let (s, g, _) = setup(0);
    let r = reviewer(&s);
    let a = artifact(&s, &r);
    read(&s, &r, &a);
    context(&s, &g);
    let frozen = s
        .verification_job(r["id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(frozen["context_revision"], 0);
    s.submit_verification(r["id"].as_str().unwrap(), &verdict("passed", vec![a]))
        .unwrap();
    finish(&s, &r, "completed");
    tick(&s);
    assert_eq!(
        s.goal(&g).unwrap()["verification"]["current"]["state"],
        "stale"
    );
    let input = Retry {
        expected_round: 1,
        expected_verification_id: frozen["id"].as_str().unwrap().into(),
    };
    let receipt = s.retry_verification(&g, &input, "retry").unwrap();
    assert_eq!(s.retry_verification(&g, &input, "retry").unwrap(), receipt);
    let next = reviewer(&s);
    assert_ne!(next["instance_id"], r["instance_id"]);
    assert_eq!(
        s.verification_job(next["id"].as_str().unwrap())
            .unwrap()
            .unwrap()["context_revision"],
        1
    );
    let a = artifact(&s, &next);
    read(&s, &next, &a);
    s.submit_verification(next["id"].as_str().unwrap(), &verdict("passed", vec![a]))
        .unwrap();
    finish(&s, &next, "completed");
    tick(&s);
    assert_eq!(
        s.accept_goal_context(&g, "reviewed latest context", 1, 1)
            .unwrap()["state"],
        "accepted"
    );
}
#[test]
fn later_context_change_invalidates_a_settled_pass() {
    let (s, g, _) = setup(0);
    let r = reviewer(&s);
    let a = artifact(&s, &r);
    read(&s, &r, &a);
    s.submit_verification(r["id"].as_str().unwrap(), &verdict("passed", vec![a]))
        .unwrap();
    finish(&s, &r, "completed");
    tick(&s);
    context(&s, &g);
    assert_eq!(s.goal(&g).unwrap()["verification"]["stale"], true);
    assert_eq!(
        s.accept_goal_context(&g, "approve", 1, 1).unwrap_err().1,
        "verification_stale"
    );
}
#[test]
fn unknown_never_repairs_and_reverification_has_a_finite_limit() {
    let (s, g, _) = setup(2);
    for attempt in 1..=3 {
        let r = reviewer(&s);
        s.submit_verification(r["id"].as_str().unwrap(), &verdict("unknown", vec![]))
            .unwrap();
        finish(&s, &r, "completed");
        tick(&s);
        let goal = s.goal(&g).unwrap();
        assert_eq!(goal["round"], 1);
        assert_eq!(goal["verification"]["current"]["state"], "inconclusive");
        assert_eq!(goal["verification"]["current"]["attempt"], attempt);
        let result = s.retry_verification(
            &g,
            &Retry {
                expected_round: 1,
                expected_verification_id: goal["verification"]["current"]["id"]
                    .as_str()
                    .unwrap()
                    .into(),
            },
            &format!("retry-{attempt}"),
        );
        if attempt < 3 {
            result.unwrap();
        } else {
            assert_eq!(result.unwrap_err().1, "verification_attempt_limit");
        }
    }
}
#[test]
fn failed_check_repairs_with_a_fresh_team_only_within_authorized_budget() {
    let (s, g, p) = setup(1);
    let r = reviewer(&s);
    s.submit_verification(r["id"].as_str().unwrap(), &verdict("failed", vec![]))
        .unwrap();
    finish(&s, &r, "completed");
    tick(&s);
    let goal = s.goal(&g).unwrap();
    assert_eq!(goal["round"], 2);
    assert_eq!(goal["verification"]["history"][0]["auto_rework_round"], 2);
    let producer = reviewer(&s);
    assert!(
        s.verification_job(producer["id"].as_str().unwrap())
            .unwrap()
            .is_none()
    );
    assert_ne!(producer["instance_id"], p["instance_id"]);
    assert!(
        producer["input"]
            .as_str()
            .unwrap()
            .contains("Independent verification")
    );
    finish(&s, &producer, "completed");
    tick(&s);
    let next = reviewer(&s);
    s.submit_verification(next["id"].as_str().unwrap(), &verdict("failed", vec![]))
        .unwrap();
    finish(&s, &next, "completed");
    tick(&s);
    let goal = s.goal(&g).unwrap();
    assert_eq!(goal["round"], 2);
    assert_eq!(goal["state"], "awaiting-review");
    assert_eq!(
        goal["verification"]["current"]["state"],
        "changes-requested"
    );
}
#[test]
fn cancellation_drains_running_verifier_and_rejects_late_report() {
    let (s, g, _) = setup(0);
    let r = reviewer(&s);
    s.cancel_goal(&g).unwrap();
    assert_eq!(
        s.submit_verification(r["id"].as_str().unwrap(), &verdict("unknown", vec![]))
            .unwrap_err()
            .1,
        "verification_run_not_authorized"
    );
    tick(&s);
    assert_eq!(s.goal(&g).unwrap()["state"], "cancelling");
    finish(&s, &r, "cancelled");
    tick(&s);
    assert_eq!(s.goal(&g).unwrap()["state"], "cancelled");
}

#[test]
fn restart_preserves_queued_job_but_does_not_promote_an_unknown_run_report() {
    let (s, g, _) = setup(0);
    tick(&s);
    tick(&s);
    let queued = s.goal(&g).unwrap()["verification"]["current"].clone();
    assert!(queued["run_id"].is_string());
    s.recover_startup().unwrap();
    tick(&s);
    assert_eq!(
        s.goal(&g).unwrap()["verification"]["current"]["id"],
        queued["id"]
    );
    let r = s.claim().unwrap().unwrap();
    let a = artifact(&s, &r);
    read(&s, &r, &a);
    s.submit_verification(r["id"].as_str().unwrap(), &verdict("passed", vec![a]))
        .unwrap();
    s.recover_startup().unwrap();
    tick(&s);
    let goal = s.goal(&g).unwrap();
    assert_eq!(goal["verification"]["current"]["state"], "failed");
    assert_eq!(goal["verification"]["history"].as_array().unwrap().len(), 1);
    assert_eq!(
        s.run(r["id"].as_str().unwrap()).unwrap()["state"],
        "interrupted"
    );
    assert!(s.claim().unwrap().is_none());
}
#[test]
fn exact_check_keys_and_evidence_scope_are_enforced() {
    let (s, _, _) = setup(0);
    let r = reviewer(&s);
    let run = r["id"].as_str().unwrap();
    let mut v = verdict("unknown", vec![]);
    v.checks[0].key = "invented".into();
    assert_eq!(
        s.submit_verification(run, &v).unwrap_err().1,
        "verification_checks_mismatch"
    );
    assert_eq!(
        s.submit_verification(run, &verdict("passed", vec![]))
            .unwrap_err()
            .1,
        "verification_evidence_required"
    );
    assert_eq!(
        s.submit_verification(run, &verdict("failed", vec!["foreign".into()]))
            .unwrap_err()
            .1,
        "verification_artifact_out_of_scope"
    );
    let a = artifact(&s, &r);
    assert_eq!(
        s.verification_evidence(run, &a, 1000000, 1).unwrap_err().1,
        "invalid_evidence_range"
    );
    assert_eq!(
        s.verification_evidence(run, &a, 0, 12001).unwrap_err().1,
        "invalid_evidence_range"
    );
}
