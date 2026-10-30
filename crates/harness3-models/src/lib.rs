//! Non-streaming protocol adapters. No provider/account is needed for demo mode.
use harness3::{
    Call, CancellationToken, Error, LiveOutput, Model, ModelInput, ModelReply, Result, TokenUsage,
    Turn,
};
use harness3_domain::{PResult, Protocol, fault};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::time::Duration;

pub struct Provider {
    pub protocol: Protocol,
    pub model: String,
    pub base: String,
    key: String,
    client: reqwest::Client,
}
impl Provider {
    pub fn from_env(protocol: Protocol) -> PResult<Self> {
        let (prefix, default) = match protocol {
            Protocol::OpenaiCompatible => ("OPENAI_COMPATIBLE", "https://api.openai.com/v1"),
            Protocol::OpenaiResponses => ("OPENAI_RESPONSES", "https://api.openai.com/v1"),
            Protocol::Anthropic => ("ANTHROPIC", "https://api.anthropic.com/v1"),
            Protocol::Demo => return Err(fault(400, "demo_has_no_provider")),
        };
        let model = std::env::var(format!("HARNESS3_{prefix}_MODEL"))
            .map_err(|_| fault(400, "model_not_configured"))?;
        let key = std::env::var(format!("HARNESS3_{prefix}_API_KEY")).unwrap_or_default();
        let base =
            std::env::var(format!("HARNESS3_{prefix}_BASE_URL")).unwrap_or_else(|_| default.into());
        Self::new(protocol, model, base, key)
    }
    pub fn new(protocol: Protocol, model: String, base: String, key: String) -> PResult<Self> {
        let url = reqwest::Url::parse(&base).map_err(|_| fault(400, "invalid_model_base_url"))?;
        if model.trim().is_empty()
            || !["http", "https"].contains(&url.scheme())
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(fault(400, "invalid_model_configuration"));
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| fault(500, "http_client_error"))?;
        Ok(Self {
            protocol,
            model,
            base: base.trim_end_matches('/').into(),
            key,
            client,
        })
    }
    pub fn request(&self, input: &ModelInput, limit: u64) -> Value {
        let mut history = vec![];
        // Core separates the first user message into `text`; `turns` is the suffix.
        let mut turns = vec![Turn::User(input.text.clone())];
        turns.extend(input.turns.clone());
        match self.protocol {
            Protocol::OpenaiCompatible => {
                history.push(json!({"role":"system","content":input.system}));
                for turn in &turns {
                    match turn {
                        Turn::User(text)=>history.push(json!({"role":"user","content":text})),
                        Turn::Context(value)=>history.push(json!({"role":"user","content":format!("Untrusted task context: {value}")})),
                        Turn::Assistant(reply)=>{
                            let mut message=json!({"role":"assistant","content":reply.text});
                            if !reply.calls.is_empty() {message["tool_calls"]=json!(reply.calls.iter().map(|call|json!({"id":call.id,"type":"function","function":{"name":wire_tool_name(&call.name),"arguments":call.arguments.to_string()}})).collect::<Vec<_>>());}
                            history.push(message);
                        }
                        Turn::Tools(observations)=>for observation in observations {history.push(json!({"role":"tool","tool_call_id":observation.call.id,"content":observation.output.content.to_string()}));}
                    }
                }
                let mut body = json!({"model":self.model,"messages":history,"max_completion_tokens":limit,"stream":false});
                if !input.tools.is_empty() {
                    body["tools"]=json!(input.tools.iter().map(|tool|json!({"type":"function","function":{"name":wire_tool_name(&tool.name),"description":tool.description,"parameters":tool.parameters}})).collect::<Vec<_>>());
                }
                body
            }
            Protocol::OpenaiResponses => {
                for turn in &turns {
                    match turn {
                        Turn::User(text)=>history.push(json!({"role":"user","content":text})),
                        Turn::Context(value)=>history.push(json!({"role":"user","content":format!("Untrusted task context: {value}")})),
                        Turn::Assistant(reply)=> {
                            if let Some(items)=reply.continuation.get("output").and_then(Value::as_array) {history.extend(items.clone());}
                            else {
                                if !reply.text.is_empty(){history.push(json!({"role":"assistant","content":reply.text}));}
                                for call in &reply.calls {history.push(json!({"type":"function_call","call_id":call.id,"name":wire_tool_name(&call.name),"arguments":call.arguments.to_string()}));}
                            }
                        }
                        Turn::Tools(observations)=>for observation in observations {history.push(json!({"type":"function_call_output","call_id":observation.call.id,"output":observation.output.content.to_string()}));}
                    }
                }
                let mut body = json!({"model":self.model,"instructions":input.system,"input":history,"max_output_tokens":limit,"stream":false,"store":false,"include":["reasoning.encrypted_content"]});
                if !input.tools.is_empty() {
                    body["tools"]=json!(input.tools.iter().map(|tool|json!({"type":"function","name":wire_tool_name(&tool.name),"description":tool.description,"parameters":tool.parameters,"strict":false})).collect::<Vec<_>>());
                }
                body
            }
            Protocol::Anthropic => {
                for turn in &turns {
                    let message = match turn {
                        Turn::User(text) => {
                            json!({"role":"user","content":[{"type":"text","text":text}]})
                        }
                        Turn::Context(value) => {
                            json!({"role":"user","content":[{"type":"text","text":format!("Untrusted task context: {value}")}]})
                        }
                        Turn::Assistant(reply) => {
                            let blocks = if let Some(blocks) =
                                reply.continuation.get("content").and_then(Value::as_array)
                            {
                                blocks.clone()
                            } else {
                                let mut blocks = vec![];
                                if !reply.text.is_empty() {
                                    blocks.push(json!({"type":"text","text":reply.text}));
                                }
                                for call in &reply.calls {
                                    blocks.push(json!({"type":"tool_use","id":call.id,"name":wire_tool_name(&call.name),"input":call.arguments}));
                                }
                                blocks
                            };
                            json!({"role":"assistant","content":blocks})
                        }
                        Turn::Tools(observations) => {
                            json!({"role":"user","content":observations.iter().map(|o|json!({"type":"tool_result","tool_use_id":o.call.id,"content":o.output.content.to_string()})).collect::<Vec<_>>()})
                        }
                    };
                    // Anthropic expects alternating roles; merge adjacent user/context blocks.
                    if let Some(last) = history
                        .last_mut()
                        .filter(|last| last["role"] == message["role"])
                    {
                        if let (Some(to), Some(from)) = (
                            last["content"].as_array_mut(),
                            message["content"].as_array(),
                        ) {
                            to.extend(from.clone());
                        }
                    } else {
                        history.push(message);
                    }
                }
                let mut body = json!({"model":self.model,"system":input.system,"messages":history,"max_tokens":limit,"stream":false});
                if !input.tools.is_empty() {
                    body["tools"]=json!(input.tools.iter().map(|tool|json!({"name":wire_tool_name(&tool.name),"description":tool.description,"input_schema":tool.parameters})).collect::<Vec<_>>());
                }
                body
            }
            Protocol::Demo => json!({}),
        }
    }
    pub fn decode(protocol: &Protocol, body: Value) -> Result<ModelReply> {
        let mut reply = ModelReply {
            text: String::new(),
            calls: vec![],
            usage: None,
            continuation: Value::Null,
            reasoning: None,
        };
        let malformed = || Error::new("provider_protocol_error");
        match protocol {
            Protocol::OpenaiCompatible => {
                let choice = body["choices"]
                    .as_array()
                    .and_then(|v| v.first())
                    .ok_or_else(malformed)?;
                let finish = choice["finish_reason"].as_str().ok_or_else(malformed)?;
                if !["stop", "tool_calls"].contains(&finish) {
                    return Err(Error::new("provider_incomplete"));
                }
                let message = &choice["message"];
                if message["refusal"].as_str().is_some() {
                    return Err(Error::new("provider_refusal"));
                }
                reply.text = message["content"].as_str().unwrap_or_default().into();
                if let Some(calls) = message["tool_calls"].as_array() {
                    for call in calls {
                        reply.calls.push(Call {
                            id: required(call, "id")?,
                            name: required(&call["function"], "name")?,
                            arguments: serde_json::from_str(
                                call["function"]["arguments"]
                                    .as_str()
                                    .ok_or_else(malformed)?,
                            )
                            .map_err(|_| Error::new("invalid_tool_json"))?,
                        });
                    }
                }
                reply.usage = usage(
                    &body["usage"],
                    "prompt_tokens",
                    "completion_tokens",
                    body["usage"]["prompt_tokens_details"]["cached_tokens"].as_u64(),
                );
            }
            Protocol::OpenaiResponses => {
                if body["status"] != "completed" {
                    return Err(Error::new("provider_incomplete"));
                }
                let output = body["output"].as_array().ok_or_else(malformed)?;
                for item in output {
                    match item["type"].as_str() {
                        Some("function_call") => reply.calls.push(Call {
                            id: required(item, "call_id")?,
                            name: required(item, "name")?,
                            arguments: serde_json::from_str(
                                item["arguments"].as_str().ok_or_else(malformed)?,
                            )
                            .map_err(|_| Error::new("invalid_tool_json"))?,
                        }),
                        Some("message") => {
                            if let Some(content) = item["content"].as_array() {
                                for block in content {
                                    if block["type"] == "refusal" {
                                        return Err(Error::new("provider_refusal"));
                                    }
                                    if block["type"] == "output_text" {
                                        reply.text.push_str(
                                            block["text"].as_str().ok_or_else(malformed)?,
                                        );
                                    }
                                }
                            }
                        }
                        Some("reasoning") => {}
                        _ => return Err(Error::new("unsupported_provider_output")),
                    }
                }
                reply.continuation = json!({"output":output});
                reply.usage = usage(
                    &body["usage"],
                    "input_tokens",
                    "output_tokens",
                    body["usage"]["input_tokens_details"]["cached_tokens"].as_u64(),
                );
            }
            Protocol::Anthropic => {
                if ![Some("end_turn"), Some("tool_use"), Some("stop_sequence")]
                    .contains(&body["stop_reason"].as_str())
                {
                    return Err(Error::new("provider_incomplete"));
                }
                let content = body["content"].as_array().ok_or_else(malformed)?;
                for block in content {
                    match block["type"].as_str() {
                        Some("text") => reply
                            .text
                            .push_str(block["text"].as_str().ok_or_else(malformed)?),
                        Some("tool_use") => reply.calls.push(Call {
                            id: required(block, "id")?,
                            name: required(block, "name")?,
                            arguments: block.get("input").cloned().ok_or_else(malformed)?,
                        }),
                        _ => return Err(Error::new("unsupported_provider_output")),
                    }
                }
                reply.continuation = json!({"content":content});
                let u = &body["usage"];
                reply.usage = usage(
                    u,
                    "input_tokens",
                    "output_tokens",
                    u["cache_read_input_tokens"].as_u64(),
                );
                // Anthropic input_tokens excludes cache reads and cache writes.
                if let Some(usage) = &mut reply.usage {
                    usage.input = usage
                        .input
                        .saturating_add(u["cache_read_input_tokens"].as_u64().unwrap_or(0))
                        .saturating_add(u["cache_creation_input_tokens"].as_u64().unwrap_or(0));
                }
            }
            Protocol::Demo => return Err(malformed()),
        }
        Ok(reply)
    }
}
fn wire_tool_name(name: &str) -> String {
    if !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
    {
        return name.into();
    }
    let stem: String = name
        .chars()
        .take(38)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let digest = format!("{:x}", Sha256::digest(name.as_bytes()));
    format!("h3_{stem}_{}", &digest[..16])
}
fn required(body: &Value, key: &str) -> Result<String> {
    body[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| Error::new("provider_protocol_error"))
}
fn usage(value: &Value, input: &str, output: &str, cached: Option<u64>) -> Option<TokenUsage> {
    Some(TokenUsage {
        input: value[input].as_u64()?,
        output: value[output].as_u64()?,
        cached_input: cached,
    })
}
#[async_trait::async_trait]
impl Model for Provider {
    fn metadata(&self) -> Value {
        json!({"protocol":self.protocol,"model":self.model,"base_url":self.base,"stream":false})
    }
    async fn complete(
        &self,
        input: ModelInput,
        live: &LiveOutput,
        cancel: CancellationToken,
    ) -> Result<ModelReply> {
        let path = match self.protocol {
            Protocol::OpenaiCompatible => "chat/completions",
            Protocol::OpenaiResponses => "responses",
            Protocol::Anthropic => "messages",
            Protocol::Demo => return Err(Error::new("invalid_provider")),
        };
        let names: Vec<_> = input
            .tools
            .iter()
            .map(|tool| (wire_tool_name(&tool.name), tool.name.clone()))
            .collect();
        let unique: std::collections::HashSet<_> = names.iter().map(|(wire, _)| wire).collect();
        if unique.len() != names.len() {
            return Err(Error::new("provider_tool_alias_collision"));
        }
        let body = self.request(&input, live.max_output_tokens().unwrap_or(4096));
        let mut request = self
            .client
            .post(format!("{}/{path}", self.base))
            .json(&body);
        if self.protocol == Protocol::Anthropic {
            request = request.header("anthropic-version", "2023-06-01");
            if !self.key.is_empty() {
                request = request.header("x-api-key", &self.key);
            }
        } else if !self.key.is_empty() {
            request = request.bearer_auth(&self.key);
        }
        let mut response = tokio::select! {
            _=cancel.cancelled()=>return Err(Error::new("cancelled")),
            response=request.send()=>response.map_err(|_|Error::new("provider_network_error"))?,
        };
        let status = response.status();
        let mut bytes = vec![];
        while let Some(chunk) = tokio::select! {_=cancel.cancelled()=>return Err(Error::new("cancelled")),chunk=response.chunk()=>chunk.map_err(|_|Error::new("provider_network_error"))?}
        {
            if bytes.len() + chunk.len() > 4 * 1024 * 1024 {
                return Err(Error::new("provider_response_too_large"));
            }
            bytes.extend_from_slice(&chunk);
        }
        if !status.is_success() {
            let provider_error = serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null);
            let mut message = provider_error["error"]["message"]
                .as_str()
                .unwrap_or("Provider rejected the request")
                .to_string();
            if !self.key.is_empty() {
                message = message.replace(&self.key, "[redacted]");
            }
            message = message.chars().take(600).collect();
            return Err(Error::new(format!("provider_http_{}", status.as_u16()))
                .explain(message, json!({"http_status":status.as_u16()})));
        }
        let body =
            serde_json::from_slice(&bytes).map_err(|_| Error::new("provider_protocol_error"))?;
        let mut reply = Self::decode(&self.protocol, body)?;
        for call in &mut reply.calls {
            if let Some((_, original)) = names.iter().find(|(wire, _)| wire == &call.name) {
                call.name = original.clone();
            }
        }
        Ok(reply)
    }
}

