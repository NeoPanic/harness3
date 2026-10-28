use harness3_platform::{
    domain::*,
    fleets::{FleetAction, FleetPlan},
    runtime,
    store::Store,
};
use serde_json::{Value, json};

fn seeded() -> Store {
    let store = Store::memory().unwrap();
    seed(&store);
    store
}
fn seed(store: &Store) {
    store
        .publish(Preset {
            name: "worker".into(),
            version: "1".into(),
            purpose: "inspect".into(),
            objective: "report".into(),
            instructions: "inspect text".into(),
            protocol: Protocol::Demo,
            plugin: None,
        })
        .unwrap();
}
fn plan(mode: &str, count: usize) -> FleetPlan {
    FleetPlan {
        parameters: Default::default(),
        preset_name: "worker".into(),
        preset_version: "1".into(),
        count,
        mode: mode.into(),
        ttl_seconds: None,
    }
}
fn action(kind: &str, revision: i64, count: Option<usize>) -> FleetAction {
    FleetAction {
        action: kind.into(),
        expected_revision: revision,
        count,
    }
}
fn settle(store: &Store) {
    for _ in 0..10 {
        store.reconcile_fleets().unwrap();
    }
}
fn agent(fleet: &Value, slot: usize) -> &str {
    fleet["slots"][slot]["instance_id"].as_str().unwrap()
}

