use harness3_platform::{
    board::{PublishInput, SplitInput},
    domain::*,
    store::Store,
};

use serde_json::{Value, json};
fn input(title: &str) -> PublishInput {
    PublishInput {
        title: title.into(),
        instructions: format!("Inspect {title}"),
        acceptance: "Evidence is reported".into(),
    }
}
fn fixture() -> (Store, Value) {
    let s = Store::memory().unwrap();
    s.publish(Preset {
        name: "reader".into(),
        version: "1".into(),
        purpose: "Read evidence".into(),
        objective: "Report findings".into(),
        instructions: "Inspect supplied evidence".into(),
        protocol: Protocol::Demo,
        plugin: None,
    })
    .unwrap();
    let t = s.publish_board_task(&input("root"), "root").unwrap();
    (s, t)
}
fn id(v: &Value) -> &str {
    v["id"].as_str().unwrap()
}
fn provision(s: &Store, t: &Value) -> Value {
    s.provision_board_task(id(t), "reader", "1", "matches evidence inspection")
        .unwrap();
    s.claim().unwrap().unwrap()
}
fn finish(s: &Store, r: &Value, state: &str) {
    s.finish(
        id(r),
        r["instance_id"].as_str().unwrap(),
        state,
        json!({"output":"Evidence"}),
    )
    .unwrap();
    s.reconcile_board().unwrap();
}
#[test]
fn publish_is_unassigned_and_idempotent() {
    let (s, t) = fixture();
    assert_eq!(s.publish_board_task(&input("root"), "root").unwrap(), t);
    assert_eq!(
        s.publish_board_task(&input("different"), "root")
            .unwrap_err()
            .1,
        "idempotency_conflict"
    );
    assert!(t["instance_id"].is_null());
    assert!(t["preset_name"].is_null());
    assert_eq!(s.board_pending().unwrap().len(), 1);
    let r = provision(&s, &t);
    let pending = s.board_task(id(&t)).unwrap();
    assert_eq!(pending["state"], "pending");
    assert!(pending["instance_id"].is_null());
    assert!(s.board_pending().unwrap().is_empty());
    assert_eq!(
        s.provision_board_task(id(&t), "reader", "1", "duplicate")
            .unwrap_err()
            .1,
        "board_task_not_routable"
    );
    s.claim_board_task(id(&r), id(&t)).unwrap();
    assert_eq!(
        s.claim_board_task(id(&r), id(&t)).unwrap()["state"],
        "running"
    );
    finish(&s, &r, "completed");
    assert_eq!(s.board_task(id(&t)).unwrap()["state"], "completed");
    assert_eq!(
        s.board_snapshot().unwrap()["agents"][0]["state"],
        "terminated"
    );
}
#[test]
fn split_routes_children_then_reoffers_parent_to_any_peer() {
    let (s, t) = fixture();
    let r = provision(&s, &t);
    s.claim_board_task(id(&r), id(&t)).unwrap();
    let split = SplitInput {
        tasks: vec![input("source A"), input("source B")],
    };
    let receipt = s.split_board_task(id(&r), &split, "split").unwrap();
    assert_eq!(
        receipt,
        s.split_board_task(id(&r), &split, "split").unwrap()
    );
    assert!(s.board_pending().unwrap().is_empty());
    finish(&s, &r, "completed");
    assert_eq!(s.board_task(id(&t)).unwrap()["state"], "waiting");
    let children = s.board_pending().unwrap();
    assert_eq!(children.len(), 2);
    for child in children {
        let work = provision(&s, &child);
        s.claim_board_task(id(&work), id(&child)).unwrap();
        finish(&s, &work, "completed");
    }
    let pending = s.board_pending().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0]["id"], t["id"]);
    let resume = provision(&s, &t);
    assert_ne!(resume["instance_id"], r["instance_id"]);
    s.claim_board_task(id(&resume), id(&t)).unwrap();
    assert_eq!(
        s.split_board_task(id(&resume), &split, "again")
            .unwrap_err()
            .1,
        "board_task_already_split"
    );
    assert_eq!(
        s.read_board(id(&resume)).unwrap()["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["state"] == "completed")
            .count(),
        2
    );
    finish(&s, &resume, "completed");
    assert_eq!(s.board_task(id(&t)).unwrap()["state"], "completed");
}
#[test]
fn failed_split_never_launches_children() {
    let (s, t) = fixture();
    let r = provision(&s, &t);
    s.claim_board_task(id(&r), id(&t)).unwrap();
    s.split_board_task(
        id(&r),
        &SplitInput {
            tasks: vec![input("child")],
        },
        "split",
    )
    .unwrap();
    finish(&s, &r, "failed");
    assert!(s.board_pending().unwrap().is_empty());
    assert_eq!(s.board_task(id(&t)).unwrap()["state"], "failed");
    assert_eq!(
        s.board_snapshot().unwrap()["tasks"][1]["state"],
        "cancelled"
    );
}
#[test]
fn failed_child_blocks_parent_and_cancels_siblings() {
    let (s, t) = fixture();
    let r = provision(&s, &t);
    s.claim_board_task(id(&r), id(&t)).unwrap();
    s.split_board_task(
        id(&r),
        &SplitInput {
            tasks: vec![input("child A"), input("child B")],
        },
        "split",
    )
    .unwrap();
    finish(&s, &r, "completed");
    let children = s.board_pending().unwrap();
    let child_run = provision(&s, &children[0]);
    s.claim_board_task(id(&child_run), id(&children[0]))
        .unwrap();
    finish(&s, &child_run, "failed");
    assert_eq!(s.board_task(id(&t)).unwrap()["state"], "blocked");
    assert_eq!(
        s.board_task(id(&children[1])).unwrap()["state"],
        "cancelled"
    );
    assert!(s.board_pending().unwrap().is_empty());
}
#[test]
fn unclaimed_and_interrupted_runs_do_not_retry_forever() {
    let (s, t) = fixture();
    let r = provision(&s, &t);
    finish(&s, &r, "completed");
    assert_eq!(
        s.board_task(id(&t)).unwrap()["error"],
        "board_agent_did_not_claim"
    );
    assert!(s.board_pending().unwrap().is_empty());
    let t = s
        .publish_board_task(&input("uncertain"), "uncertain")
        .unwrap();
    let r = provision(&s, &t);
    s.claim_board_task(id(&r), id(&t)).unwrap();
    s.finish(
        id(&r),
        r["instance_id"].as_str().unwrap(),
        "completed",
        json!({"error":{"code":"interrupted_outcome_unknown"}}),
    )
    .unwrap();
    s.reconcile_board().unwrap();
    assert_eq!(
        s.board_task(id(&t)).unwrap()["error"],
        "interrupted_outcome_unknown"
    );
    assert!(s.board_pending().unwrap().is_empty());
}
#[test]
fn peers_can_claim_each_others_invitation_and_claims_are_exclusive() {
    let (s, t) = fixture();
    let r = provision(&s, &t);
    s.claim_board_task(id(&r), id(&t)).unwrap();
    s.split_board_task(
        id(&r),
        &SplitInput {
            tasks: vec![input("child A"), input("child B")],
        },
        "split",
    )
    .unwrap();
    finish(&s, &r, "completed");
    let children = s.board_pending().unwrap();
    let a = provision(&s, &children[0]);
    let b = provision(&s, &children[1]);
    s.claim_board_task(id(&a), id(&children[1])).unwrap();
    assert_eq!(
        s.claim_board_task(id(&b), id(&children[1])).unwrap_err().1,
        "board_task_already_claimed"
    );
    s.claim_board_task(id(&b), id(&children[0])).unwrap();
    finish(&s, &a, "completed");
    finish(&s, &b, "completed");
    assert_eq!(s.board_task(id(&t)).unwrap()["state"], "pending");
}
#[test]
fn cancel_revokes_tools_and_staged_children() {
    let (s, t) = fixture();
    let r = provision(&s, &t);
    s.claim_board_task(id(&r), id(&t)).unwrap();
    s.split_board_task(
        id(&r),
        &SplitInput {
            tasks: vec![input("child")],
        },
        "split",
    )
    .unwrap();
    s.cancel_board_task(id(&t)).unwrap();
    assert!(s.read_board(id(&r)).is_err());
    assert!(s.cancel_requested(id(&r)).unwrap());
    finish(&s, &r, "completed");
    assert_eq!(s.board_task(id(&t)).unwrap()["state"], "cancelled");
    assert!(s.board_pending().unwrap().is_empty());
}
#[test]
fn cross_root_claim_is_rejected() {
    let (s, t) = fixture();
    let r = provision(&s, &t);
    let other = s.publish_board_task(&input("other"), "other").unwrap();
    s.provision_board_task(id(&other), "reader", "1", "test")
        .unwrap();
    assert_eq!(
        s.claim_board_task(id(&r), id(&other)).unwrap_err().1,
        "board_task_outside_scope"
    );
}