#[derive(Clone, Default)]
pub struct Models {
    pub demo_only: bool,
    pub compatible: Option<std::sync::Arc<Provider>>,
    pub responses: Option<std::sync::Arc<Provider>>,
    pub anthropic: Option<std::sync::Arc<Provider>>,
}
impl Models {
    pub fn from_env() -> Self {
        Self {
            demo_only: false,
            compatible: Provider::from_env(Protocol::OpenaiCompatible)
                .ok()
                .map(std::sync::Arc::new),
            responses: Provider::from_env(Protocol::OpenaiResponses)
                .ok()
                .map(std::sync::Arc::new),
            anthropic: Provider::from_env(Protocol::Anthropic)
                .ok()
                .map(std::sync::Arc::new),
        }
    }
    pub fn load_codex_responses(&mut self, root: &std::path::Path, model: &str) -> PResult<()> {
        let text = std::fs::read_to_string(root.join("config.toml"))
            .map_err(|_| fault(400, "codex_config_unavailable"))?;
        let config: toml::Value =
            toml::from_str(&text).map_err(|_| fault(400, "codex_config_invalid"))?;
        let provider = config
            .get("model_provider")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| fault(400, "codex_provider_missing"))?;
        let selected = config
            .get("model_providers")
            .and_then(|v| v.get(provider))
            .ok_or_else(|| fault(400, "codex_provider_missing"))?;
        if selected.get("wire_api").and_then(toml::Value::as_str) != Some("responses") {
            return Err(fault(400, "codex_provider_is_not_responses"));
        }
        let base = selected
            .get("base_url")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| fault(400, "codex_base_url_missing"))?;
        let key = if let Some(env) = selected.get("env_key").and_then(toml::Value::as_str) {
            std::env::var(env).map_err(|_| fault(400, "codex_provider_env_key_missing"))?
        } else {
            let text = std::fs::read_to_string(root.join("auth.json"))
                .map_err(|_| fault(400, "codex_auth_unavailable"))?;
            let auth: Value =
                serde_json::from_str(&text).map_err(|_| fault(400, "codex_auth_invalid"))?;
            auth["OPENAI_API_KEY"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| fault(400, "codex_api_key_missing"))?
                .to_owned()
        };
        self.responses = Some(std::sync::Arc::new(Provider::new(
            Protocol::OpenaiResponses,
            model.into(),
            base.into(),
            key,
        )?));
        Ok(())
    }
    pub fn get(&self, protocol: &Protocol) -> PResult<std::sync::Arc<Provider>> {
        match protocol {
            Protocol::OpenaiCompatible => self.compatible.clone(),
            Protocol::OpenaiResponses => self.responses.clone(),
            Protocol::Anthropic => self.anthropic.clone(),
            Protocol::Demo => None,
        }
        .ok_or_else(|| fault(400, "model_not_configured"))
    }
    pub fn describe(&self) -> Value {
        let info = |protocol: &str, provider: &Option<std::sync::Arc<Provider>>| json!({"protocol":protocol,"configured":provider.is_some(),"model":provider.as_ref().map(|p|p.model.clone())});
        json!([{"protocol":"demo","configured":true,"model":"deterministic"},info("openai-compatible",&self.compatible),info("openai-responses",&self.responses),info("anthropic",&self.anthropic)])
    }
}