#[test]
fn creation_is_bounded_durable_and_idempotent() {
    let store = seeded();
    let p = plan("finite", 20);
    let receipt = store.create_fleet(&p, "create").unwrap();
    let id = receipt["fleet_id"].as_str().unwrap();
    let op = receipt["operation_id"].as_str().unwrap();
    assert!(store.list("instances").unwrap().is_empty());
    assert_eq!(store.capacity().unwrap()["reserved"], 20);
    assert_eq!(store.create_fleet(&p, "create").unwrap(), receipt);
    assert_eq!(
        store
            .create_fleet(&plan("maintain", 20), "create")
            .unwrap_err()
            .1,
        "idempotency_conflict"
    );
    store.reconcile_fleets().unwrap();
    assert_eq!(store.fleet(id).unwrap()["current_count"], 8);
    assert_eq!(store.operation(op).unwrap()["state"], "running");
    settle(&store);
    let fleet = store.fleet(id).unwrap();
    assert_eq!(fleet["current_count"], 20);
    assert_eq!(fleet["state"], "active");
    assert_eq!(store.operation(op).unwrap()["state"], "completed");
    store.transition(agent(&fleet, 0), "terminate").unwrap();
    settle(&store);
    assert_eq!(store.fleet(id).unwrap()["current_count"], 19);
    assert_eq!(store.fleet(id).unwrap()["slots"][0]["generation"], 1);
    assert_eq!(store.capacity().unwrap()["reserved"], 19);
}
#[test]
fn maintain_replaces_same_slot_with_new_identity_and_generation() {
    let store = seeded();
    let receipt = store.create_fleet(&plan("maintain", 2), "create").unwrap();
    let id = receipt["fleet_id"].as_str().unwrap();
    settle(&store);
    let first = store.fleet(id).unwrap();
    let original = agent(&first, 0);
    store.transition(original, "terminate").unwrap();
    assert_eq!(store.capacity().unwrap()["reserved"], 2);
    settle(&store);
    let second = store.fleet(id).unwrap();
    assert_ne!(original, agent(&second, 0));
    assert_eq!(second["slots"][0]["generation"], 2);
    assert_eq!(second["current_count"], 2);
    assert_eq!(second["terminated_count"], 1);
    assert_eq!(agent(&first, 1), agent(&second, 1));
}
#[test]
fn pending_reservations_prevent_cross_fleet_and_legacy_overcommit() {
    let store = seeded();
    store.create_fleet(&plan("maintain", 64), "full").unwrap();
    assert_eq!(store.capacity().unwrap()["active"], 0);
    assert_eq!(
        store
            .create_fleet(&plan("finite", 1), "extra")
            .unwrap_err()
            .1,
        "instance_quota_exceeded"
    );
    assert_eq!(
        store
            .spawn(
                &Spawn {
                    preset_name: "worker".into(),
                    preset_version: "1".into(),
                    count: 1
                },
                "legacy"
            )
            .unwrap_err()
            .1,
        "instance_quota_exceeded"
    );
    assert_eq!(store.fleets().unwrap().len(), 1);
    settle(&store);
    assert_eq!(store.capacity().unwrap()["active"], 64);
}
#[test]
fn terminate_supersedes_pending_creation_and_does_not_spawn() {
    let store = seeded();
    let receipt = store.create_fleet(&plan("maintain", 64), "create").unwrap();
    let id = receipt["fleet_id"].as_str().unwrap();
    let stop = store
        .fleet_action(id, &action("terminate", 1, None), "stop")
        .unwrap();
    assert_eq!(
        store
            .operation(receipt["operation_id"].as_str().unwrap())
            .unwrap()["state"],
        "superseded"
    );
    assert_eq!(store.capacity().unwrap()["reserved"], 0);
    settle(&store);
    assert_eq!(store.fleet(id).unwrap()["state"], "terminated");
    assert!(store.list("instances").unwrap().is_empty());
    assert_eq!(
        store
            .operation(stop["operation_id"].as_str().unwrap())
            .unwrap()["state"],
        "completed"
    );
    assert_eq!(
        store
            .fleet_action(id, &action("terminate", 1, None), "stop")
            .unwrap(),
        stop
    );
    assert_eq!(
        store
            .fleet_action(id, &action("scale", 2, Some(1)), "restart")
            .unwrap_err()
            .1,
        "fleet_terminal"
    );
}
#[test]
fn shrink_drains_accepted_work_preserves_receipts_and_keeps_capacity_reserved() {
    let store = seeded();
    let receipt = store.create_fleet(&plan("maintain", 2), "create").unwrap();
    let id = receipt["fleet_id"].as_str().unwrap();
    settle(&store);
    let fleet = store.fleet(id).unwrap();
    let retiring = agent(&fleet, 1);
    let run = store
        .enqueue(
            retiring,
            &RunInput {
                text: "accepted".into(),
            },
            "run",
        )
        .unwrap();
    let scale = store
        .fleet_action(id, &action("scale", 1, Some(1)), "scale")
        .unwrap();
    assert_eq!(store.capacity().unwrap()["reserved"], 2);
    assert!(
        store
            .enqueue(
                retiring,
                &RunInput {
                    text: "late".into()
                },
                "late"
            )
            .is_err()
    );
    settle(&store);
    assert_eq!(
        store
            .operation(scale["operation_id"].as_str().unwrap())
            .unwrap()["state"],
        "running"
    );
    assert_eq!(store.fleet(id).unwrap()["draining_count"], 1);
    let work = store.claim().unwrap().unwrap();
    assert_eq!(work["id"], run["id"]);
    store
        .finish(
            work["id"].as_str().unwrap(),
            retiring,
            "completed",
            json!({"output":"retained"}),
        )
        .unwrap();
    settle(&store);
    assert_eq!(store.capacity().unwrap()["reserved"], 1);
    assert_eq!(store.fleet(id).unwrap()["current_count"], 1);
    assert_eq!(
        store
            .operation(scale["operation_id"].as_str().unwrap())
            .unwrap()["state"],
        "completed"
    );
    assert_eq!(
        store.run(run["id"].as_str().unwrap()).unwrap()["report"]["output"],
        "retained"
    );
}
#[test]
fn revisions_busy_operations_and_idempotency_do_not_overwrite_intent() {
    let store = seeded();
    let receipt = store.create_fleet(&plan("maintain", 1), "create").unwrap();
    let id = receipt["fleet_id"].as_str().unwrap();
    assert_eq!(
        store
            .fleet_action(id, &action("scale", 1, Some(3)), "early")
            .unwrap_err()
            .1,
        "fleet_operation_in_progress"
    );
    settle(&store);
    let scale = store
        .fleet_action(id, &action("scale", 1, Some(3)), "scale")
        .unwrap();
    assert_eq!(
        store
            .fleet_action(id, &action("scale", 1, Some(3)), "scale")
            .unwrap(),
        scale
    );
    assert_eq!(
        store
            .fleet_action(id, &action("scale", 1, Some(4)), "scale")
            .unwrap_err()
            .1,
        "idempotency_conflict"
    );
    assert_eq!(
        store
            .fleet_action(id, &action("scale", 1, Some(4)), "stale")
            .unwrap_err()
            .1,
        "fleet_revision_conflict"
    );
    settle(&store);
    assert_eq!(store.fleet(id).unwrap()["desired_count"], 3);
}
#[test]
fn finite_can_scale_to_zero_then_reactivate_slots_without_automatic_replacement() {
    let store = seeded();
    let receipt = store.create_fleet(&plan("finite", 2), "create").unwrap();
    let id = receipt["fleet_id"].as_str().unwrap();
    settle(&store);
    let initial = store.fleet(id).unwrap();
    store
        .fleet_action(id, &action("scale", 1, Some(0)), "zero")
        .unwrap();
    settle(&store);
    assert_eq!(store.fleet(id).unwrap()["state"], "completed");
    assert_eq!(store.capacity().unwrap()["reserved"], 0);
    store
        .fleet_action(id, &action("scale", 2, Some(3)), "grow")
        .unwrap();
    settle(&store);
    let grown = store.fleet(id).unwrap();
    assert_eq!(grown["current_count"], 3);
    assert_eq!(grown["slots"][0]["generation"], 2);
    assert_eq!(grown["slots"][2]["generation"], 1);
    assert_ne!(agent(&initial, 0), agent(&grown, 0));
}
#[test]
fn rejected_scale_rolls_back_revision_slots_and_operations() {
    let store = seeded();
    let receipt = store.create_fleet(&plan("maintain", 2), "first").unwrap();
    let id = receipt["fleet_id"].as_str().unwrap();
    store.create_fleet(&plan("maintain", 62), "rest").unwrap();
    settle(&store);
    let before = store.fleet(id).unwrap();
    assert_eq!(
        store
            .fleet_action(id, &action("scale", 1, Some(3)), "over")
            .unwrap_err()
            .1,
        "instance_quota_exceeded"
    );
    assert_eq!(store.fleet(id).unwrap(), before);
}
#[test]
fn ttl_maintain_replaces_only_after_old_generation_is_terminal() {
    let store = seeded();
    let mut p = plan("maintain", 1);
    p.ttl_seconds = Some(60);
    let receipt = store.create_fleet(&p, "ttl").unwrap();
    let id = receipt["fleet_id"].as_str().unwrap();
    settle(&store);
    let first = store.fleet(id).unwrap();
    let old = agent(&first, 0);
    let run = store
        .enqueue(
            old,
            &RunInput {
                text: "busy".into(),
            },
            "work",
        )
        .unwrap();
    store.claim().unwrap().unwrap();
    store
        .transaction(|tx| {
            tx.execute("UPDATE instance_deadlines SET expires_at=0", [])?;
            Ok(())
        })
        .unwrap();
    settle(&store);
    assert_eq!(store.fleet(id).unwrap()["current_count"], 1);
    assert_eq!(store.fleet(id).unwrap()["slots"][0]["generation"], 1);
    assert!(store.cancel_requested(run["id"].as_str().unwrap()).unwrap());
    store
        .finish(
            run["id"].as_str().unwrap(),
            old,
            "cancelled",
            json!({"error":{"code":"cancelled"}}),
        )
        .unwrap();
    settle(&store);
    let replaced = store.fleet(id).unwrap();
    assert_eq!(replaced["slots"][0]["generation"], 2);
    assert_ne!(agent(&replaced, 0), old);
}
#[tokio::test]
async fn restart_continues_operations_and_preserves_uncertain_retirement() {
    let path =
        std::env::temp_dir().join(format!("harness3-fleets-{}.sqlite", uuid::Uuid::new_v4()));
    let store = Store::open(&path).unwrap();
    seed(&store);
    let receipt = store.create_fleet(&plan("maintain", 10), "create").unwrap();
    let id = receipt["fleet_id"].as_str().unwrap();
    store.reconcile_fleets().unwrap();
    let fleet = store.fleet(id).unwrap();
    let old = agent(&fleet, 0).to_owned();
    let run = store
        .enqueue(
            &old,
            &RunInput {
                text: "unknown".into(),
            },
            "run",
        )
        .unwrap();
    store.claim().unwrap().unwrap();
    store.transition(&old, "drain").unwrap();
    drop(store);
    let store = Store::open(&path).unwrap();
    runtime::recover(&store).await.unwrap();
    settle(&store);
    assert_eq!(
        store.run(run["id"].as_str().unwrap()).unwrap()["state"],
        "interrupted"
    );
    let restored = store.fleet(id).unwrap();
    assert_eq!(restored["current_count"], 10);
    assert_eq!(restored["slots"][0]["generation"], 2);
    assert_eq!(
        store
            .operation(receipt["operation_id"].as_str().unwrap())
            .unwrap()["state"],
        "completed"
    );
    drop(store);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn concurrent_reconciliation_produces_one_current_generation_per_slot() {
    let store = seeded();
    let receipt = store.create_fleet(&plan("maintain", 16), "create").unwrap();
    let id = receipt["fleet_id"].as_str().unwrap();
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let store = &store;
            scope.spawn(move || settle(store));
        }
    });
    assert_eq!(store.list("instances").unwrap().len(), 16);
    assert!(
        store.fleet(id).unwrap()["slots"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["generation"] == 1)
    );
}

