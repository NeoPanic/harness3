use harness3_platform::{
    domain::*,
    fleets::{FleetAction, FleetPlan},
    presets::{bindings, instantiate},
    store::Store,
};
use serde_json::{Value, json};
fn definition(name: &str) -> Value {
    json!({"name":name,"version":"1","purpose":"inspect","objective":"report","instructions":"use evidence","protocol":"demo"})
}
fn schema() -> Value {
    json!({"type":"object","properties":{"topic":{"type":"string","minLength":1,"maxLength":200},"count":{"type":"integer","minimum":1,"maximum":3},"flag":{"type":"boolean"}},"required":["topic"],"additionalProperties":false})
}
fn bundle(store: &Store, id: &str, version: &str) {
    store
        .install(Bundle {
            id: id.into(),
            version: version.into(),
            description: "guide".into(),
            instructions: format!("{id}/{version}"),
        })
        .unwrap();
}
#[test]
fn inheritance_materializes_lineage_ordered_capability_locks_and_immutable_source() {
    let store = Store::memory().unwrap();
    bundle(&store, "local.a", "1");
    bundle(&store, "local.b", "1");
    let mut parent = definition("base");
    parent["plugins"] = json!([{"id":"local.a","version":"1"},{"id":"local.b","version":"1"}]);
    let base = store.publish_definition(parent.clone()).unwrap();
    let draft = json!({"name":"child","version":"1","extends":{"name":"base","version":"1"},"objective":"specialize"});
    let child = store.publish_definition(draft.clone()).unwrap();
    assert_eq!(child["purpose"], base["purpose"]);
    assert_eq!(child["objective"], "specialize");
    assert_eq!(bindings(&child), bindings(&base));
    assert_eq!(child["lineage"], json!([{"name":"base","version":"1"}]));
    assert!(
        bindings(&child)
            .iter()
            .all(|p| p["digest"].as_str().unwrap().starts_with("sha256:"))
    );
    assert_eq!(store.publish_definition(draft.clone()).unwrap(), child);
    let mut conflict = draft;
    conflict["objective"] = "changed".into();
    assert_eq!(
        store.publish_definition(conflict).unwrap_err().1,
        "preset_version_immutable"
    );
    parent["version"] = "2".into();
    parent["instructions"] = "changed base".into();
    store.publish_definition(parent).unwrap();
    assert_eq!(child["instructions"], "use evidence");
    let cleared=store.publish_definition(json!({"name":"cleared","version":"1","extends":{"name":"child","version":"1"},"plugins":[]})).unwrap();
    assert!(bindings(&cleared).is_empty());
    assert_eq!(cleared["lineage"].as_array().unwrap().len(), 2);
}
#[test]
fn publication_rejects_missing_parents_self_reference_depth_and_duplicate_versions() {
    let store = Store::memory().unwrap();
    assert_eq!(
        store
            .publish_definition(
                json!({"name":"a","version":"1","extends":{"name":"missing","version":"1"}})
            )
            .unwrap_err()
            .1,
        "parent_preset_not_found"
    );
    assert_eq!(
        store
            .publish_definition(
                json!({"name":"a","version":"1","extends":{"name":"a","version":"1"}})
            )
            .unwrap_err()
            .1,
        "invalid_preset_parent"
    );
    store.publish_definition(definition("root")).unwrap();
    let mut previous = "root".to_owned();
    for i in 1..=16 {
        let name = format!("child-{i}");
        store
            .publish_definition(
                json!({"name":name,"version":"1","extends":{"name":previous,"version":"1"}}),
            )
            .unwrap();
        previous = name;
    }
    assert_eq!(
        store
            .publish_definition(
                json!({"name":"overflow","version":"1","extends":{"name":previous,"version":"1"}})
            )
            .unwrap_err()
            .1,
        "preset_inheritance_depth_or_cycle"
    );
    bundle(&store, "local.a", "1");
    bundle(&store, "local.a", "2");
    let mut p = definition("duplicates");
    p["plugins"] = json!([{"id":"local.a","version":"1"},{"id":"local.a","version":"2"}]);
    assert_eq!(
        store.publish_definition(p).unwrap_err().1,
        "duplicate_plugin_binding"
    );
    let mut p = definition("ambiguous");
    p["plugin"] = Value::Null;
    p["plugins"] = json!([]);
    assert_eq!(
        store.publish_definition(p).unwrap_err().1,
        "ambiguous_plugin_bindings"
    );
}
#[test]
fn parameter_defaults_validation_and_literal_single_pass_expansion() {
    let store = Store::memory().unwrap();
    let mut p = definition("template");
    p["objective"] = "Study {{topic}} with {{count}} reviewers".into();
    p["parameters_schema"] = schema();
    p["parameter_defaults"] = json!({"count":2,"flag":false});
    let snapshot = store.publish_definition(p).unwrap();
    assert_eq!(
        instantiate(snapshot.clone(), &serde_json::Map::new())
            .unwrap_err()
            .1,
        "invalid_instance_parameters"
    );
    for bad in [
        json!({"topic":""}),
        json!({"topic":"x","count":4}),
        json!({"topic":"x","extra":1}),
        json!({"topic":"x","flag":"false"}),
    ] {
        assert!(instantiate(snapshot.clone(), bad.as_object().unwrap()).is_err());
    }
    let instance = instantiate(
        snapshot.clone(),
        json!({"topic":"{{count}} 中文 🧪","count":3})
            .as_object()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        instance["objective"],
        "Study {{count}} 中文 🧪 with 3 reviewers"
    );
    assert_eq!(instance["parameters"]["flag"], false);
    assert_eq!(
        snapshot["objective"],
        "Study {{topic}} with {{count}} reviewers"
    );
    let child=store.publish_definition(json!({"name":"derived","version":"1","extends":{"name":"template","version":"1"},"parameter_defaults":{"topic":"local","count":1}})).unwrap();
    assert_eq!(
        instantiate(child, &serde_json::Map::new()).unwrap()["objective"],
        "Study local with 1 reviewers"
    );
}
#[test]
fn invalid_schema_defaults_templates_and_model_configuration_do_not_publish() {
    let store = Store::memory().unwrap();
    for mutation in [
        json!({"parameters_schema":{"type":"object","properties":{},"additionalProperties":true}}),
        json!({"parameters_schema":{"type":"string"}}),
        json!({"parameters_schema":schema(),"parameter_defaults":{"count":4}}),
        json!({"parameters_schema":schema(),"parameter_defaults":{"secret":"x"}}),
        json!({"parameters_schema":schema(),"objective":"{{unknown}}"}),
        json!({"parameters_schema":schema(),"objective":"{{flag}}"}),
        json!({"instructions":"{{topic"}),
        json!({"unknown":true}),
        json!({"parameters_schema":null}),
    ] {
        let mut p = definition("invalid");
        p.as_object_mut()
            .unwrap()
            .extend(mutation.as_object().unwrap().clone());
        assert!(store.publish_definition(p).is_err());
    }
    assert!(
        store
            .publish_checked(definition("blocked"), |_| Err(fault(
                400,
                "profile_unavailable"
            )))
            .is_err()
    );
    assert!(store.list("presets").unwrap().is_empty());
    let mut p = definition("too-large");
    p["instructions"] = "{{topic}}".into();
    p["parameters_schema"] = json!({"type":"object","properties":{"topic":{"type":"string"}},"required":["topic"],"additionalProperties":false});
    let snapshot = store.publish_definition(p).unwrap();
    assert_eq!(
        instantiate(
            snapshot,
            json!({"topic":"x".repeat(17000)}).as_object().unwrap()
        )
        .unwrap_err()
        .1,
        "parameters_too_large"
    );
}
#[test]
fn fleet_parameters_are_atomic_idempotent_and_retained_across_scale_and_replacement() {
    let store = Store::memory().unwrap();
    let mut p = definition("worker");
    p["parameters_schema"] = schema();
    p["objective"] = "Study {{topic}}".into();
    store.publish_definition(p).unwrap();
    let mut plan = FleetPlan {
        preset_name: "worker".into(),
        preset_version: "1".into(),
        count: 1,
        mode: "maintain".into(),
        ttl_seconds: None,
        parameters: Default::default(),
    };
    assert!(store.create_fleet(&plan, "same").is_err());
    assert_eq!(store.capacity().unwrap()["reserved"], 0);
    assert!(store.fleets().unwrap().is_empty());
    plan.parameters = json!({"topic":"original"}).as_object().unwrap().clone();
    let accepted = store.create_fleet(&plan, "same").unwrap();
    assert_eq!(accepted, store.create_fleet(&plan, "same").unwrap());
    plan.parameters.insert("topic".into(), "different".into());
    assert_eq!(
        store.create_fleet(&plan, "same").unwrap_err().1,
        "idempotency_conflict"
    );
    store.reconcile_fleets().unwrap();
    let id = accepted["fleet_id"].as_str().unwrap();
    let first = store.fleet(id).unwrap()["slots"][0]["instance_id"]
        .as_str()
        .unwrap()
        .to_owned();
    store.transition(&first, "terminate").unwrap();
    store.reconcile_fleets().unwrap();
    store
        .fleet_action(
            id,
            &FleetAction {
                action: "scale".into(),
                expected_revision: 1,
                count: Some(2),
            },
            "scale",
        )
        .unwrap();
    store.reconcile_fleets().unwrap();
    let instances = store.list("instances").unwrap();
    assert_eq!(instances.len(), 3);
    assert!(
        instances
            .iter()
            .all(|i| i["snapshot"]["objective"] == "Study original"
                && i["snapshot"]["parameters"]["topic"] == "original")
    );
}
#[test]
fn legacy_publication_and_instantiation_keep_their_existing_wire_contract() {
    let store = Store::memory().unwrap();
    let p = Preset {
        name: "legacy".into(),
        version: "1".into(),
        purpose: "old".into(),
        objective: "report".into(),
        instructions: "inspect".into(),
        protocol: Protocol::Demo,
        plugin: None,
    };
    let old = serde_json::to_string(&p).unwrap();
    store
        .transaction(|tx| {
            tx.execute(
                "INSERT INTO presets(name,version,body,created_at) VALUES('legacy','1',?,0)",
                [old],
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(store.publish(p).unwrap()["name"], "legacy");
    let spawned = store
        .spawn(
            &Spawn {
                preset_name: "legacy".into(),
                preset_version: "1".into(),
                count: 1,
            },
            "legacy",
        )
        .unwrap();
    assert_eq!(spawned["count"], 1);
}
