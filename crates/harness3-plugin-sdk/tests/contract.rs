use harness3_plugin_sdk::{protocol::Manifest, serve_io};
use serde_json::{Value, json};
fn manifest() -> Manifest {
    serde_json::from_str(include_str!(
        "../../../examples/plugins/text-inspector/manifest.json"
    ))
    .unwrap()
}
#[test]
fn sdk_exposes_exact_capabilities_and_validates_arguments_before_dispatch() {
    let requests = [
        json!({"jsonrpc":"2.0","id":"init","method":"initialize"}),
        json!({"jsonrpc":"2.0","method":"cancel","params":{"invocation_id":"old"}}),
        json!({"jsonrpc":"2.0","id":"bad","method":"tools/call","params":{"name":"inspect","arguments":{"text":42}}}),
        json!({"jsonrpc":"2.0","id":"call","method":"tools/call","params":{"name":"inspect","arguments":{"text":"Harness³"}}}),
        json!({"jsonrpc":"2.0","id":"stop","method":"shutdown"}),
    ];
    let input = requests
        .iter()
        .map(|r| format!("{r}\n"))
        .collect::<String>();
    let mut output = Vec::new();
    let mut calls = 0;
    serve_io(manifest(),input.as_bytes(),&mut output,|_,args| {calls+=1;let text=args["text"].as_str().unwrap();Ok(json!({"characters":text.chars().count(),"lines":text.lines().count(),"bytes":text.len()}))}).unwrap();
    let replies: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(replies.len(), 4);
    assert_eq!(calls, 1);
    assert_eq!(
        replies[0]["result"]["tools"],
        serde_json::to_value(manifest().tools).unwrap()
    );
    assert!(replies[1].get("error").is_some());
    assert_eq!(replies[2]["result"]["content"]["characters"], 8);
    assert_eq!(replies[3]["result"]["closed"], true);
}
#[test]
fn sdk_rejects_oversized_frames_and_invalid_outputs() {
    let oversized = vec![b' '; 128 * 1024 + 1];
    assert!(
        serve_io(
            manifest(),
            &oversized[..],
            Vec::new(),
            |_, _| unreachable!()
        )
        .is_err()
    );
    let input=b"{\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"inspect\",\"arguments\":{\"text\":\"x\"}}}\n";
    let mut output = Vec::new();
    serve_io(manifest(), &input[..], &mut output, |_, _| {
        Ok(json!({"characters":"wrong"}))
    })
    .unwrap();
    let response: Value = serde_json::from_slice(&output).unwrap();
    assert!(response.get("error").is_some());
}