#[tokio::test]
async fn fleet_api_accepts_operations_and_shares_auth_idempotency_and_revisions() {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use harness3_platform::{
        api::{App, router},
        models::Models,
    };
    use std::{collections::HashMap, sync::Arc};
    use tokio::sync::Mutex;
    use tower::ServiceExt;
    let store = seeded();
    let app = router(App {
        plugins: harness3_platform::plugins::ProcessHost::disabled(store.clone()),
        store: store.clone(),
        token: "a".repeat(64),
        models: Models::default(),
        cancellations: Arc::new(Mutex::new(HashMap::new())),
    });
    let request =
        |method: &str, path: &str, body: Option<Value>, authorized: bool, key: Option<&str>| {
            let mut req = Request::builder()
                .method(method)
                .uri(path)
                .header("host", "127.0.0.1:7333");
            if authorized {
                req = req.header("authorization", format!("Bearer {}", "a".repeat(64)));
            }
            if let Some(key) = key {
                req = req.header("idempotency-key", key);
            }
            if let Some(body) = body {
                req.header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap()
            } else {
                req.body(Body::empty()).unwrap()
            }
        };
    assert_eq!(
        app.clone()
            .oneshot(request("GET", "/api/v1/agent-fleets", None, false, None))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let body = serde_json::to_value(plan("maintain", 3)).unwrap();
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/v1/agent-fleets",
            Some(body.clone()),
            true,
            Some("api"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let receipt: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 10000).await.unwrap()).unwrap();
    let repeat = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/v1/agent-fleets",
            Some(body),
            true,
            Some("api"),
        ))
        .await
        .unwrap();
    let repeat: Value =
        serde_json::from_slice(&to_bytes(repeat.into_body(), 10000).await.unwrap()).unwrap();
    assert_eq!(repeat, receipt);
    let operation = receipt["links"]["operation"].as_str().unwrap();
    let response = app
        .clone()
        .oneshot(request("GET", operation, None, true, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    settle(&store);
    let fleet = receipt["links"]["fleet"].as_str().unwrap();
    let response = app
        .clone()
        .oneshot(request("GET", fleet, None, true, None))
        .await
        .unwrap();
    let value: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 20000).await.unwrap()).unwrap();
    assert_eq!(value["current_count"], 3);
    let response = app
        .oneshot(request(
            "POST",
            &format!("{fleet}/actions"),
            Some(json!({"action":"scale","expected_revision":99,"count":1})),
            true,
            Some("stale"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[test]
fn explicit_finite_scale_replenishes_once_without_switching_to_maintain() {
    let store = seeded();
    let receipt = store.create_fleet(&plan("finite", 2), "create").unwrap();
    let id = receipt["fleet_id"].as_str().unwrap();
    settle(&store);
    let first = store.fleet(id).unwrap();
    store.transition(agent(&first, 0), "terminate").unwrap();
    settle(&store);
    assert_eq!(store.fleet(id).unwrap()["current_count"], 1);
    store
        .fleet_action(id, &action("scale", 1, Some(2)), "refill")
        .unwrap();
    settle(&store);
    let refilled = store.fleet(id).unwrap();
    assert_eq!(refilled["current_count"], 2);
    assert_eq!(refilled["slots"][0]["generation"], 2);
    store.transition(agent(&refilled, 0), "terminate").unwrap();
    settle(&store);
    assert_eq!(store.fleet(id).unwrap()["current_count"], 1);
}

#[test]
fn legacy_batches_migrate_to_finite_slots_without_losing_queue_or_deadlines() {
    let path = std::env::temp_dir().join(format!(
        "harness3-fleets-legacy-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let store = Store::open(&path).unwrap();
    seed(&store);
    let input = Spawn {
        preset_name: "worker".into(),
        preset_version: "1".into(),
        count: 2,
    };
    let batch = store.spawn_with_ttl(&input, "legacy", Some(60)).unwrap();
    let original = store.list("instances").unwrap();
    let agent = batch["instance_ids"][0].as_str().unwrap();
    let run = store
        .enqueue(
            agent,
            &RunInput {
                text: "preserve queue".into(),
            },
            "legacy-run",
        )
        .unwrap();
    drop(store);
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("PRAGMA foreign_keys=OFF; DROP TABLE operations; DROP TABLE instance_allocations; DROP TABLE fleet_slots; DROP TABLE fleets; PRAGMA user_version=3;").unwrap();
    drop(connection);
    let store = Store::open(&path).unwrap();
    let fleet = store.fleet(batch["fleet_id"].as_str().unwrap()).unwrap();
    assert_eq!(fleet["mode"], "finite");
    assert_eq!(fleet["slots"].as_array().unwrap().len(), 2);
    assert_eq!(fleet["operations"], json!([]));
    assert_eq!(
        store
            .list("instances")
            .unwrap()
            .iter()
            .map(|a| a["id"].clone())
            .collect::<Vec<_>>(),
        original.iter().map(|a| a["id"].clone()).collect::<Vec<_>>()
    );
    assert!(
        store
            .list("instances")
            .unwrap()
            .iter()
            .all(|a| !a["expires_at"].is_null())
    );
    assert_eq!(
        store.spawn_with_ttl(&input, "legacy", Some(60)).unwrap(),
        batch
    );
    assert_eq!(store.claim().unwrap().unwrap()["id"], run["id"]);
    drop(store);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn legacy_http_idempotency_key_cannot_create_a_second_fleet_after_upgrade() {
    let store = seeded();
    let input = Spawn {
        preset_name: "worker".into(),
        preset_version: "1".into(),
        count: 2,
    };
    let receipt = store
        .spawn_with_ttl(&input, "legacy-http", Some(60))
        .unwrap();
    let mut p = plan("finite", 2);
    p.ttl_seconds = Some(60);
    assert_eq!(store.create_fleet(&p, "legacy-http").unwrap(), receipt);
    p.mode = "maintain".into();
    assert_eq!(
        store.create_fleet(&p, "legacy-http").unwrap_err().1,
        "idempotency_conflict"
    );
    assert_eq!(store.fleets().unwrap().len(), 1);
    assert_eq!(store.list("instances").unwrap().len(), 2);
}
