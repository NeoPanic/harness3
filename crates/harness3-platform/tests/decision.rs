use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, Uri},
};
use harness3::{CancellationToken, LiveOutput, Model, ModelInput};
use harness3_platform::decision::DecisionProvider;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::sync::mpsc;

const KEY: &str = "test-key-must-not-appear-in-errors";
fn provider(base: &str) -> DecisionProvider {
    DecisionProvider::new(
        base.into(),
        "decision-model-preview".into(),
        KEY.into(),
        0.65,
    )
    .unwrap()
}
fn catalog() -> Vec<Value> {
    vec![
        json!({"preset_name":"reader","preset_version":"1","purpose":"Read sources","objective":"Produce cited evidence","capabilities":[{"name":"read","description":"Read local sources"}]}),
        json!({"preset_name":"coder","preset_version":"2","purpose":"Edit code","objective":"Implement and validate","capabilities":[{"name":"edit","description":"Edit workspace"}]}),
    ]
}
fn input() -> ModelInput {
    ModelInput{system:"route task".into(),text:json!({"task":{"title":"Read report","instructions":"Inspect supplied report","acceptance":"Cite evidence"},"presets":catalog()}).to_string(),turns:vec![],tools:vec![]}
}
fn answer() -> Value {
    json!({"model":"decision-model-preview","request_id":"req-test","answers":{"preset":{"type":"choice","choice":"preset_0","confidence":0.9,"probabilities":{"preset_0":0.8,"preset_1":0.15,"no_suitable_preset":0.05}}},"usage":{"input_tokens":125},"latency_ms":52.9})
}
#[test]
fn request_uses_typed_choice_with_capability_descriptions_and_fallback() {
    let p = provider("https://workspace.cn-beijing.maas.aliyuncs.com/compatible-mode/v1");
    let (request, presets) = p.request(&input()).unwrap();
    assert_eq!(presets, catalog());
    assert_eq!(request["model"], "decision-model-preview");
    assert_eq!(request["questions"]["preset"]["type"], "choice");
    let criteria = &request["questions"]["preset"]["criteria"];
    assert_eq!(criteria.as_object().unwrap().len(), 3);
    assert!(criteria["preset_0"].as_str().unwrap().contains("reader@1"));
    assert!(
        criteria["preset_0"]
            .as_str()
            .unwrap()
            .contains("Read local sources")
    );
    assert!(criteria["no_suitable_preset"].is_string());
    let state: Value = serde_json::from_str(request["state"].as_str().unwrap()).unwrap();
    assert_eq!(state["task"]["title"], "Read report");
    assert!(request.get("messages").is_none());
    assert!(!request.to_string().contains(KEY));
}
#[test]
fn decode_maps_choice_to_exact_preset_and_preserves_decision_evidence() {
    let reply = provider("https://example.test/compatible-mode/v1")
        .decode(answer(), &catalog())
        .unwrap();
    assert_eq!(reply.calls.len(), 1);
    assert_eq!(reply.calls[0].name, "select_preset");
    assert_eq!(reply.calls[0].arguments["preset_name"], "reader");
    assert_eq!(reply.calls[0].arguments["preset_version"], "1");
    assert_eq!(reply.continuation["request_id"], "req-test");
    assert_eq!(reply.continuation["confidence"], 0.9);
    assert_eq!(reply.continuation["probabilities"]["preset_1"], 0.15);
    assert_eq!(reply.continuation["input_tokens"], 125);
    assert_eq!(reply.continuation["options"][1]["preset_name"], "coder");
    let mut sparse = answer();
    sparse["answers"]["preset"]["probabilities"] = json!({"preset_0":1.0});
    assert!(
        provider("https://example.test/v1")
            .decode(sparse, &catalog())
            .is_ok()
    );
}
#[test]
fn unknown_and_noncanonical_choices_are_rejected() {
    let p = provider("https://example.test/v1");
    for choice in [
        "preset_99",
        "preset_-1",
        "preset_00",
        "preset_+0",
        "reader",
        "other",
    ] {
        let mut body = answer();
        body["answers"]["preset"]["choice"] = json!(choice);
        assert_eq!(
            p.decode(body, &catalog()).err().unwrap().code,
            "decision_selection_not_allowed",
            "{choice}"
        );
    }
}
#[test]
fn unsuitable_and_low_confidence_decisions_do_not_launch_an_agent() {
    let p = provider("https://example.test/v1");
    let mut body = answer();
    body["answers"]["preset"]["choice"] = json!("no_suitable_preset");
    assert_eq!(
        p.decode(body, &catalog()).err().unwrap().code,
        "board_no_suitable_preset"
    );
    let mut body = answer();
    body["answers"]["preset"]["confidence"] = json!(0.64);
    assert_eq!(
        p.decode(body, &catalog()).err().unwrap().code,
        "decision_low_confidence"
    );
    let mut body = answer();
    body["answers"]["preset"]["confidence"] = json!(0.65);
    assert!(p.decode(body, &catalog()).is_ok());
}
#[test]
fn invalid_confidence_and_probability_distributions_are_rejected() {
    let p = provider("https://example.test/v1");
    for confidence in [json!(-0.1), json!(1.1), json!(null), json!("high")] {
        let mut body = answer();
        body["answers"]["preset"]["confidence"] = confidence;
        assert_eq!(
            p.decode(body, &catalog()).err().unwrap().code,
            "decision_invalid_confidence"
        );
    }
    for probabilities in [
        json!({}),
        json!({"preset_1":1.0}),
        json!({"preset_0":-0.1,"preset_1":1.05,"no_suitable_preset":0.05}),
        json!({"preset_0":"0.8","preset_1":0.15,"no_suitable_preset":0.05}),
        json!({"preset_0":0.8,"invented":0.2}),
        json!({"preset_0":0.8,"preset_01":0.15,"no_suitable_preset":0.05}),
        json!({"preset_0":0.1,"preset_1":0.1,"no_suitable_preset":0.1}),
        json!({"preset_0":0.8,"preset_1":0.8,"no_suitable_preset":0.8}),
    ] {
        let mut body = answer();
        body["answers"]["preset"]["probabilities"] = probabilities.clone();
        assert_eq!(
            p.decode(body, &catalog()).err().unwrap().code,
            "decision_invalid_probabilities",
            "{probabilities}"
        );
    }
}
#[test]
fn configuration_rejects_credentials_in_urls_and_invalid_thresholds() {
    for base in [
        "file:///tmp/api",
        "https://user:password@example.test/v1",
        "https://example.test/v1?key=secret",
        "https://example.test/v1#secret",
    ] {
        assert!(
            DecisionProvider::new(
                base.into(),
                "decision-model-preview".into(),
                KEY.into(),
                0.65
            )
            .is_err()
        );
    }
    for threshold in [-0.1, 1.1, f64::NAN, f64::INFINITY] {
        assert!(
            DecisionProvider::new(
                "https://example.test/v1".into(),
                "decision-model-preview".into(),
                KEY.into(),
                threshold
            )
            .is_err()
        );
    }
    assert!(
        DecisionProvider::new(
            "https://example.test/v1".into(),
            "decision-model-preview".into(),
            String::new(),
            0.65
        )
        .is_err()
    );
}
#[test]
fn request_rejects_empty_or_oversized_catalogs() {
    let p = provider("https://example.test/v1");
    for count in [0, 255] {
        let mut i = input();
        i.text = json!({"task":{},"presets":vec![catalog()[0].clone();count]}).to_string();
        assert_eq!(
            p.request(&i).err().unwrap().code,
            "decision_invalid_catalog"
        );
    }
}
#[derive(Clone)]
struct Mock {
    status: StatusCode,
    body: Value,
    delay: Duration,
    seen: mpsc::UnboundedSender<(String, HeaderMap, Value)>,
}
async fn mock(
    State(s): State<Mock>,
    uri: Uri,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
    let _ = s.seen.send((uri.path().into(), headers, body));
    tokio::time::sleep(s.delay).await;
    (s.status, Json(s.body))
}
async fn server(
    status: StatusCode,
    body: Value,
    delay: Duration,
) -> (
    String,
    mpsc::UnboundedReceiver<(String, HeaderMap, Value)>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (send, receive) = mpsc::unbounded_channel();
    let app = Router::new().fallback(mock).with_state(Mock {
        status,
        body,
        delay,
        seen: send,
    });
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}/compatible-mode/v1"), receive, handle)
}
#[tokio::test]
async fn posts_to_systemone_with_bearer_auth_and_does_not_duplicate_endpoint() {
    for suffix in ["", "/systemone", "/systemone/"] {
        let (base, mut requests, server) = server(StatusCode::OK, answer(), Duration::ZERO).await;
        let p = provider(&format!("{base}{suffix}"));
        let reply = p
            .complete(input(), &LiveOutput::internal(), CancellationToken::new())
            .await
            .unwrap();
        let (path, headers, body) = requests.recv().await.unwrap();
        server.abort();
        assert_eq!(path, "/compatible-mode/v1/systemone");
        assert_eq!(headers["authorization"], format!("Bearer {KEY}"));
        assert!(
            headers["content-type"]
                .to_str()
                .unwrap()
                .starts_with("application/json")
        );
        assert_eq!(body["questions"]["preset"]["type"], "choice");
        assert_eq!(reply.calls[0].arguments["preset_name"], "reader");
        assert!(!p.metadata().to_string().contains(KEY));
    }
}
#[tokio::test]
async fn cancellation_interrupts_an_inflight_decision_request() {
    let (base, mut requests, server) =
        server(StatusCode::OK, answer(), Duration::from_secs(30)).await;
    let p = provider(&base);
    let stop = CancellationToken::new();
    let child = stop.clone();
    let request =
        tokio::spawn(async move { p.complete(input(), &LiveOutput::internal(), child).await });
    tokio::time::timeout(Duration::from_secs(2), requests.recv())
        .await
        .unwrap()
        .unwrap();
    stop.cancel();
    let error = tokio::time::timeout(Duration::from_secs(1), request)
        .await
        .unwrap()
        .unwrap()
        .err()
        .unwrap();
    server.abort();
    assert_eq!(error.code, "cancelled");
}
#[tokio::test]
async fn http_errors_do_not_expose_response_bodies_or_credentials() {
    let (base, mut requests, server) = server(
        StatusCode::UNAUTHORIZED,
        json!({"error":format!("rejected credential {KEY}")}),
        Duration::ZERO,
    )
    .await;
    let error = provider(&base)
        .complete(input(), &LiveOutput::internal(), CancellationToken::new())
        .await
        .err()
        .unwrap();
    assert!(requests.recv().await.is_some());
    server.abort();
    assert_eq!(error.code, "decision_http_401");
    assert!(!format!("{error:?}").contains(KEY));
}
#[tokio::test]
async fn oversized_responses_are_rejected_before_decoding() {
    let (base, _, server) = server(
        StatusCode::OK,
        json!({"untrusted":"x".repeat(512*1024)}),
        Duration::ZERO,
    )
    .await;
    let error = provider(&base)
        .complete(input(), &LiveOutput::internal(), CancellationToken::new())
        .await
        .err()
        .unwrap();
    server.abort();
    assert_eq!(error.code, "decision_response_too_large");
}
