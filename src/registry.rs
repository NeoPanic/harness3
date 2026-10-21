//! Transactional registration of capabilities and typed services.
use crate::*;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Default, Clone)]
pub struct Registry {
    pub(crate) services: BTreeMap<String, Arc<dyn std::any::Any + Send + Sync>>,
    pub(crate) models: BTreeMap<String, Arc<dyn Model>>,
    pub(crate) tools: BTreeMap<String, Arc<dyn Tool>>,
    pub(crate) sources: BTreeMap<String, Arc<dyn Source>>,
    pub(crate) tasks: BTreeMap<String, AgentProfile>,
    pub(crate) skills: BTreeMap<String, Arc<dyn skills::Skill>>,
    pub(crate) observers: Vec<Arc<dyn Observer>>,
    pub(crate) trace: Option<Arc<dyn trace::TraceSink>>,
    pub(crate) prompts: BTreeMap<String, Arc<dyn prompt::PromptSection>>,
    transports: BTreeMap<String, Arc<dyn transport::Transport>>,
    pub(crate) policies: Vec<Arc<dyn policy::Policy>>,
}
pub(crate) fn name(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 100
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    {
        Err(Error::new("invalid_registration_name"))
    } else {
        Ok(())
    }
}
impl Registry {
    pub fn policy(&mut self, policy: Arc<dyn policy::Policy>) -> Result<()> {
        name(policy.name())?;
        if self.policies.iter().any(|p| p.name() == policy.name()) {
            return Err(Error::new("duplicate_policy"));
        }
        self.policies.push(policy);
        Ok(())
    }
    pub fn transport(&mut self, id: &str, transport: Arc<dyn transport::Transport>) -> Result<()> {
        name(id)?;
        if self.transports.contains_key(id) {
            return Err(Error::new("duplicate_transport"));
        }
        self.transports.insert(id.into(), transport);
        Ok(())
    }
    /// Only the host calls this after claiming its durable delivery ledger.
    /// It never retries or sends as a consequence of model completion alone.
    pub async fn deliver(
        &self,
        id: &str,
        message: &transport::Outbound,
        cancel: CancellationToken,
    ) -> Result<transport::Receipt> {
        if message.id.trim().is_empty()
            || message.recipient.trim().is_empty()
            || message.text.trim().is_empty()
        {
            return Err(Error::new("invalid_outbound"));
        }
        if cancel.is_cancelled() {
            return Err(Error::new("cancelled"));
        }
        self.transports
            .get(id)
            .ok_or_else(|| Error::new("unknown_transport"))?
            .send(message, cancel)
            .await
    }
    pub fn provide<T: Send + Sync + 'static>(&mut self, key: &str, value: Arc<T>) -> Result<()> {
        name(key)?;
        if self.services.contains_key(key) {
            return Err(Error::new("duplicate_service"));
        }
        self.services.insert(key.into(), value);
        Ok(())
    }
    pub fn service<T: Send + Sync + 'static>(&self, key: &str) -> Result<Arc<T>> {
        self.services
            .get(key)
            .cloned()
            .ok_or_else(|| Error::new("missing_service"))?
            .downcast()
            .map_err(|_| Error::new("service_type_mismatch"))
    }
    pub fn has_service(&self, key: &str) -> bool {
        self.services.contains_key(key)
    }
    pub fn register(&mut self, register: impl FnOnce(&mut Self) -> Result<()>) -> Result<()> {
        let mut staged = self.clone();
        register(&mut staged)?;
        *self = staged;
        Ok(())
    }
    pub fn model(&mut self, id: &str, model: Arc<dyn Model>) -> Result<()> {
        name(id)?;
        if self.models.contains_key(id) {
            return Err(Error::new("duplicate_model"));
        }
        self.models.insert(id.into(), model);
        Ok(())
    }
    pub fn tool(&mut self, tool: Arc<dyn Tool>) -> Result<()> {
        let spec = tool.spec();
        name(&spec.name)?;
        if self.tools.contains_key(&spec.name) {
            return Err(Error::new("duplicate_tool"));
        }
        self.tools.insert(spec.name, tool);
        Ok(())
    }
    pub fn source(&mut self, source: Arc<dyn Source>) -> Result<()> {
        name(source.name())?;
        if self.sources.contains_key(source.name()) {
            return Err(Error::new("duplicate_source"));
        }
        self.sources.insert(source.name().into(), source);
        Ok(())
    }
    pub fn task(&mut self, task: AgentProfile) -> Result<()> {
        name(&task.name)?;
        if self.tasks.contains_key(&task.name) {
            return Err(Error::new("duplicate_task"));
        }
        self.tasks.insert(task.name.clone(), task);
        Ok(())
    }
    pub fn skill(&mut self, skill: Arc<dyn skills::Skill>) -> Result<()> {
        let info = skill.info();
        name(&info.name)?;
        if self.skills.contains_key(&info.name) {
            return Err(Error::new("duplicate_skill"));
        }
        self.skills.insert(info.name, skill);
        Ok(())
    }
    pub fn skill_names(&self) -> Vec<String> {
        self.skills.keys().cloned().collect()
    }
    pub fn tool_names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }
    pub fn tool_spec(&self, name: &str) -> Option<ToolSpec> {
        self.tools.get(name).map(|tool| tool.spec())
    }
    /// Host authorization is applied before publishing an execution registry.
    pub fn retain_tools(&mut self, allowed: impl Fn(&ToolSpec) -> bool) {
        self.tools.retain(|_, tool| allowed(&tool.spec()));
    }
    pub fn prompt(&mut self, section: Arc<dyn prompt::PromptSection>) -> Result<()> {
        name(section.name())?;
        if self.prompts.contains_key(section.name()) {
            return Err(Error::new("duplicate_prompt"));
        }
        self.prompts.insert(section.name().into(), section);
        Ok(())
    }
    pub fn trace(&mut self, sink: Arc<dyn trace::TraceSink>) -> Result<()> {
        if self.trace.is_some() {
            return Err(Error::new("duplicate_trace"));
        }
        self.trace = Some(sink);
        Ok(())
    }
    pub fn observer(&mut self, observer: Arc<dyn Observer>) {
        self.observers.push(observer);
    }
}
