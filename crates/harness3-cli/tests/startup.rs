use serde_json::Value;
use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};

struct Service {
    child: Child,
    root: PathBuf,
}
impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[tokio::test]
async fn fresh_start_is_empty_with_or_without_a_model() {
    for configured in [false, true] {
        let root = std::env::temp_dir().join(format!("harness3-empty-{}", uuid::Uuid::new_v4()));
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let mut command = Command::new(env!("CARGO_BIN_EXE_harness3"));
        command
            .args(["up", "--port", &port.to_string(), "--data-dir"])
            .arg(&root)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for (key, _) in std::env::vars().filter(|(key, _)| key.starts_with("HARNESS3_")) {
            command.env_remove(key);
        }
        if configured {
            command
                .env("HARNESS3_OPENAI_RESPONSES_MODEL", "test-model")
                .env(
                    "HARNESS3_OPENAI_RESPONSES_BASE_URL",
                    "http://127.0.0.1:1/v1",
                );
        }
        let mut service = Service {
            child: command.spawn().unwrap(),
            root,
        };
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(1))
            .build()
            .unwrap();
        let snapshot: Value = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                assert!(
                    service.child.try_wait().unwrap().is_none(),
                    "service exited before ready"
                );
                if let Ok(token) = std::fs::read_to_string(service.root.join("token"))
                    && let Ok(response) = client
                        .get(format!("http://127.0.0.1:{port}/api/v1/snapshot"))
                        .bearer_auth(token.trim())
                        .send()
                        .await
                    && response.status().is_success()
                {
                    break response.json().await.unwrap();
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("service readiness timeout");
        for collection in [
            "presets",
            "instances",
            "runs",
            "goals",
            "fleets",
            "plugins",
            "channels",
            "research_jobs",
        ] {
            assert!(
                snapshot[collection].as_array().unwrap().is_empty(),
                "{collection} was seeded"
            );
        }
        let models = snapshot["models"].as_array().unwrap();
        assert!(models.iter().all(|m| m["protocol"] != "demo"));
        assert_eq!(models.iter().any(|m| m["configured"] == true), configured);
    }
}

#[test]
fn demo_startup_flag_is_removed() {
    let output = Command::new(env!("CARGO_BIN_EXE_harness3"))
        .args(["up", "--demo"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unexpected argument"));
}
