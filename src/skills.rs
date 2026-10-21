//! Content bundles for native capabilities. Files are read afresh per run;
//! scripts may be listed as resources but this module never executes them.
use crate::{CancellationToken, Context, Error, Result, Tool, ToolOutput, ToolSpec};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::Read,
    path::{Component, Path, PathBuf},
    sync::Arc,
};
const MAX_FILE: usize = 64 * 1024;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub id: String,
    pub path: String,
    pub description: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub tools: Vec<String>,
    #[serde(default)]
    pub initial_tools: Vec<String>,
    pub skills: Vec<String>,
    #[serde(default)]
    pub resources: Vec<Resource>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
}
pub struct LoadedSkill {
    pub info: SkillInfo,
    pub instructions: String,
    pub revision: String,
}
#[async_trait]
pub trait Skill: Send + Sync {
    fn info(&self) -> SkillInfo;
    fn tools(&self) -> Vec<String> {
        vec![]
    }
    fn initial_tools(&self) -> Vec<String> {
        vec![]
    }
    async fn load(&self, cancel: CancellationToken) -> Result<LoadedSkill>;
}
#[derive(Clone)]
pub struct FileBundle {
    root: PathBuf,
    embedded: Option<&'static [(&'static str, &'static str)]>,
    pub manifest: Manifest,
}
fn bounded_text(path: &Path) -> Result<String> {
    let mut text = String::new();
    std::fs::File::open(path)
        .map_err(|_| Error::new("resource_file_unavailable"))?
        .take(MAX_FILE as u64 + 1)
        .read_to_string(&mut text)
        .map_err(|_| Error::new("resource_file_invalid"))?;
    if text.len() > MAX_FILE {
        return Err(Error::new("resource_file_limit"));
    }
    Ok(text)
}
fn relative(path: &str) -> Result<()> {
    if path.is_empty()
        || Path::new(path)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(Error::new("resource_path_invalid"));
    }
    Ok(())
}
fn parse_skill(text: &str) -> Result<(SkillInfo, String)> {
    let normalized = text.replace("\r\n", "\n");
    let remainder = normalized
        .strip_prefix("---\n")
        .ok_or_else(|| Error::new("skill_frontmatter_missing"))?;
    let (header, body) = remainder
        .split_once("\n---\n")
        .ok_or_else(|| Error::new("skill_frontmatter_invalid"))?;
    let info: SkillInfo =
        serde_yaml_ng::from_str(header).map_err(|_| Error::new("skill_frontmatter_invalid"))?;
    crate::name(&info.name)?;
    if info.description.trim().is_empty() || info.description.len() > 2048 || body.trim().is_empty()
    {
        return Err(Error::new("skill_content_invalid"));
    }
    Ok((info, body.trim().into()))
}
impl FileBundle {
    pub fn embedded(files: &'static [(&'static str, &'static str)]) -> Result<Self> {
        let text = files
            .iter()
            .find(|(path, _)| *path == "manifest.json")
            .ok_or_else(|| Error::new("capability_manifest_unavailable"))?
            .1;
        let manifest: Manifest =
            serde_json::from_str(text).map_err(|_| Error::new("capability_manifest_invalid"))?;
        crate::name(&manifest.id)?;
        Ok(Self {
            root: PathBuf::new(),
            embedded: Some(files),
            manifest,
        })
    }
    pub fn open(root: &Path) -> Result<Self> {
        let root = root
            .canonicalize()
            .map_err(|_| Error::new("resource_directory_unavailable"))?;
        let manifest_path = root
            .join("manifest.json")
            .canonicalize()
            .map_err(|_| Error::new("capability_manifest_unavailable"))?;
        if !manifest_path.starts_with(&root) {
            return Err(Error::new("resource_path_invalid"));
        }
        let manifest: Manifest = serde_json::from_str(&bounded_text(&manifest_path)?)
            .map_err(|_| Error::new("capability_manifest_invalid"))?;
        crate::name(&manifest.id)?;
        if manifest.name.trim().is_empty()
            || manifest.name.len() > 256
            || manifest.version.is_empty()
            || manifest.version.len() > 64
            || manifest.description.len() > 4096
            || manifest.skills.len() > 16
            || manifest.resources.len() > 32
            || manifest.tools.len() > 32
        {
            return Err(Error::new("capability_manifest_invalid"));
        }
        let mut ids = BTreeSet::new();
        for resource in &manifest.resources {
            crate::name(&resource.id)?;
            relative(&resource.path)?;
            if !ids.insert(&resource.id) {
                return Err(Error::new("duplicate_resource"));
            }
        }
        for path in &manifest.skills {
            relative(path)?;
        }
        ids.clear();
        for tool in &manifest.tools {
            crate::name(tool)?;
            if !ids.insert(tool) {
                return Err(Error::new("duplicate_tool"));
            }
        }
        Ok(Self {
            root,
            embedded: None,
            manifest,
        })
    }
    pub fn directory(&self) -> &Path {
        &self.root
    }
    fn text(&self, path: &str) -> Result<String> {
        relative(path)?;
        if let Some(files) = self.embedded {
            return files
                .iter()
                .find(|(name, _)| *name == path)
                .map(|(_, text)| text.to_string())
                .ok_or_else(|| Error::new("resource_file_unavailable"));
        }
        let path = self
            .root
            .join(path)
            .canonicalize()
            .map_err(|_| Error::new("resource_file_unavailable"))?;
        if !path.starts_with(&self.root) {
            return Err(Error::new("resource_path_invalid"));
        }
        bounded_text(&path)
    }
    pub fn skills(&self) -> Result<Vec<Arc<dyn Skill>>> {
        if self.manifest.initial_tools.len() > 8
            || self
                .manifest
                .initial_tools
                .iter()
                .any(|name| !self.manifest.tools.contains(name))
        {
            return Err(Error::new("capability_manifest_invalid"));
        }
        let mut names = BTreeSet::new();
        self.manifest
            .skills
            .iter()
            .map(|path| {
                let (info, _) = parse_skill(&self.text(path)?)?;
                if !names.insert(info.name.clone()) {
                    return Err(Error::new("duplicate_skill"));
                }
                Ok(Arc::new(FileSkill {
                    bundle: self.clone(),
                    path: path.clone(),
                    info,
                }) as Arc<dyn Skill>)
            })
            .collect()
    }
    pub fn resource(&self, id: &str) -> Result<String> {
        let item = self
            .manifest
            .resources
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| Error::new("unknown_capability_resource"))?;
        self.text(&item.path)
    }
    pub fn resource_tool_name(&self) -> String {
        format!(
            "skill_read_{}_resource",
            self.manifest.id.replace(['-', '.'], "_")
        )
    }
    pub fn resource_tool(&self) -> Arc<dyn Tool> {
        Arc::new(ResourceReader {
            bundle: self.clone(),
        })
    }
}
struct FileSkill {
    bundle: FileBundle,
    path: String,
    info: SkillInfo,
}
#[async_trait]
impl Skill for FileSkill {
    fn info(&self) -> SkillInfo {
        self.info.clone()
    }
    fn tools(&self) -> Vec<String> {
        let mut tools = self.bundle.manifest.tools.clone();
        if !self.bundle.manifest.resources.is_empty() {
            tools.push(self.bundle.resource_tool_name());
        }
        tools
    }
    fn initial_tools(&self) -> Vec<String> {
        let mut tools = self.bundle.manifest.initial_tools.clone();
        if !self.bundle.manifest.resources.is_empty() {
            tools.push(self.bundle.resource_tool_name());
        }
        tools
    }
    async fn load(&self, cancel: CancellationToken) -> Result<LoadedSkill> {
        if cancel.is_cancelled() {
            return Err(Error::new("cancelled"));
        }
        let bundle = self.bundle.clone();
        let path = self.path.clone();
        let expected = self.info.name.clone();
        tokio::task::spawn_blocking(move || {
            let text = bundle.text(&path)?;
            let (info, body) = parse_skill(&text)?;
            if info.name != expected {
                return Err(Error::new("skill_changed_during_run"));
            }
            let mut instructions = body;
            if !bundle.manifest.resources.is_empty() {
                instructions.push_str(&format!(
                    "\n\n参考资源（通过 {} 按 id 读取，不执行脚本）：",
                    bundle.resource_tool_name()
                ));
                for resource in &bundle.manifest.resources {
                    instructions.push_str(&format!("\n{}: {}", resource.id, resource.description));
                }
            }
            let revision = format!("{:x}", Sha256::digest(instructions.as_bytes()));
            Ok(LoadedSkill {
                info,
                instructions,
                revision,
            })
        })
        .await
        .map_err(|_| Error::new("skill_load_failed"))?
    }
}
struct ResourceReader {
    bundle: FileBundle,
}
#[async_trait]
impl Tool for ResourceReader {
    fn parallel_safe(&self) -> bool {
        true
    }
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.bundle.resource_tool_name(),
            description: format!(
                "按 ID 读取 {} 插件声明的参考资源",
                self.bundle.manifest.name
            ),
            parameters: json!({"type":"object","properties":{"id":{"type":"string","enum":self.bundle.manifest.resources.iter().map(|r|&r.id).collect::<Vec<_>>()}},"required":["id"],"additionalProperties":false}),
            mutates: false,
        }
    }
    async fn execute(
        &self,
        _: &Context,
        arguments: Value,
        cancel: CancellationToken,
    ) -> Result<ToolOutput> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Args {
            id: String,
        }
        let args: Args =
            serde_json::from_value(arguments).map_err(|_| Error::new("invalid_arguments"))?;
        if cancel.is_cancelled() {
            return Err(Error::new("cancelled"));
        }
        let bundle = self.bundle.clone();
        let text = tokio::task::spawn_blocking(move || bundle.resource(&args.id))
            .await
            .map_err(|_| Error::new("resource_read_failed"))??;
        Ok(ToolOutput {
            content: json!({"text":text}),
            changed: false,
        })
    }
}
