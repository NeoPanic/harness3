use harness3_platform::{
    delegation::{Child, Plan, Policy, Role},
    domain::*,
    store::Store,
};
use serde_json::{Value, json};
fn seed(s: &Store) -> String {
    s.publish_definition(json!({"name":"delegate","version":"1","purpose":"test delegation","objective":"inspect contributions","instructions":"delegate within bounds","protocol":"demo","plugins":[{"id":"platform.delegation","version":"1.0.0"}]})).unwrap();
    s.spawn(
        &Spawn {
            preset_name: "delegate".into(),
            preset_version: "1".into(),
            count: 1,
        },
        "root",
    )
    .unwrap()["instance_ids"][0]
        .as_str()
        .unwrap()
        .into()
}
fn policy(depth: usize, budget: usize) -> Policy {
    Policy {
        max_depth: depth,
        max_children: 2,
        max_descendants: budget,
        roles: vec![Role {
            name: "researcher".into(),
            preset_name: "delegate".into(),
            preset_version: "1".into(),
            parameters: Default::default(),
        }],
    }
}
fn create(s: &Store, agent: &str, depth: usize, budget: usize) -> String {
    s.create_goal(
        &GoalPlan {
            title: "dynamic research".into(),
            objective: "independent contributions then resume".into(),
            acceptance: "verify provenance".into(),
            tasks: vec![
                TaskPlan {
                    key: "root".into(),
                    instance_id: agent.into(),
                    instructions: "research then synthesize".into(),
                    delegation: Some(policy(depth, budget)),
                    ..Default::default()
                },
                TaskPlan {
                    key: "downstream".into(),
                    instance_id: agent.into(),
                    instructions: "inspect final contribution".into(),
                    depends_on: vec!["root".into()],
                    ..Default::default()
                },
            ],
        },
        "goal",
    )
    .unwrap()["id"]
        .as_str()
        .unwrap()
        .into()
}
fn plan() -> Plan {
    Plan {
        tasks: (0..2)
            .map(|n| Child {
                role: "researcher".into(),
                title: format!("dimension {n}"),
                instructions: format!("research dimension {n}"),
            })
            .collect(),
        continuation: "compare child evidence and cite their run IDs".into(),
    }
}
fn tick(s: &Store) {
    s.reconcile_fleets().unwrap();
    s.reconcile_goals().unwrap();
}
fn claim(s: &Store) -> Value {
    s.claim().unwrap().unwrap()
}
fn finish(s: &Store, r: &Value, state: &str) {
    s.finish(
        r["id"].as_str().unwrap(),
        r["instance_id"].as_str().unwrap(),
        state,
        json!({"output":"evidence from actual task"}),
    )
    .unwrap();
}
fn runid(r: &Value) -> &str {
    r["id"].as_str().unwrap()
}
#[test]
fn children_wait_for_source_and_parent_resumes_before_downstream() {
    let s = Store::memory().unwrap();
    let a = seed(&s);
    let g = create(&s, &a, 2, 6);
    tick(&s);
    let root = claim(&s);
    let receipt = s.submit_delegation(runid(&root), &plan()).unwrap();
    assert_eq!(s.submit_delegation(runid(&root), &plan()).unwrap(), receipt);
    tick(&s);
    assert!(s.claim().unwrap().is_none());
    assert_eq!(s.goal(&g).unwrap()["tasks"][0]["state"], "delegating");
    finish(&s, &root, "completed");
    tick(&s);
    let children = (0..2).map(|_| claim(&s)).collect::<Vec<_>>();
    assert!(children.iter().all(|r| r["instance_id"] != a));
    assert!(s.claim().unwrap().is_none());
    let snapshot = s.goal(&g).unwrap();
    assert_eq!(snapshot["tasks"].as_array().unwrap().len(), 4);
    assert!(snapshot["artifacts"].as_array().unwrap().is_empty());
    for child in &children {
        finish(&s, child, "completed");
    }
    tick(&s);
    let resume = claim(&s);
    assert_eq!(resume["instance_id"], a);
    assert_ne!(resume["id"], root["id"]);
    assert!(
        resume["input"]
            .as_str()
            .unwrap()
            .contains(runid(&children[0]))
    );
    assert_eq!(
        s.delegation_scope(runid(&resume)).unwrap()["reason"],
        "already_delegated"
    );
    finish(&s, &resume, "completed");
    tick(&s);
    let downstream = claim(&s);
    assert!(
        downstream["input"]
            .as_str()
            .unwrap()
            .contains(runid(&resume))
    );
    assert!(!downstream["input"].as_str().unwrap().contains("activation"));
    finish(&s, &downstream, "completed");
    tick(&s);
    let goal = s.goal(&g).unwrap();
    assert_eq!(goal["state"], "awaiting-review");
    assert_eq!(goal["artifacts"].as_array().unwrap().len(), 4);
    assert!(
        goal["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["run_id"] != root["id"])
    );
    let root_task = goal["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["key"] == "root")
        .unwrap();
    assert_eq!(root_task["attempts"].as_array().unwrap().len(), 2);
    assert_eq!(root_task["instructions"], "research then synthesize");
    assert_eq!(root_task["delegations"][0]["source_run_id"], root["id"]);
}
#[test]
fn concurrent_children_share_root_quota_and_depth_bound() {
    let s = Store::memory().unwrap();
    let a = seed(&s);
    let g = create(&s, &a, 2, 4);
    tick(&s);
    let root = claim(&s);
    s.submit_delegation(runid(&root), &plan()).unwrap();
    finish(&s, &root, "completed");
    tick(&s);
    let c1 = claim(&s);
    let c2 = claim(&s);
    s.submit_delegation(runid(&c1), &plan()).unwrap();
    assert_eq!(
        s.submit_delegation(runid(&c2), &plan()).unwrap_err().1,
        "delegation_task_limit"
    );
    finish(&s, &c1, "completed");
    finish(&s, &c2, "completed");
    tick(&s);
    let leaf = claim(&s);
    assert_eq!(
        s.delegation_scope(runid(&leaf)).unwrap()["reason"],
        "depth_limit"
    );
    assert_eq!(
        s.submit_delegation(runid(&leaf), &plan()).unwrap_err().1,
        "delegation_depth_limit"
    );
    assert_eq!(s.goal(&g).unwrap()["tasks"].as_array().unwrap().len(), 6);
}
#[test]
fn unauthorized_role_rolls_back_every_reservation_and_plan_is_immutable() {
    let s = Store::memory().unwrap();
    let a = seed(&s);
    create(&s, &a, 1, 2);
    tick(&s);
    let r = claim(&s);
    let mut bad = plan();
    bad.tasks[1].role = "forbidden".into();
    assert_eq!(
        s.submit_delegation(runid(&r), &bad).unwrap_err().1,
        "delegation_role_not_allowed"
    );
    let count: i64 = s
        .transaction(|tx| Ok(tx.query_row("SELECT count(*) FROM fleets", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(count, 1);
    s.submit_delegation(runid(&r), &plan()).unwrap();
    let mut changed = plan();
    changed.continuation = "different".into();
    assert_eq!(
        s.submit_delegation(runid(&r), &changed).unwrap_err().1,
        "delegation_plan_immutable"
    );
    assert_eq!(
        s.submit_delegation("forged-run", &plan()).unwrap_err().1,
        "delegation_run_not_authorized"
    );
}
#[test]
fn failed_source_never_expands_and_retires_its_reserved_team() {
    for state in ["failed", "interrupted", "cancelled"] {
        let s = Store::memory().unwrap();
        let a = seed(&s);
        let g = create(&s, &a, 1, 2);
        tick(&s);
        let r = claim(&s);
        let receipt = s.submit_delegation(runid(&r), &plan()).unwrap();
        finish(&s, &r, state);
        tick(&s);
        let goal = s.goal(&g).unwrap();
        assert_eq!(goal["state"], "failed");
        assert_eq!(goal["tasks"].as_array().unwrap().len(), 2);
        assert_eq!(goal["tasks"][0]["delegations"][0]["state"], "abandoned");
        for child in receipt["children"].as_array().unwrap() {
            assert!(
                ["draining", "terminated"].contains(
                    &s.fleet(child["fleet_id"].as_str().unwrap()).unwrap()["state"]
                        .as_str()
                        .unwrap()
                )
            );
        }
    }
}
#[test]
fn failed_child_blocks_parent_continuation_and_downstream() {
    let s = Store::memory().unwrap();
    let a = seed(&s);
    let g = create(&s, &a, 1, 2);
    tick(&s);
    let r = claim(&s);
    s.submit_delegation(runid(&r), &plan()).unwrap();
    finish(&s, &r, "completed");
    tick(&s);
    let c1 = claim(&s);
    let c2 = claim(&s);
    finish(&s, &c1, "failed");
    finish(&s, &c2, "completed");
    tick(&s);
    assert!(s.claim().unwrap().is_none());
    let goal = s.goal(&g).unwrap();
    assert_eq!(goal["state"], "failed");
    assert_eq!(
        goal["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["key"] == "root")
            .unwrap()["state"],
        "blocked"
    );
}
#[test]
fn cancel_during_handoff_prevents_expansion_and_releases_capacity() {
    let s = Store::memory().unwrap();
    let a = seed(&s);
    let g = create(&s, &a, 1, 2);
    tick(&s);
    let r = claim(&s);
    s.submit_delegation(runid(&r), &plan()).unwrap();
    finish(&s, &r, "completed");
    s.cancel_goal(&g).unwrap();
    tick(&s);
    assert_eq!(s.goal(&g).unwrap()["state"], "cancelled");
    assert_eq!(s.goal(&g).unwrap()["tasks"].as_array().unwrap().len(), 2);
    assert!(s.claim().unwrap().is_none());
    assert_eq!(
        s.transaction(|tx| Ok(tx.query_row(
            "SELECT count(*) FROM fleet_slots WHERE enabled=1 AND fulfilled=0",
            [],
            |r| r.get::<_, i64>(0)
        )?))
        .unwrap(),
        0
    );
}
#[test]
fn restart_activates_known_success_once_and_never_publishes_planning_as_artifact() {
    let dir = std::env::temp_dir().join(format!("h3-delegation-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("store.sqlite");
    let (g, r) = {
        let s = Store::open(&path).unwrap();
        let a = seed(&s);
        let g = create(&s, &a, 1, 2);
        tick(&s);
        let r = claim(&s);
        s.submit_delegation(runid(&r), &plan()).unwrap();
        finish(&s, &r, "completed");
        (g, r)
    };
    let s = Store::open(&path).unwrap();
    s.recover_startup().unwrap();
    assert!(
        s.goal(&g).unwrap()["artifacts"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    tick(&s);
    tick(&s);
    assert_eq!(s.goal(&g).unwrap()["tasks"].as_array().unwrap().len(), 4);
    assert_eq!(
        s.submit_delegation(runid(&r), &plan()).unwrap()["accepted"],
        true
    );
    drop(s);
    std::fs::remove_dir_all(dir).unwrap();
}
#[tokio::test]
async fn real_workers_execute_recursive_delegation_and_rework_without_expanding_again() {
    use harness3::CancellationToken;
    use harness3_platform::{models::Models, runtime};
    use std::{collections::HashMap, sync::Arc, time::Duration};
    use tokio::sync::Mutex;
    let s = Store::memory().unwrap();
    let a = seed(&s);
    let g = create(&s, &a, 2, 6);
    let cancel = CancellationToken::new();
    let workers = tokio::spawn(runtime::worker_loop(
        s.clone(),
        Models::default(),
        Arc::new(Mutex::new(HashMap::new())),
        cancel.clone(),
    ));
    let wait = async {
        loop {
            let goal = s.goal(&g).unwrap();
            if goal["state"] == "awaiting-review" {
                break goal;
            }
            assert_ne!(goal["state"], "failed", "{goal}");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    };
    let goal = tokio::time::timeout(Duration::from_secs(40), wait)
        .await
        .unwrap();
    assert_eq!(goal["tasks"].as_array().unwrap().len(), 8);
    assert_eq!(goal["artifacts"].as_array().unwrap().len(), 8);
    let count: i64 = s
        .transaction(|tx| {
            Ok(tx.query_row("SELECT count(*) FROM task_delegations", [], |r| r.get(0))?)
        })
        .unwrap();
    assert_eq!(count, 3);
    s.rework_goal(
        &g,
        &harness3_platform::artifacts::ReworkInput {
            expected_round: 1,
            note: "verify evidence again".into(),
            assignments: vec![],
        },
        "rework",
    )
    .unwrap();
    tokio::time::timeout(Duration::from_secs(40), async {
        loop {
            let g = s.goal(&g).unwrap();
            if g["state"] == "awaiting-review" {
                assert_eq!(g["round"], 2);
                assert_eq!(g["tasks"].as_array().unwrap().len(), 8);
                break;
            }
            assert_ne!(g["state"], "failed", "{g}");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    cancel.cancel();
    workers.await.unwrap();
}
#[test]
fn interrupted_restart_does_not_expand_committed_handoff() {
    let s = Store::memory().unwrap();
    let a = seed(&s);
    let g = create(&s, &a, 1, 2);
    tick(&s);
    let r = claim(&s);
    s.submit_delegation(runid(&r), &plan()).unwrap();
    s.recover_startup().unwrap();
    tick(&s);
    let goal = s.goal(&g).unwrap();
    assert_eq!(goal["state"], "failed");
    assert_eq!(goal["tasks"].as_array().unwrap().len(), 2);
    assert_eq!(goal["tasks"][0]["delegations"][0]["state"], "abandoned");
    assert!(s.claim().unwrap().is_none());
}
#[test]
fn cancellation_winning_before_success_does_not_turn_handoff_into_final_contribution() {
    let s = Store::memory().unwrap();
    let a = seed(&s);
    let g = create(&s, &a, 1, 2);
    tick(&s);
    let r = claim(&s);
    s.submit_delegation(runid(&r), &plan()).unwrap();
    s.cancel(runid(&r)).unwrap();
    finish(&s, &r, "completed");
    tick(&s);
    assert_eq!(s.goal(&g).unwrap()["state"], "failed");
    assert_eq!(s.goal(&g).unwrap()["tasks"][0]["state"], "blocked");
}
#[test]
fn unavailable_child_before_activation_blocks_without_waiting_forever() {
    let s = Store::memory().unwrap();
    let a = seed(&s);
    let g = create(&s, &a, 1, 2);
    tick(&s);
    let r = claim(&s);
    let receipt = s.submit_delegation(runid(&r), &plan()).unwrap();
    s.reconcile_fleets().unwrap();
    let child = s
        .fleet(receipt["children"][0]["fleet_id"].as_str().unwrap())
        .unwrap()["slots"][0]["instance_id"]
        .as_str()
        .unwrap()
        .to_owned();
    s.transition(&child, "terminate").unwrap();
    finish(&s, &r, "completed");
    tick(&s);
    assert_eq!(s.goal(&g).unwrap()["state"], "failed");
}
#[test]
fn concurrent_submit_reuses_one_plan_and_one_reserved_team() {
    let s = Store::memory().unwrap();
    let a = seed(&s);
    create(&s, &a, 1, 2);
    tick(&s);
    let r = claim(&s);
    let handles = (0..2)
        .map(|_| {
            let s = s.clone();
            let run = runid(&r).to_owned();
            std::thread::spawn(move || s.submit_delegation(&run, &plan()).unwrap())
        })
        .collect::<Vec<_>>();
    let results = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results[0], results[1]);
    assert_eq!(
        s.transaction(|tx| Ok(
            tx.query_row("SELECT count(*) FROM fleets", [], |r| r.get::<_, i64>(0))?
        ))
        .unwrap(),
        3
    );
}
#[tokio::test]
async fn research_entry_can_authorize_runtime_delegation() {
    use harness3::CancellationToken;
    use harness3_platform::{models::Models, research::ResearchInput, runtime};
    use std::{collections::HashMap, sync::Arc, time::Duration};
    use tokio::sync::Mutex;
    let s = Store::memory().unwrap();
    seed(&s);
    let input = ResearchInput {
        title: "research with runtime delegation".into(),
        objective: "split after planning".into(),
        acceptance: "each contributor resumes".into(),
        preset_name: "delegate".into(),
        preset_version: "1".into(),
        researchers: 2,
        parameters: Default::default(),
        delegation: Some(policy(1, 2)),
    };
    let job = s.create_research(&input, "delegated-research").unwrap();
    assert_eq!(
        s.create_research(&input, "delegated-research").unwrap(),
        job
    );
    let cancel = CancellationToken::new();
    let workers = tokio::spawn(runtime::worker_loop(
        s.clone(),
        Models::default(),
        Arc::new(Mutex::new(HashMap::new())),
        cancel.clone(),
    ));
    tokio::time::timeout(Duration::from_secs(35), async {
        loop {
            let j = s.research(job["id"].as_str().unwrap()).unwrap();
            if j["state"] == "awaiting-review" {
                assert_eq!(j["goal"]["tasks"].as_array().unwrap().len(), 7);
                assert_eq!(j["delegation"]["max_depth"], 1);
                let tasks = j["goal"]["tasks"].as_array().unwrap();
                assert_eq!(
                    tasks
                        .iter()
                        .filter(|t| !t["delegations"].as_array().unwrap().is_empty())
                        .count(),
                    2
                );
                break;
            }
            assert_ne!(j["state"], "failed", "{j}");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    cancel.cancel();
    workers.await.unwrap();
}
