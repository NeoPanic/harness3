use harness3_plugin_sdk::{protocol::Manifest, serve};
use serde_json::json;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest: Manifest = serde_json::from_reader(std::fs::File::open("manifest.json")?)?;
    serve(manifest, |name, args| {
        if name != "inspect" {
            return Err("unknown_tool".into());
        }
        let text = args["text"].as_str().ok_or("invalid_text")?;
        Ok(
            json!({"characters":text.chars().count(),"lines":text.lines().count(),"bytes":text.len()}),
        )
    })
}