#[test]
fn separate_sqlite_connections_race_for_one_task_atomically() {
    let path = std::env::temp_dir().join(format!("harness3-board-{}.sqlite", uuid::Uuid::new_v4()));
    let s = Store::open(&path).unwrap();
    s.publish(Preset {
        name: "reader".into(),
        version: "1".into(),
        purpose: "Read".into(),
        objective: "Find evidence".into(),
        instructions: "Read evidence".into(),
        protocol: Protocol::Demo,
        plugin: None,
    })
    .unwrap();
    let t = s.publish_board_task(&input("root"), "root").unwrap();
    let r = provision(&s, &t);
    s.claim_board_task(id(&r), id(&t)).unwrap();
    s.split_board_task(
        id(&r),
        &SplitInput {
            tasks: vec![input("A"), input("B")],
        },
        "split",
    )
    .unwrap();
    finish(&s, &r, "completed");
    let children = s.board_pending().unwrap();
    let a = provision(&s, &children[0]);
    let b = provision(&s, &children[1]);
    let other = Store::open(&path).unwrap();
    let gate = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut handles = vec![];
    for (store, run) in [(s.clone(), a), (other, b)] {
        let gate = gate.clone();
        let target = id(&children[0]).to_string();
        handles.push(std::thread::spawn(move || {
            gate.wait();
            store.claim_board_task(id(&run), &target)
        }));
    }
    let results = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter_map(|r| r.as_ref().err())
            .next()
            .unwrap()
            .1,
        "board_task_already_claimed"
    );
    s.cancel_board_task(id(&t)).unwrap();
    drop(s);
    std::fs::remove_file(&path).unwrap();
    let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
}
#[test]
fn cancelling_a_task_does_not_cancel_a_peer_working_on_a_different_claim() {
    let (s, t) = fixture();
    let r = provision(&s, &t);
    s.claim_board_task(id(&r), id(&t)).unwrap();
    s.split_board_task(
        id(&r),
        &SplitInput {
            tasks: vec![input("A"), input("B")],
        },
        "split",
    )
    .unwrap();
    finish(&s, &r, "completed");
    let children = s.board_pending().unwrap();
    let a = provision(&s, &children[0]);
    let b = provision(&s, &children[1]);
    s.claim_board_task(id(&a), id(&children[1])).unwrap();
    s.claim_board_task(id(&b), id(&children[0])).unwrap();
    s.cancel_board_task(id(&children[0])).unwrap();
    assert!(!s.cancel_requested(id(&a)).unwrap());
    assert!(s.cancel_requested(id(&b)).unwrap());
    assert_eq!(s.board_task(id(&children[1])).unwrap()["state"], "running");
    // Reconciliation then deliberately cancels the entire blocked root's remaining work.
    s.reconcile_board().unwrap();
    assert!(s.cancel_requested(id(&a)).unwrap());
}
#[test]
fn result_previews_have_a_lossless_paged_read_path() {
    let (s, t) = fixture();
    let r = provision(&s, &t);
    s.claim_board_task(id(&r), id(&t)).unwrap();
    s.split_board_task(
        id(&r),
        &SplitInput {
            tasks: vec![input("A")],
        },
        "split",
    )
    .unwrap();
    finish(&s, &r, "completed");
    let child = s.board_pending().unwrap().remove(0);
    let work = provision(&s, &child);
    s.claim_board_task(id(&work), id(&child)).unwrap();
    let report =
        json!({"output":"证据🦀".repeat(3000),"observations":[{"fact":"full observation"}]});
    s.finish(
        id(&work),
        work["instance_id"].as_str().unwrap(),
        "completed",
        report.clone(),
    )
    .unwrap();
    s.reconcile_board().unwrap();
    let resume = provision(&s, &t);
    let board = s.read_board(id(&resume)).unwrap();
    let preview = &board["tasks"][1]["result"];
    assert_eq!(preview["output_truncated"], true);
    assert_eq!(preview["observations_omitted"], 1);
    let mut offset = 0;
    let mut full = String::new();
    loop {
        let page = s
            .read_board_result(id(&resume), id(&child), offset, 97)
            .unwrap();
        full.push_str(page["content"].as_str().unwrap());
        match page["next_offset"].as_u64() {
            Some(next) => offset = next as usize,
            None => break,
        }
    }
    assert_eq!(serde_json::from_str::<Value>(&full).unwrap(), report);
}
#[test]
fn snapshot_keeps_active_roots_and_reports_omitted_history() {
    let (s, active) = fixture();
    for i in 0..100 {
        let t = s
            .publish_board_task(&input(&format!("archive {i}")), &format!("archive-{i}"))
            .unwrap();
        s.cancel_board_task(id(&t)).unwrap();
    }
    let snapshot = s.board_snapshot().unwrap();
    assert_eq!(snapshot["roots_total"], 101);
    assert_eq!(snapshot["roots_included"], 100);
    assert_eq!(snapshot["history_truncated"], true);
    assert!(
        snapshot["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["id"] == active["id"])
    );
}

#[tokio::test]
async fn worker_loop_completes_a_peer_board_split_and_continuation() {
    use harness3::CancellationToken;
    use harness3_platform::{models::Models, plugins::ProcessHost, routing::Router};
    use harness3_worker::worker_loop_with_router;

    let (s, t) = fixture();
    let stop = CancellationToken::new();
    let worker = tokio::spawn(worker_loop_with_router(
        s.clone(),
        Models::default(),
        Default::default(),
        stop.clone(),
        ProcessHost::disabled(s.clone()),
        Router::demo(),
    ));
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(25), async {
        loop {
            let root = s.board_task(id(&t)).unwrap();
            if ["completed", "failed", "blocked", "cancelled"]
                .contains(&root["state"].as_str().unwrap())
            {
                break root;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await;
    stop.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(5), worker)
        .await
        .unwrap()
        .unwrap();
    let root = outcome.expect("Demo board timed out");
    assert_eq!(root["state"], "completed", "{root}");
    let snapshot = s.board_snapshot().unwrap();
    assert_eq!(snapshot["tasks"].as_array().unwrap().len(), 3);
    assert!(
        snapshot["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|t| t["state"] == "completed")
    );
    assert_eq!(snapshot["agents"].as_array().unwrap().len(), 4);
    assert!(
        snapshot["agents"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["state"] == "terminated" && a["claimed_task_id"].is_string())
    );
    let mut events = vec![];
    let mut cursor = 0;
    loop {
        let page = s.events(cursor).unwrap();
        if page.is_empty() {
            break;
        }
        cursor = page.last().unwrap()["seq"].as_i64().unwrap();
        events.extend(page);
    }
    let sequence = |kind: &str, subject: &str| {
        events
            .iter()
            .find(|e| e["kind"] == kind && e["subject"] == subject)
            .unwrap()["seq"]
            .as_i64()
            .unwrap()
    };
    let root_id = id(&t);
    let route = events
        .iter()
        .find(|e| e["kind"] == "board.task_routed" && e["subject"] == root_id)
        .unwrap();
    let root_run = route["data"]["run_id"].as_str().unwrap();
    assert!(sequence("board.task_routed", root_id) < sequence("run.started", root_run));
    assert!(sequence("run.started", root_run) < sequence("board.task_claimed", root_id));
    assert!(sequence("board.task_claimed", root_id) < sequence("board.task_split", root_id));
    assert!(sequence("board.task_split", root_id) < sequence("board.children_activated", root_id));
    assert!(
        sequence("board.children_activated", root_id)
            < sequence("board.continuation_ready", root_id)
    );
    assert!(
        sequence("board.continuation_ready", root_id) < sequence("board.task_settled", root_id)
    );
}

#[test]
fn cancelled_staged_child_prevents_parent_success() {
    let (s, t) = fixture();
    let r = provision(&s, &t);
    s.claim_board_task(id(&r), id(&t)).unwrap();
    let split = s
        .split_board_task(
            id(&r),
            &SplitInput {
                tasks: vec![input("child")],
            },
            "split",
        )
        .unwrap();
    s.cancel_board_task(split["task_ids"][0].as_str().unwrap())
        .unwrap();
    finish(&s, &r, "completed");
    assert_eq!(s.board_task(id(&t)).unwrap()["state"], "blocked");
}

#[test]
fn a_slow_obsolete_invitation_cannot_fail_a_new_continuation() {
    let (s, root) = fixture();
    let r = provision(&s, &root);
    s.claim_board_task(id(&r), id(&root)).unwrap();
    s.split_board_task(
        id(&r),
        &SplitInput {
            tasks: vec![input("A"), input("B")],
        },
        "split",
    )
    .unwrap();
    finish(&s, &r, "completed");
    let children = s.board_pending().unwrap();
    let old_a = provision(&s, &children[0]);
    let peer_b = provision(&s, &children[1]);
    s.claim_board_task(id(&peer_b), id(&children[0])).unwrap();
    let split = s
        .split_board_task(
            id(&peer_b),
            &SplitInput {
                tasks: vec![input("A detail")],
            },
            "detail",
        )
        .unwrap();
    finish(&s, &peer_b, "completed");
    let detail = s
        .board_task(split["task_ids"][0].as_str().unwrap())
        .unwrap();
    let detail_run = provision(&s, &detail);
    s.claim_board_task(id(&detail_run), id(&detail)).unwrap();
    finish(&s, &detail_run, "completed");
    let continuation = provision(&s, &children[0]);
    finish(&s, &old_a, "completed");
    let task = s.board_task(id(&children[0])).unwrap();
    assert_eq!(task["state"], "pending");
    assert_eq!(task["route_state"], "provisioned");
    s.claim_board_task(id(&continuation), id(&children[0]))
        .unwrap();
    finish(&s, &continuation, "completed");
    assert_eq!(
        s.board_task(id(&children[0])).unwrap()["state"],
        "completed"
    );
    s.cancel_board_task(id(&root)).unwrap();
}
