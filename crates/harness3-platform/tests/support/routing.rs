use super::*;
pub(super) fn select(catalog: &[Value]) -> PResult<Selection> {
    let preset = &catalog[0];
    Ok(Selection {
        preset_name: preset["preset_name"].as_str().unwrap().into(),
        preset_version: preset["preset_version"].as_str().unwrap().into(),
        reason: "Offline regression fixture; no semantic routing.".into(),
        evidence: Value::Null,
    })
}
