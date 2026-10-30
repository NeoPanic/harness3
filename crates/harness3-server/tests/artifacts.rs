use harness3_platform::{artifacts::ReworkInput, domain::*, store::Store};
use serde_json::{Value, json};
fn setup() -> (Store, String) {
    let s = Store::memory().unwrap();
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
    let f = s
        .spawn(
            &Spawn {
                preset_name: "worker".into(),
                preset_version: "1".into(),
                count: 3,
            },
            "initial",
        )
        .unwrap();
    let g = s
        .create_goal(
            &GoalPlan {
                verification: None,
                title: "joint evidence".into(),
                objective: "compare".into(),
                acceptance: "check".into(),
                tasks: (0..3)
                    .map(|i| TaskPlan {
                        key: format!("task-{i}"),
                        instance_id: f["instance_ids"][i].as_str().unwrap().into(),
                        instructions: format!("analyze {i}"),
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
    (s, g["id"].as_str().unwrap().into())
}
fn work(s: &Store, output: &str) -> Vec<Value> {
    let mut works = Vec::new();
    for _ in 0..3 {
        s.reconcile_fleets().unwrap();
        s.reconcile_goals().unwrap();
        let w = s.claim().unwrap().unwrap();
        s.finish(
            w["id"].as_str().unwrap(),
            w["instance_id"].as_str().unwrap(),
            "completed",
            json!({"output":output}),
        )
        .unwrap();
        works.push(w);
    }
    s.reconcile_goals().unwrap();
    works
}
fn rework(round: i64) -> ReworkInput {
    ReworkInput {
        task_keys: vec![],
        expected_round: round,
        note: "补充证据，区分事实和假设".into(),
        assignments: vec![],
    }
}
#[test]
fn contributions_are_immutable_versioned_and_review_pins_a_manifest() {
    let (s, id) = setup();
    let originals = work(&s, "first contribution 🧪");
    let first = s.goal(&id).unwrap();
    assert_eq!(first["artifacts"].as_array().unwrap().len(), 3);
    let aid = first["tasks"][0]["artifact"]["id"].as_str().unwrap();
    let artifact = s.artifact(aid).unwrap();
    assert_eq!(artifact["content"], "first contribution 🧪");
    assert_eq!(artifact["integrity_verified"], true);
    let receipt = s.rework_goal(&id, &rework(1), "again").unwrap();
    assert_eq!(receipt["round"], 2);
    assert_eq!(s.rework_goal(&id, &rework(1), "again").unwrap(), receipt);
    s.reconcile_goals().unwrap();
    assert!(s.claim().unwrap().is_none());
    let replacements = work(&s, "revised contribution");
    for w in replacements {
        assert!(
            !originals
                .iter()
                .any(|old| old["instance_id"] == w["instance_id"])
        );
        let input = w["input"].as_str().unwrap();
        assert!(input.contains("补充证据") && input.contains("first contribution"));
    }
    assert_eq!(s.artifact(aid).unwrap(), artifact);
    let g = s.goal(&id).unwrap();
    assert_eq!(g["round"], 2);
    assert_eq!(g["artifacts"].as_array().unwrap().len(), 6);
    assert_eq!(g["rounds"][0]["state"], "changes-requested");
    assert_eq!(g["rounds"][0]["tasks"].as_array().unwrap().len(), 3);
    assert_eq!(
        s.accept_goal(&id, "stale review").unwrap_err().1,
        "goal_round_conflict"
    );
    s.accept_goal_round(&id, "核对第二版", 2).unwrap();
    let g = s.goal(&id).unwrap();
    assert_eq!(g["rounds"][1]["manifest"].as_array().unwrap().len(), 3);
    assert_eq!(
        s.artifacts(Some(&id), 0, 2).unwrap()["artifacts"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let page = s.artifacts(Some(&id), 0, 2).unwrap();
    let after = page["next_cursor"].as_i64().unwrap();
    let next = s.artifacts(Some(&id), after, 2).unwrap();
    assert_ne!(page["artifacts"][0]["id"], next["artifacts"][0]["id"]);
    let managed = s
        .list("instances")
        .unwrap()
        .into_iter()
        .filter(|a| !originals.iter().any(|w| w["instance_id"] == a["id"]))
        .collect::<Vec<_>>();
    assert!(managed.iter().all(|a| a["state"] == "terminated"));
}
#[test]
fn validation_blocks_empty_text_and_a_fresh_round_can_fix_it() {
    let (s, id) = setup();
    work(&s, " \n ");
    let g = s.goal(&id).unwrap();
    assert_eq!(g["tasks"][0]["artifact"]["checks"][0]["passed"], false);
    assert_eq!(
        s.accept_goal(&id, "looks done").unwrap_err().1,
        "goal_artifact_validation_failed"
    );
    s.rework_goal(&id, &rework(1), "fix").unwrap();
    work(&s, "real contribution");
    s.accept_goal_round(&id, "checked", 2).unwrap();
    assert_eq!(s.goal(&id).unwrap()["state"], "accepted");
}
#[test]
fn rework_is_bounded_and_stale_keys_do_not_create_more_teams() {
    let (s, id) = setup();
    work(&s, "v1");
    s.rework_goal(&id, &rework(1), "round2").unwrap();
    work(&s, "v2");
    s.rework_goal(&id, &rework(2), "round3").unwrap();
    work(&s, "v3");
    let before = s.list("instances").unwrap().len();
    assert_eq!(
        s.rework_goal(&id, &rework(3), "over").unwrap_err().1,
        "invalid_goal_rework"
    );
    assert_eq!(s.list("instances").unwrap().len(), before);
    assert_eq!(
        s.rework_goal(&id, &rework(1), "other-key").unwrap_err().1,
        "goal_round_conflict"
    );
    let mut changed = rework(1);
    changed.note.push('x');
    assert_eq!(
        s.rework_goal(&id, &changed, "round2").unwrap_err().1,
        "idempotency_conflict"
    );
}
#[test]
fn capacity_rejection_rolls_back_round_and_all_reservations() {
    let (s, id) = setup();
    work(&s, "valid");
    s.spawn(
        &Spawn {
            preset_name: "worker".into(),
            preset_version: "1".into(),
            count: harness3_platform::limits::INSTANCES - 4,
        },
        "fill",
    )
    .unwrap();
    let before = s.capacity().unwrap();
    assert_eq!(
        s.rework_goal(&id, &rework(1), "no-room").unwrap_err().1,
        "instance_quota_exceeded"
    );
    assert_eq!(s.capacity().unwrap(), before);
    let g = s.goal(&id).unwrap();
    assert_eq!(g["round"], 1);
    assert_eq!(g["state"], "awaiting-review");
    assert_eq!(g["rounds"].as_array().unwrap().len(), 1);
}
#[test]
fn cancel_before_provisioning_releases_reserved_capacity_without_retiring_originals() {
    let (s, id) = setup();
    let original = work(&s, "valid");
    let before = s.capacity().unwrap();
    s.rework_goal(&id, &rework(1), "round2").unwrap();
    assert_ne!(s.capacity().unwrap(), before);
    s.cancel_goal(&id).unwrap();
    s.reconcile_fleets().unwrap();
    assert_eq!(s.capacity().unwrap()["reserved"], before["reserved"]);
    assert!(
        s.list("instances")
            .unwrap()
            .iter()
            .filter(|a| original.iter().any(|w| w["instance_id"] == a["id"]))
            .all(|a| a["state"] == "ready")
    );
    assert_eq!(s.goal(&id).unwrap()["state"], "cancelled");
}
#[test]
fn unknown_tool_effect_never_becomes_a_published_contribution_or_automatic_retry() {
    let (s, id) = setup();
    s.reconcile_goals().unwrap();
    let w = s.claim().unwrap().unwrap();
    s.finish(w["id"].as_str().unwrap(),w["instance_id"].as_str().unwrap(),"completed",json!({"output":"looks complete","observations":[{"output":{"content":{"outcome_unknown":true}}}]})).unwrap();
    s.reconcile_goals().unwrap();
    let g = s.goal(&id).unwrap();
    assert_eq!(g["tasks"][0]["artifact"], Value::Null);
    assert!(s.rework_goal(&id, &rework(1), "not-reviewed").is_err());
    assert!(
        s.artifacts(Some(&id), 0, 100).unwrap()["artifacts"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn rework_survives_reopen_and_keeps_one_round_one_team_and_original_artifacts() {
    let (seed, source_id) = setup();
    let source = seed.goal(&source_id).unwrap();
    let path =
        std::env::temp_dir().join(format!("harness3-rework-{}.sqlite", uuid::Uuid::new_v4()));
    let s = Store::open(&path).unwrap();
    s.publish(
        harness3_platform::presets::runtime_preset(&seed.list("presets").unwrap()[0]["definition"])
            .unwrap(),
    )
    .unwrap();
    let fleet = s
        .spawn(
            &Spawn {
                preset_name: "worker".into(),
                preset_version: "1".into(),
                count: 3,
            },
            "initial",
        )
        .unwrap();
    let tasks = source["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, t)| TaskPlan {
            key: t["key"].as_str().unwrap().into(),
            instance_id: fleet["instance_ids"][i].as_str().unwrap().into(),
            instructions: t["instructions"].as_str().unwrap().into(),
            depends_on: serde_json::from_value(t["depends_on"].clone()).unwrap(),
            ..Default::default()
        })
        .collect();
    let goal = s
        .create_goal(
            &GoalPlan {
                verification: None,
                title: "durable rework".into(),
                objective: "compare".into(),
                acceptance: "check".into(),
                tasks,
            },
            "goal",
        )
        .unwrap();
    let id = goal["id"].as_str().unwrap();
    work(&s, "original 🧪");
    let first = s.goal(id).unwrap()["artifacts"].clone();
    let receipt = s.rework_goal(id, &rework(1), "same-key").unwrap();
    let capacity = s.capacity().unwrap();
    drop(s);
    let s = Store::open(&path).unwrap();
    assert_eq!(s.goal(id).unwrap()["artifacts"], first);
    assert_eq!(s.rework_goal(id, &rework(1), "same-key").unwrap(), receipt);
    assert_eq!(s.capacity().unwrap(), capacity);
    work(&s, "revised");
    assert_eq!(
        s.goal(id).unwrap()["artifacts"].as_array().unwrap().len(),
        6
    );
    drop(s);
    let s = Store::open(&path).unwrap();
    assert_eq!(s.goal(id).unwrap()["rounds"].as_array().unwrap().len(), 2);
    assert_eq!(s.goal(id).unwrap()["round"], 2);
    let legacy_run = s.goal(id).unwrap()["tasks"][0]["run_id"]
        .as_str()
        .unwrap()
        .to_owned();
    drop(s);
    // Older binaries could settle a successful answer despite an unknown tool effect.
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute("DELETE FROM artifacts WHERE run_id=?", [&legacy_run])
        .unwrap();
    db.execute("UPDATE runs SET report=? WHERE id=?",rusqlite::params![json!({"output":"legacy answer","observations":[{"output":{"content":{"outcome_unknown":true}}}]}).to_string(),legacy_run]).unwrap();
    drop(db);
    let s = Store::open(&path).unwrap();
    assert_eq!(s.goal(id).unwrap()["tasks"][0]["artifact"], Value::Null);
    assert_eq!(
        s.accept_goal_round(id, "cannot trust this", 2)
            .unwrap_err()
            .1,
        "goal_artifacts_missing"
    );
    assert_eq!(
        s.rework_goal(id, &rework(2), "unsafe-retry").unwrap_err().1,
        "goal_unknown_outcome"
    );
    drop(s);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn partial_rework_closes_downstream_and_pins_reused_provenance_across_rounds() {
    let (s, id) = setup();
    work(&s, "original evidence");
    let original = s.goal(&id).unwrap();
    let mut input = rework(1);
    input.task_keys = vec!["task-0".into()];
    let receipt = s.rework_goal(&id, &input, "partial").unwrap();
    assert_eq!(
        receipt["rework_scope"]["rerun"],
        json!(["task-0", "task-2"])
    );
    assert_eq!(
        receipt["rework_scope"]["reused"][0]["run_id"],
        original["tasks"][1]["run_id"]
    );
    assert_eq!(s.rework_goal(&id, &input, "partial").unwrap(), receipt);
    s.recover_startup().unwrap();
    for key in ["task-0", "task-2"] {
        s.reconcile_fleets().unwrap();
        s.reconcile_goals().unwrap();
        let run = s.claim().unwrap().unwrap();
        assert!(run["input"].as_str().unwrap().contains(key));
        if key == "task-2" {
            assert!(
                run["input"]
                    .as_str()
                    .unwrap()
                    .contains(original["tasks"][1]["run_id"].as_str().unwrap())
            );
        }
        assert!(
            !original["tasks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t["instance_id"] == run["instance_id"])
        );
        s.finish(
            run["id"].as_str().unwrap(),
            run["instance_id"].as_str().unwrap(),
            "completed",
            json!({"output":"revised evidence"}),
        )
        .unwrap();
    }
    s.reconcile_goals().unwrap();
    assert!(s.claim().unwrap().is_none());
    let second = s.goal(&id).unwrap();
    assert_eq!(second["state"], "awaiting-review");
    assert_eq!(second["artifacts"].as_array().unwrap().len(), 5);
    assert_eq!(second["tasks"][1]["run_id"], original["tasks"][1]["run_id"]);
    assert_eq!(
        second["tasks"][1]["artifact"],
        original["tasks"][1]["artifact"]
    );
    let mut third = rework(2);
    third.task_keys = vec!["task-2".into()];
    let next = s.rework_goal(&id, &third, "synthesis-only").unwrap();
    assert_eq!(next["rework_scope"]["reused"].as_array().unwrap().len(), 2);
    s.reconcile_fleets().unwrap();
    s.reconcile_goals().unwrap();
    let run = s.claim().unwrap().unwrap();
    s.finish(
        run["id"].as_str().unwrap(),
        run["instance_id"].as_str().unwrap(),
        "completed",
        json!({"output":"final synthesis"}),
    )
    .unwrap();
    s.reconcile_goals().unwrap();
    s.accept_goal_round(&id, "checked mixed version evidence", 3)
        .unwrap();
    let final_goal = s.goal(&id).unwrap();
    let manifest = final_goal["rounds"][2]["manifest"].as_array().unwrap();
    assert!(
        manifest
            .iter()
            .any(|a| a["artifact_id"] == original["tasks"][1]["artifact"]["id"])
    );
    assert_eq!(
        final_goal["rounds"][1]["rework_scope"],
        receipt["rework_scope"]
    );
}
#[test]
fn invalid_partial_selection_is_atomic_and_legacy_serialization_stays_unchanged() {
    let (s, id) = setup();
    work(&s, "evidence");
    let original = s.goal(&id).unwrap();
    assert!(
        serde_json::to_value(rework(1))
            .unwrap()
            .get("task_keys")
            .is_none()
    );
    for keys in [
        vec!["missing".into()],
        vec!["task-0".into(), "task-0".into()],
    ] {
        let mut input = rework(1);
        input.task_keys = keys;
        assert_eq!(
            s.rework_goal(&id, &input, "bad").unwrap_err().1,
            "invalid_rework_task_selection"
        );
        assert_eq!(s.goal(&id).unwrap(), original);
    }
}

#[test]
fn partial_rework_refuses_invalid_carried_evidence_without_allocating_a_round() {
    let (s, id) = setup();
    work(&s, "");
    let before = s.goal(&id).unwrap();
    let mut input = rework(1);
    input.task_keys = vec!["task-0".into()];
    assert_eq!(
        s.rework_goal(&id, &input, "invalid-carry").unwrap_err().1,
        "rework_reused_artifact_invalid"
    );
    assert_eq!(s.goal(&id).unwrap(), before);
    input.task_keys.clear();
    assert_eq!(
        s.rework_goal(&id, &input, "repair-all").unwrap()["round"],
        2
    );
}
