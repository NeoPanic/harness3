use super::*;
use harness3_plugin_protocol::{MAX_FRAME, PROTOCOL};
use std::process::Stdio;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout},
};

type R<T> = Result<T, &'static str>;
struct Process {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    group: Option<u32>,
}
impl Drop for Process {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(pid) = self.group {
            // The child remains unreaped while this guard exists, so its process-group
            // ID cannot be reused by an unrelated process before cleanup.
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
        let _ = self.child.start_kill();
    }
}
impl Process {
    fn spawn(path: &Path, manifest: &Manifest) -> R<Self> {
        let mut command = tokio::process::Command::new(path.join(&manifest.runtime.entry));
        command
            .current_dir(path)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("LANG", "C.UTF-8")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(unix)]
        {
            command.process_group(0);
        }
        let mut child = command.spawn().map_err(|_| "plugin_spawn_failed")?;
        let group = child.id();
        let input = child.stdin.take().ok_or("plugin_pipe_failed")?;
        let output = BufReader::new(child.stdout.take().ok_or("plugin_pipe_failed")?);
        Ok(Self {
            child,
            input,
            output,
            group,
        })
    }
    async fn send(&mut self, request: Value) -> R<()> {
        let mut bytes = serde_json::to_vec(&request).map_err(|_| "plugin_request_invalid")?;
        if bytes.len() as u64 >= MAX_FRAME {
            return Err("plugin_request_too_large");
        }
        bytes.push(b'\n');
        self.input
            .write_all(&bytes)
            .await
            .map_err(|_| "plugin_pipe_closed")?;
        self.input.flush().await.map_err(|_| "plugin_pipe_closed")
    }
    async fn response(&mut self, id: &str, progress: Option<(&Store, &str)>) -> R<Value> {
        for _ in 0..65 {
            let mut bytes = Vec::new();
            let n = (&mut self.output)
                .take(MAX_FRAME + 1)
                .read_until(b'\n', &mut bytes)
                .await
                .map_err(|_| "plugin_pipe_closed")?;
            if n == 0 {
                return Err("plugin_disconnected");
            }
            if n as u64 > MAX_FRAME {
                return Err("plugin_response_too_large");
            }
            let response: Value =
                serde_json::from_slice(&bytes).map_err(|_| "plugin_protocol_error")?;
            if response["jsonrpc"] != "2.0" {
                return Err("plugin_protocol_error");
            }
            if response.get("id").is_none() && response["method"] == "progress" {
                let params = &response["params"];
                if params["invocation_id"] != id
                    || params["message"].as_str().is_none_or(|m| m.len() > 2000)
                {
                    return Err("plugin_protocol_error");
                }
                if let Some((store, subject)) = progress {
                    store
                        .record(
                            "plugin.progress",
                            subject,
                            json!({"invocation_id":id,"message":params["message"]}),
                        )
                        .map_err(|_| "plugin_audit_failed")?;
                }
                continue;
            }
            if response["id"] != id
                || response.get("result").is_some() == response.get("error").is_some()
            {
                return Err("plugin_protocol_error");
            }
            if response.get("error").is_some() {
                return Err("plugin_tool_error");
            }
            return Ok(response["result"].clone());
        }
        Err("plugin_progress_limit_exceeded")
    }
    async fn rpc(&mut self, id: &str, method: &str, params: Value) -> R<Value> {
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .await?;
        self.response(id, None).await
    }
    async fn handshake(&mut self, manifest: &Manifest) -> R<()> {
        let result = self
            .rpc("initialize", "initialize", json!({"protocol":PROTOCOL}))
            .await?;
        let tools: Vec<harness3_plugin_protocol::Tool> =
            serde_json::from_value(result["tools"].clone())
                .map_err(|_| "plugin_capabilities_mismatch")?;
        if result["protocol"] != PROTOCOL
            || result["id"] != manifest.id
            || result["version"] != manifest.version
            || tools != manifest.tools
        {
            return Err("plugin_capabilities_mismatch");
        }
        Ok(())
    }
    async fn close(mut self) {
        let _ = tokio::time::timeout(
            Duration::from_millis(250),
            self.rpc("shutdown", "shutdown", json!({})),
        )
        .await;
        #[cfg(unix)]
        if let Some(pid) = self.group.take() {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
    }
}
pub async fn health(path: &Path, manifest: &Manifest) -> R<()> {
    let mut process = Process::spawn(path, manifest)?;
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        process.handshake(manifest).await?;
        if process.rpc("health", "health", json!({})).await?["healthy"] != true {
            return Err("plugin_unhealthy");
        }
        Ok(())
    })
    .await
    .unwrap_or(Err("plugin_health_timeout"));
    process.close().await;
    result
}
pub struct Invocation<'a> {
    pub path: &'a Path,
    pub manifest: &'a Manifest,
    pub tool: &'a str,
    pub context: &'a Context,
    pub invocation: &'a str,
    pub store: &'a Store,
}
pub async fn invoke(
    call: Invocation<'_>,
    arguments: Value,
    cancel: CancellationToken,
) -> Result<ToolOutput, (&'static str, bool)> {
    if cancel.is_cancelled() {
        return Err(("cancelled", false));
    }
    let Invocation {
        path,
        manifest,
        tool,
        context,
        invocation,
        store,
    } = call;
    let mut process = Process::spawn(path, manifest).map_err(|code| (code, false))?;
    let handshake = tokio::select! {
        _=cancel.cancelled()=>Err("cancelled"),
        result=tokio::time::timeout(Duration::from_secs(5),process.handshake(manifest))=>result.unwrap_or(Err("plugin_health_timeout")),
    };
    if let Err(code) = handshake {
        process.close().await;
        return Err((code, false));
    }
    let params = json!({"name":tool,"arguments":arguments,"invocation_id":invocation,"deadline_ms":crate::domain::now()+manifest.timeout_ms as i64,"context":{"run_id":context.run_id,"event_id":context.event_id,"source":context.source,"task":context.task}});
    if cancel.is_cancelled() {
        process.close().await;
        return Err(("cancelled", false));
    }
    // Mark uncertainty before writing: a partial write or a lost response cannot prove no effect.
    let request = json!({"jsonrpc":"2.0","id":invocation,"method":"tools/call","params":params});
    let result = tokio::select! {
        _=cancel.cancelled()=>Err("cancelled"),
        result=tokio::time::timeout(Duration::from_millis(manifest.timeout_ms),async {
            process.send(request).await?;
            process.response(invocation,Some((store,&context.event_id))).await
        })=>result.unwrap_or(Err("plugin_timeout")),
    };
    let result = if matches!(result, Err("cancelled" | "plugin_timeout")) {
        let notification =
            json!({"jsonrpc":"2.0","method":"cancel","params":{"invocation_id":invocation}});
        let grace = tokio::time::timeout(Duration::from_millis(250), async {
            process.send(notification).await?;
            process
                .response(invocation, Some((store, &context.event_id)))
                .await
        })
        .await;
        match grace {
            Ok(Ok(body)) => Ok(body),
            _ => result,
        }
    } else {
        result
    };
    process.close().await;
    let result = result.map_err(|code| (code, true))?;
    let declaration = manifest
        .tools
        .iter()
        .find(|t| t.name == tool)
        .ok_or(("plugin_tool_missing", true))?;
    let changed = result["changed"]
        .as_bool()
        .ok_or(("plugin_protocol_error", true))?;
    if (changed && !declaration.mutates)
        || harness3_plugin_protocol::validate_value(&declaration.output_schema, &result["content"])
            .is_err()
    {
        return Err(("plugin_output_schema_mismatch", true));
    }
    Ok(ToolOutput {
        content: result["content"].clone(),
        changed,
    })
}
