//! Minimal synchronous plugin authoring SDK. Long-running tools need their own
//! cooperative cancellation implementation; the host enforces a hard deadline.
pub use harness3_plugin_protocol as protocol;
use protocol::{MAX_FRAME, Manifest, PROTOCOL, validate_value};
use serde_json::{Value, json};
use std::io::{BufRead, Write};

pub fn serve(
    manifest: Manifest,
    call: impl FnMut(&str, Value) -> Result<Value, String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    serve_io(manifest, stdin.lock(), stdout.lock(), call)
}
/// Run the same adapter against custom streams for author contract tests.
pub fn serve_io(
    manifest: Manifest,
    mut input: impl BufRead,
    mut output: impl Write,
    mut call: impl FnMut(&str, Value) -> Result<Value, String>,
) -> Result<(), Box<dyn std::error::Error>> {
    manifest.validate().map_err(std::io::Error::other)?;
    loop {
        let mut bytes = Vec::new();
        if std::io::Read::take(&mut input, MAX_FRAME + 1).read_until(b'\n', &mut bytes)? == 0 {
            return Ok(());
        }
        if bytes.len() as u64 > MAX_FRAME {
            return Err("frame_too_large".into());
        }
        let request: Value = serde_json::from_slice(&bytes)?;
        if request.get("id").is_none() {
            continue;
        }
        let result: Result<Value, String> = match request["method"].as_str() {
            Some("initialize") => Ok(
                json!({"protocol":PROTOCOL,"id":manifest.id,"version":manifest.version,"tools":manifest.tools}),
            ),
            Some("health") => Ok(json!({"healthy":true})),
            Some("shutdown") => Ok(json!({"closed":true})),
            Some("tools/call") => {
                let params = &request["params"];
                if let Some(tool) = manifest.tools.iter().find(|t| t.name == params["name"]) {
                    validate_value(&tool.input_schema, &params["arguments"])
                        .and_then(|()| call(&tool.name, params["arguments"].clone()))
                        .and_then(|value| {
                            validate_value(&tool.output_schema, &value)?;
                            Ok(json!({"content":value,"changed":tool.mutates}))
                        })
                } else {
                    Err("unknown_tool".into())
                }
            }
            _ => Err("unknown_method".into()),
        };
        let response = match result {
            Ok(body) => json!({"jsonrpc":"2.0","id":request["id"],"result":body}),
            Err(code) => json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":code}}),
        };
        writeln!(output, "{response}")?;
        output.flush()?;
        if request["method"] == "shutdown" {
            return Ok(());
        }
    }
}
