use crate::network;
use harness3::{CancellationToken, Context, Error, Result, Tool, ToolOutput, ToolSpec};
use serde::Deserialize;
use serde_json::{Value, json};
pub struct Search;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    query: String,
    #[serde(default = "default_count")]
    count: usize,
}
fn default_count() -> usize {
    5
}
fn results(value: Value, count: usize) -> Result<Value> {
    let entries = value["web"]["results"]
        .as_array()
        .ok_or_else(|| Error::new("web_search_invalid_response"))?;
    Ok(Value::Array(
        entries
            .iter()
            .take(count)
            .map(|v| json!({"title":v["title"],"url":v["url"],"description":v["description"]}))
            .collect(),
    ))
}
#[async_trait::async_trait]
impl Tool for Search {
    fn spec(&self) -> ToolSpec {
        ToolSpec{name:"web.search".into(),description:"Search the web using Brave Search. Sends the query to an external provider and may incur charges. Returns titles, URLs and snippets, not verified facts. Requires HARNESS3_WEB_SEARCH_API_KEY.".into(),parameters:json!({"type":"object","properties":{"query":{"type":"string","minLength":1,"maxLength":2000},"count":{"type":"integer","minimum":1,"maximum":10}},"required":["query"],"additionalProperties":false}),mutates:false}
    }
    fn parallel_safe(&self) -> bool {
        true
    }
    async fn execute(
        &self,
        _: &Context,
        args: Value,
        cancel: CancellationToken,
    ) -> Result<ToolOutput> {
        let input: Input =
            serde_json::from_value(args).map_err(|_| Error::new("invalid_arguments"))?;
        if input.query.trim().is_empty()
            || input.query.len() > 2000
            || !(1..=10).contains(&input.count)
        {
            return Err(Error::new("invalid_arguments"));
        }
        let key = std::env::var("HARNESS3_WEB_SEARCH_API_KEY")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .ok_or_else(|| Error::new("web_search_not_configured"))?;
        let client = network::client()?
            .build()
            .map_err(|_| Error::new("web_client_failed"))?;
        let response = tokio::select! {_=cancel.cancelled()=>return Err(Error::new("cancelled")),r=client.get("https://api.search.brave.com/res/v1/web/search").header("X-Subscription-Token",key).query(&[("q",input.query.as_str()),("count",&input.count.to_string())]).send()=>r.map_err(|_|Error::new("web_search_request_failed"))?};
        let bytes = network::body(response, &cancel).await?;
        let value = serde_json::from_slice(&bytes)
            .map_err(|_| Error::new("web_search_invalid_response"))?;
        Ok(ToolOutput {
            content: json!({"query":input.query,"provider":"brave","results":results(value,input.count)?,"untrusted":true}),
            changed: false,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keeps_citations_and_bounds_provider_results() {
        let v=results(json!({"web":{"results":[{"title":"A","url":"https://example.com","description":"evidence","extra":"discard"},{"title":"B"}]}}),1).unwrap();
        assert_eq!(v.as_array().unwrap().len(), 1);
        assert_eq!(v[0]["url"], "https://example.com");
        assert!(v[0].get("extra").is_none());
        assert!(results(json!({"error":"provider secret"}), 5).is_err());
    }
}
