use serde_json::Value;
pub(super) fn capabilities() -> Value {
    serde_json::from_str(include_str!("../../../../docs/implementation-status.json"))
        .expect("embedded implementation status must be valid JSON")
}
