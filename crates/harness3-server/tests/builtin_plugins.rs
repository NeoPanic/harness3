use harness3_platform::{organization, store::Store};
use serde_json::json;

#[test]
fn official_bindings_lock_real_tools_and_reject_tampered_snapshots() {
    let store = Store::memory().unwrap();
    let definition = store.publish_definition(json!({
        "name":"official", "version":"1", "purpose":"test", "objective":"inspect",
        "instructions":"Use the bound tools.", "protocol":"demo",
        "plugins":[{"id":"text","version":"1.0.0"},{"id":"web.fetch","version":"1.0.0"},{"id":"web.search","version":"1.0.0"}]
    })).unwrap();
    assert_eq!(definition["plugin_snapshots"].as_array().unwrap().len(), 3);
    for id in ["text", "web.fetch", "web.search"] {
        let mut package = organization::package(id).unwrap();
        let tools = organization::tools(&package, store.clone(), "test").unwrap();
        if id != "text" {
            assert_eq!(tools.len(), 1);
            assert_eq!(tools[0].spec().name, id);
        }
        package["instructions"] = "tampered".into();
        assert!(organization::tools(&package, store.clone(), "test").is_err());
    }
}
