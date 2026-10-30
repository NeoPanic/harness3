use harness3_platform::{
    domain::*,
    store::Store,
    workspaces::{PublishInput, WriteInput},
};

use serde_json::{Value, json};
use std::path::PathBuf;
struct Fixture {
    store: Store,
    root: PathBuf,
    goal: String,
    runs: Vec<Value>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn fixture() -> Fixture {
    fixture_with_verification(None)
}
fn fixture_with_verification(verification: Option<Value>) -> Fixture {
    let store = Store::memory().unwrap();
    let root = std::env::temp_dir().join(format!("h3-workspaces-{}", uuid::Uuid::new_v4()));
    store.set_workspace_root(&root).unwrap();
    store.publish_definition(json!({"name":"writer","version":"1","purpose":"files","objective":"share evidence","instructions":"write and publish files","protocol":"demo","plugins":[{"id":"platform.workspace","version":"1.0.0"}]})).unwrap();
    let instances = store
        .spawn(
            &Spawn {
                preset_name: "writer".into(),
                preset_version: "1".into(),
                count: 2,
            },
            "agents",
        )
        .unwrap();
    let plan:GoalPlan=serde_json::from_value(json!({"verification":verification,"title":"shared files","objective":"exchange file evidence","acceptance":"source traceable","tasks":[{"key":"a","instance_id":instances["instance_ids"][0],"instructions":"write evidence"},{"key":"b","instance_id":instances["instance_ids"][1],"instructions":"read peer evidence"}]})).unwrap();
    let goal = store.create_goal(&plan, "goal").unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    store.reconcile_goals().unwrap();
    let runs = vec![
        store.claim().unwrap().unwrap(),
        store.claim().unwrap().unwrap(),
    ];
    Fixture {
        store,
        root,
        goal,
        runs,
    }
}
fn rid(run: &Value) -> &str {
    run["id"].as_str().unwrap()
}
#[test]
fn one_agent_writes_and_another_reads_an_immutable_file_snapshot() {
    let f = fixture();
    let a = rid(&f.runs[0]);
    let b = rid(&f.runs[1]);
    let write = WriteInput {
        path: "research/evidence.md".into(),
        content: "证据甲🙂\nsource: supplied material".into(),
    };
    let saved = f.store.workspace_write(a, &write, "write").unwrap();
    assert_eq!(f.store.workspace_write(a, &write, "write").unwrap(), saved);
    let listing = f.store.run_workspace_files(a, "research").unwrap();
    assert_eq!(listing["entries"][0]["path"], write.path);
    let publication = PublishInput {
        name: "research evidence".into(),
        path: Some(write.path.clone()),
        value: None,
    };
    let artifact = f
        .store
        .publish_workspace_artifact(a, &publication, "publish")
        .unwrap();
    assert_eq!(artifact["goal_id"], f.goal);
    assert_eq!(artifact["source_state"], "running");
    assert_eq!(
        f.store
            .read_workspace_artifact(b, artifact["id"].as_str().unwrap())
            .unwrap()["content"],
        write.content
    );
    assert!(f.store.run_workspace_read(b, &write.path).is_err());
    f.store
        .workspace_write(
            a,
            &WriteInput {
                path: write.path.clone(),
                content: "revised live file".into(),
            },
            "edit",
        )
        .unwrap();
    assert_eq!(
        f.store
            .publish_workspace_artifact(a, &publication, "publish")
            .unwrap()["id"],
        artifact["id"]
    );
    assert_eq!(
        f.store
            .workspace_artifact(artifact["id"].as_str().unwrap())
            .unwrap()["content"],
        write.content
    );
    f.store
        .finish(
            a,
            f.runs[0]["instance_id"].as_str().unwrap(),
            "completed",
            json!({"output":"published evidence"}),
        )
        .unwrap();
    assert_eq!(
        f.store
            .read_workspace_artifact(b, artifact["id"].as_str().unwrap())
            .unwrap()["source_state"],
        "completed"
    );
    assert_eq!(
        f.store.workspace_artifacts(Some(&f.goal)).unwrap()["artifacts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn structured_json_publication_keeps_value_and_rejects_mutating_the_receipt() {
    let f = fixture();
    let a = rid(&f.runs[0]);
    let input = PublishInput {
        name: "structured findings".into(),
        path: None,
        value: Some(json!({"answer":42,"uncertainties":["source missing"]})),
    };
    let p = f
        .store
        .publish_workspace_artifact(a, &input, "json")
        .unwrap();
    let read = f
        .store
        .read_workspace_artifact(rid(&f.runs[1]), p["id"].as_str().unwrap())
        .unwrap();
    assert_eq!(read["value"], input.value.unwrap());
    assert_eq!(read["kind"], "json");
    assert_eq!(read["integrity_verified"], true);
    let changed = PublishInput {
        name: "different".into(),
        path: None,
        value: Some(json!({"answer":0})),
    };
    assert_eq!(
        f.store
            .publish_workspace_artifact(a, &changed, "json")
            .unwrap_err()
            .1,
        "idempotency_conflict"
    );
}
#[test]
fn file_paths_stay_relative_and_cancelled_runs_cannot_write() {
    let f = fixture();
    let a = rid(&f.runs[0]);
    for path in [
        "../escape.txt",
        "/tmp/escape.txt",
        "nested/../../escape.txt",
    ] {
        assert_eq!(
            f.store
                .workspace_write(
                    a,
                    &WriteInput {
                        path: path.into(),
                        content: "bad".into()
                    },
                    path
                )
                .unwrap_err()
                .1,
            "invalid_workspace_path"
        );
    }
    f.store.cancel_goal(&f.goal).unwrap();
    assert_eq!(
        f.store
            .workspace_write(
                a,
                &WriteInput {
                    path: "late.txt".into(),
                    content: "late".into()
                },
                "late"
            )
            .unwrap_err()
            .1,
        "workspace_run_not_authorized"
    );
}

#[test]
fn shared_file_and_json_snapshots_are_frozen_read_and_cited_by_independent_verification() {
    use harness3_platform::verification::{Finding, Verdict};
    let f = fixture_with_verification(Some(json!({
        "preset_name":"writer","preset_version":"1",
        "checks":[{"key":"evidence","description":"the shared file and JSON support the final contributions"}],
        "max_auto_reworks":0
    })));
    let source = rid(&f.runs[0]);
    let peer = rid(&f.runs[1]);
    f.store
        .workspace_write(
            source,
            &WriteInput {
                path: "research/shared.md".into(),
                content: "材料证据🙂：事实与不确定性已分开。".into(),
            },
            "file",
        )
        .unwrap();
    let file = f
        .store
        .publish_workspace_artifact(
            source,
            &PublishInput {
                name: "shared file evidence".into(),
                path: Some("research/shared.md".into()),
                value: None,
            },
            "file-publish",
        )
        .unwrap();
    let structured=f.store.publish_workspace_artifact(source,&PublishInput{name:"structured finding".into(),path:None,value:Some(json!({"fact":"given material","uncertainty":"external verification unavailable"}))},"json-publish").unwrap();
    let file_id = file["id"].as_str().unwrap();
    let json_id = structured["id"].as_str().unwrap();
    assert_eq!(
        f.store.read_workspace_artifact(peer, file_id).unwrap()["source_state"],
        "running"
    );
    assert_eq!(
        f.store.read_workspace_artifact(peer, json_id).unwrap()["value"]["fact"],
        "given material"
    );
    for run in &f.runs {
        f.store.finish(rid(run),run["instance_id"].as_str().unwrap(),"completed",json!({"output":format!("Final contribution refers to file {file_id} and JSON {json_id}")})).unwrap();
    }
    for _ in 0..3 {
        f.store.reconcile_fleets().unwrap();
        f.store.reconcile_goals().unwrap();
    }
    let reviewer = f.store.claim().unwrap().unwrap();
    let review_run = rid(&reviewer);
    assert!(
        f.runs
            .iter()
            .all(|r| r["instance_id"] != reviewer["instance_id"])
    );
    let job = f.store.verification_job(review_run).unwrap().unwrap();
    let manifest = job["manifest"].as_array().unwrap();
    assert_eq!(manifest.len(), 4);
    for id in [file_id, json_id] {
        let meta = manifest.iter().find(|m| m["id"] == id).unwrap();
        assert_eq!(meta["workspace"], true);
        assert_eq!(meta["run_id"], source);
        assert_eq!(meta["source_state"], "completed");
        assert!(meta["task_key"].is_string());
    }
    f.store
        .verification_evidence(review_run, file_id, 0, 2)
        .unwrap();
    f.store
        .verification_evidence(review_run, json_id, 0, 12000)
        .unwrap();
    let report = Verdict {
        summary: "Both published snapshots support the final evidence".into(),
        checks: vec![Finding {
            key: "evidence".into(),
            status: "passed".into(),
            reason: "Read the file evidence and structured finding".into(),
            artifact_ids: vec![file_id.into(), json_id.into()],
        }],
    };
    assert_eq!(
        f.store
            .submit_verification(review_run, &report)
            .unwrap_err()
            .1,
        "verification_evidence_incomplete"
    );
    let evidence = f
        .store
        .verification_evidence(review_run, file_id, 2, 12000)
        .unwrap();
    assert_eq!(evidence["complete"], true);
    f.store.submit_verification(review_run, &report).unwrap();
    f.store
        .finish(
            review_run,
            reviewer["instance_id"].as_str().unwrap(),
            "completed",
            json!({"output":"Independent file evidence review completed"}),
        )
        .unwrap();
    f.store.reconcile_goals().unwrap();
    let goal = f.store.goal(&f.goal).unwrap();
    assert_eq!(goal["verification"]["current"]["state"], "passed");
    assert_eq!(
        f.store
            .accept_goal(&f.goal, "Checked the file evidence and source identity")
            .unwrap()["state"],
        "accepted"
    );
}

#[tokio::test]
async fn actual_demo_team_verifier_paginates_large_workspace_evidence_before_passing() {
    use harness3::CancellationToken;
    use harness3_platform::{models::Models, research::ResearchInput};
    use harness3_worker as runtime;
    use std::{collections::HashMap, sync::Arc, time::Duration};
    use tokio::sync::Mutex;
    let f = Fixture {
        store: Store::memory().unwrap(),
        root: std::env::temp_dir().join(format!("h3-workspace-worker-{}", uuid::Uuid::new_v4())),
        goal: String::new(),
        runs: vec![],
    };

    let s = &f.store;
    s.set_workspace_root(&f.root).unwrap();
    s.publish_definition(json!({
        "name":"demo-team","version":"1.0.0","purpose":"deterministic file collaboration",
        "objective":"exercise real team tools","instructions":"Use tools to share evidence; this is a deterministic demonstration.","protocol":"demo",
        "plugins":[{"id":"platform.context","version":"1.0.0"},{"id":"platform.channels","version":"1.0.0"},{"id":"platform.workspace","version":"1.0.0"},{"id":"platform.tasks","version":"1.0.0"}]
    })).unwrap();
    s.publish_definition(json!({
        "name":"demo-verifier","version":"1.0.0","purpose":"deterministic evidence reader",
        "objective":"read every supplied artifact","instructions":"Read frozen evidence before reporting. Demo does not judge semantic quality.","protocol":"demo","plugins":[]
    })).unwrap();
    let input:ResearchInput=serde_json::from_value(json!({
        "title":"large shared evidence pagination",
        "objective":format!("Inspect the following supplied fixture material using the deterministic team tools. {}","A".repeat(6500)),
        "acceptance":"Files, structured publications and final contributions are source-traceable; all cited evidence must be fully read.",
        "preset_name":"demo-team","preset_version":"1.0.0","researchers":2,
        "verification":{"preset_name":"demo-verifier","preset_version":"1.0.0","checks":[{"key":"read-evidence","description":"Read and cite all frozen contributions and file/JSON publications."}],"max_auto_reworks":0}
    })).unwrap();
    let receipt = s.create_research(&input, "large-file-review").unwrap();
    let research = receipt["id"].as_str().unwrap();
    let stop = CancellationToken::new();
    let worker = tokio::spawn(runtime::worker_loop(
        s.clone(),
        Models::default(),
        Arc::new(Mutex::new(HashMap::new())),
        stop.clone(),
    ));
    let result = tokio::time::timeout(Duration::from_secs(45), async {
        loop {
            let job = s.research(research).unwrap();
            if ["awaiting-review", "failed", "cancelled"].contains(&job["state"].as_str().unwrap())
            {
                break job;
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
    })
    .await;
    stop.cancel();
    worker.await.unwrap();
    let job = result.expect("the actual Demo team must settle within the fixture deadline");
    assert_eq!(job["state"], "awaiting-review", "{job}");
    let goal = &job["goal"];
    let review = &goal["verification"]["current"];
    assert_eq!(review["state"], "passed", "{review}");
    let manifest = review["manifest"].as_array().unwrap();
    assert_eq!(manifest.len(), 9);
    assert_eq!(
        review["verdict"]["checks"][0]["artifact_ids"]
            .as_array()
            .unwrap()
            .len(),
        9
    );
    assert_eq!(
        manifest.iter().filter(|m| m["workspace"] == true).count(),
        6
    );
    assert_eq!(manifest.iter().filter(|m| m["kind"] == "file").count(), 3);
    assert_eq!(manifest.iter().filter(|m| m["kind"] == "json").count(), 3);
    let large_files = manifest
        .iter()
        .filter(|m| m["workspace"] == true && m["kind"] == "file")
        .filter_map(|m| {
            let snapshot = s.workspace_artifact(m["id"].as_str().unwrap()).unwrap();
            (snapshot["content"].as_str().unwrap().chars().count() > 12000)
                .then_some(m["id"].as_str().unwrap())
        })
        .collect::<Vec<_>>();
    assert!(
        !large_files.is_empty(),
        "fixture must actually exceed the 12,000-character evidence page limit"
    );
    let reviewer = s.run(review["run_id"].as_str().unwrap()).unwrap();
    assert_eq!(reviewer["state"], "completed");
    let observations = reviewer["report"]["observations"].as_array().unwrap();
    for id in &large_files {
        let pages = observations
            .iter()
            .filter(|o| {
                o["call"]["name"] == "platform.verification.evidence"
                    && o["call"]["arguments"]["artifact_id"] == *id
            })
            .collect::<Vec<_>>();
        assert!(
            pages.len() >= 2,
            "large workspace evidence needs multiple real tool calls"
        );
        assert!(pages.iter().any(|p| p["call"]["arguments"]["offset"] == 0
            && p["output"]["content"]["complete"] == false));
        assert!(pages.iter().any(|p| {
            p["call"]["arguments"]["offset"]
                .as_u64()
                .is_some_and(|n| n >= 12000)
                && p["output"]["content"]["complete"] == true
        }));
    }
    for artifact in manifest {
        assert_eq!(
            s.run(artifact["run_id"].as_str().unwrap()).unwrap()["state"],
            "completed"
        );
    }
    assert!(
        observations
            .iter()
            .any(|o| o["call"]["name"] == "platform.verification.report")
    );
    assert_eq!(
        s.accept_goal_context(
            goal["id"].as_str().unwrap(),
            "Checked actual tool pagination and source provenance",
            goal["round"].as_i64().unwrap(),
            goal["shared_context"]["revision"].as_i64().unwrap()
        )
        .unwrap()["state"],
        "accepted"
    );
}
