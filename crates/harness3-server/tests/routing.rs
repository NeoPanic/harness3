use harness3::{CancellationToken, LiveOutput, Model, ModelInput, ModelReply};
use harness3_platform::{
    board::PublishInput,
    domain::*,
    models::{Models, Provider},
    routing::Router,
    store::Store,
};

use harness3_server::api::{App, router};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tower::ServiceExt;

struct Classifier {
    calls: Arc<AtomicUsize>,
    invalid: bool,
}
struct ContextClassifier;
#[async_trait::async_trait]
impl Model for ContextClassifier {
    async fn complete(
        &self,
        input: ModelInput,
        _: &LiveOutput,
        _: CancellationToken,
    ) -> harness3::Result<ModelReply> {
        let body: Value = serde_json::from_str(&input.text).unwrap();
        assert_eq!(
            body["task"]["shared_context"][0]["instructions"],
            "Original source material lives here"
        );
        Ok(ModelReply{text:json!({"preset_name":"writer","preset_version":"1","reason":"shared source material is available"}).to_string(),calls:vec![],usage:None,continuation:Value::Null,reasoning:None})
    }
}
#[async_trait::async_trait]
impl Model for Classifier {
    async fn complete(
        &self,
        input: ModelInput,
        _: &LiveOutput,
        _: CancellationToken,
    ) -> harness3::Result<ModelReply> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let data: Value = serde_json::from_str(&input.text).unwrap();
        let name = if self.invalid {
            "invented"
        } else if data["task"]["title"] == "review" {
            "reviewer"
        } else {
            "writer"
        };
        Ok(ModelReply {
            text:
                json!({"preset_name":name,"preset_version":"1","reason":"matches task capability"})
                    .to_string(),
            calls: vec![],
            usage: None,
            continuation: Value::Null,
            reasoning: None,
        })
    }
}
fn input(title: &str) -> PublishInput {
    PublishInput {
        title: title.into(),
        instructions: "Analyze supplied text".into(),
        acceptance: "Show evidence".into(),
    }
}
fn setup() -> (Store, Models) {
    let store = Store::memory().unwrap();
    for name in ["writer", "reviewer"] {
        store
            .publish(Preset {
                name: name.into(),
                version: "1".into(),
                purpose: name.into(),
                objective: "Analyze supplied evidence".into(),
                instructions: "Work on the claimed task".into(),
                protocol: Protocol::OpenaiCompatible,
                plugin: None,
            })
            .unwrap();
    }
    let models = Models {
        compatible: Some(Arc::new(
            Provider::new(
                Protocol::OpenaiCompatible,
                "test-worker".into(),
                "http://127.0.0.1:1/v1".into(),
                String::new(),
            )
            .unwrap(),
        )),
        ..Default::default()
    };
    (store, models)
}
#[test]
fn explicit_demo_mode_uses_only_local_routing() {
    let (_, mut models) = setup();
    models.demo_only = true;
    assert_eq!(
        Router::from_env(&models).unwrap().describe()["mode"],
        "deterministic-demo"
    );
}
#[test]
fn classifier_catalog_includes_injected_board_capabilities_and_preset_constraints() {
    let (store, models) = setup();
    let router = Router::with_model(Arc::new(Classifier {
        calls: Default::default(),
        invalid: false,
    }));
    for preset in router.catalog(&store, &models, false).unwrap() {
        assert_eq!(preset["instructions"], "Work on the claimed task");
        let names = preset["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert!(names.contains(&"platform.board.claim"));
        assert!(names.contains(&"platform.board.split"));
        assert!(names.contains(&"platform.board.result"));
        assert!(names.contains(&"text.inspect"));
    }
}
#[tokio::test]
async fn classifier_selects_different_presets_but_does_not_claim() {
    let (store, models) = setup();
    let a = store.publish_board_task(&input("write"), "write").unwrap();
    let b = store
        .publish_board_task(&input("review"), "review")
        .unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let classifier = Router::with_model(Arc::new(Classifier {
        calls: calls.clone(),
        invalid: false,
    }));
    let cancel = CancellationToken::new();
    let (one, two) = tokio::join!(
        classifier.route_pending(&store, &models, false, cancel.clone()),
        classifier.route_pending(&store, &models, false, cancel)
    );
    one.unwrap();
    two.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let a = store.board_task(a["id"].as_str().unwrap()).unwrap();
    let b = store.board_task(b["id"].as_str().unwrap()).unwrap();
    assert_eq!(a["preset_name"], "writer");
    assert_eq!(b["preset_name"], "reviewer");
    assert_eq!(a["state"], "pending");
    assert!(a["instance_id"].is_null());
    assert_eq!(store.list("instances").unwrap().len(), 2);
}
#[tokio::test]
async fn child_routing_sees_the_same_ancestor_material_as_workers() {
    let (store, models) = setup();
    let mut root_input = input("root");
    root_input.instructions = "Original source material lives here".into();
    let root = store.publish_board_task(&root_input, "root").unwrap();
    let root_id = root["id"].as_str().unwrap();
    store
        .provision_board_task(root_id, "writer", "1", "initial work")
        .unwrap();
    let run = store.claim().unwrap().unwrap();
    let run_id = run["id"].as_str().unwrap();
    store.claim_board_task(run_id, root_id).unwrap();
    store
        .split_board_task(
            run_id,
            &harness3_platform::board::SplitInput {
                tasks: vec![input("analyze the source above")],
            },
            "split",
        )
        .unwrap();
    store
        .finish(
            run_id,
            run["instance_id"].as_str().unwrap(),
            "completed",
            json!({"output":"split"}),
        )
        .unwrap();
    store.reconcile_board().unwrap();
    Router::with_model(Arc::new(ContextClassifier))
        .route_pending(&store, &models, false, CancellationToken::new())
        .await
        .unwrap();
    assert!(store.board_pending().unwrap().is_empty());
    assert_eq!(
        store.board_snapshot().unwrap()["agents"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
#[tokio::test]
async fn invalid_decision_keeps_work_pending_and_backs_off() {
    let (store, models) = setup();
    let task = store.publish_board_task(&input("write"), "write").unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let classifier = Router::with_model(Arc::new(Classifier {
        calls: calls.clone(),
        invalid: true,
    }));
    for _ in 0..2 {
        classifier
            .route_pending(&store, &models, false, CancellationToken::new())
            .await
            .unwrap();
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(store.list("instances").unwrap().is_empty());
    assert_eq!(
        store.board_task(task["id"].as_str().unwrap()).unwrap()["state"],
        "pending"
    );
    assert_eq!(
        store.board_routing_status().unwrap()[0]["error"],
        "routing_selection_not_allowed"
    );
}
#[tokio::test]
async fn unavailable_worker_model_is_never_selected() {
    let (store, _) = setup();
    store.publish_board_task(&input("write"), "write").unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let classifier = Router::with_model(Arc::new(Classifier {
        calls: calls.clone(),
        invalid: false,
    }));
    classifier
        .route_pending(&store, &Models::default(), false, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(store.list("instances").unwrap().is_empty());
    assert_eq!(
        store.board_routing_status().unwrap()[0]["error"],
        "board_no_eligible_preset"
    );
}
#[tokio::test]
async fn shutdown_and_cancelled_tasks_never_start_agents() {
    let (store, models) = setup();
    let task = store.publish_board_task(&input("write"), "write").unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let classifier = Router::with_model(Arc::new(Classifier {
        calls: calls.clone(),
        invalid: false,
    }));
    let cancel = CancellationToken::new();
    cancel.cancel();
    classifier
        .route_pending(&store, &models, false, cancel)
        .await
        .unwrap();
    store
        .cancel_board_task(task["id"].as_str().unwrap())
        .unwrap();
    classifier
        .route_pending(&store, &models, false, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(store.list("instances").unwrap().is_empty());
}
#[tokio::test]
async fn board_api_auth_admission_idempotency_and_cancel() {
    let store = Store::memory().unwrap();
    let app = router(App {
        store: store.clone(),
        token: "test-token".into(),
        cancellations: Default::default(),
        models: Models::default(),
        plugins: harness3_platform::plugins::ProcessHost::disabled(store.clone()),
    });
    let request = |auth: bool, body: Value| {
        let mut r = axum::http::Request::builder()
            .method("POST")
            .uri("/api/board/tasks")
            .header("Host", "localhost")
            .header("Content-Type", "application/json")
            .header("Idempotency-Key", "same");
        if auth {
            r = r.header("Authorization", "Bearer test-token");
        }
        r.body(axum::body::Body::from(body.to_string())).unwrap()
    };
    let body = serde_json::to_value(input("write")).unwrap();
    assert_eq!(
        app.clone()
            .oneshot(request(false, body.clone()))
            .await
            .unwrap()
            .status(),
        401
    );
    let response = app
        .clone()
        .oneshot(request(true, body.clone()))
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    let created: Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 100000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(created["instance_id"].is_null());
    assert!(created["preset_name"].is_null());
    assert_eq!(
        app.clone()
            .oneshot(request(true, body.clone()))
            .await
            .unwrap()
            .status(),
        201
    );
    let mut wrong = body.clone();
    wrong["instance_id"] = json!("agent-x");
    assert_eq!(
        app.clone()
            .oneshot(request(true, wrong))
            .await
            .unwrap()
            .status(),
        422
    );
    let mut changed = body;
    changed["title"] = json!("different");
    assert_eq!(
        app.clone()
            .oneshot(request(true, changed))
            .await
            .unwrap()
            .status(),
        409
    );
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/v1/board/tasks/{}/cancel",
                    created["id"].as_str().unwrap()
                ))
                .header("Host", "localhost")
                .header("Authorization", "Bearer test-token")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        store.board_task(created["id"].as_str().unwrap()).unwrap()["state"],
        "cancelled"
    );
}
