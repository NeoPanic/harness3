#!/usr/bin/python3
"""Reference Harness³ stdio plugin. No network or third-party Python packages."""
import json
import sys

with open("manifest.json", encoding="utf-8") as source:
    manifest = json.load(source)
for line in sys.stdin:
    request = json.loads(line)
    if "id" not in request:
        continue
    method = request["method"]
    if method == "initialize":
        result = {"protocol": "harness3-plugin-rpc/v1", "id": manifest["id"],
                  "version": manifest["version"], "tools": manifest["tools"]}
    elif method == "health":
        result = {"healthy": True}
    elif method == "shutdown":
        result = {"closed": True}
    elif method == "tools/call":
        params = request["params"]
        text = params["arguments"].get("text")
        if params["name"] != "inspect" or not isinstance(text, str):
            print(json.dumps({"jsonrpc": "2.0", "id": request["id"], "error": {"code": "invalid_arguments"}}), flush=True)
            continue
        # Python and Rust count Unicode scalar values identically for valid UTF-8.
        result = {"content": {"characters": len(text), "lines": text.count('\n') + int(bool(text) and not text.endswith('\n')),
                              "bytes": len(text.encode("utf-8"))}, "changed": False}
    else:
        print(json.dumps({"jsonrpc": "2.0", "id": request["id"], "error": {"code": "unknown_method"}}), flush=True)
        continue
    print(json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": result}), flush=True)
    if method == "shutdown":
        break
