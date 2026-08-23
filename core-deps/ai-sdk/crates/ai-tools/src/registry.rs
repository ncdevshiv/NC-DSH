//! Tool and skill registries.

use std::collections::HashMap;
use std::sync::Arc;

use ai_errors::{AiError, ToolError};

use crate::Tool;

/// A registry of named tools with duplicate detection.
#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRegistry")
            .field("tools", &self.names())
            .finish()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn require(&self, name: &str) -> Result<Arc<dyn Tool>, AiError> {
        self.get(name).ok_or_else(|| {
            AiError::Tool(ToolError::new(
                name,
                format!("tool `{name}` is not registered"),
            ))
        })
    }

    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.tools.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// All tools as model-facing definitions.
    pub fn definitions(&self) -> Vec<ai_core::ToolDefinition> {
        self.tools
            .values()
            .map(|tool| crate::to_tool_definition(tool.as_ref()))
            .collect()
    }
}

/// A versioned skill: a named capability bundling instructions and required
/// tools (PRD §3.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub version: String,
    pub description: String,
    /// Instructions injected into agent context.
    pub instructions: String,
    /// Tools the skill requires (must exist in the registry).
    pub required_tools: Vec<String>,
}

impl Skill {
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        description: impl Into<String>,
        instructions: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            description: description.into(),
            instructions: instructions.into(),
            required_tools: Vec::new(),
        }
    }

    pub fn with_required_tools(mut self, tools: &[&str]) -> Self {
        self.required_tools = tools.iter().map(|t| t.to_string()).collect();
        self
    }
}

/// A registry of versioned skills with dependency validation.
#[derive(Debug, Default, Clone)]
pub struct SkillRegistry {
    skills: HashMap<String, Vec<Skill>>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a skill (by name, keeping versions sorted descending).
    pub fn register(&mut self, skill: Skill) {
        let versions = self.skills.entry(skill.name.clone()).or_default();
        versions.push(skill);
        versions.sort_by(|a, b| b.version.cmp(&a.version));
    }

    /// Latest version of a skill.
    pub fn latest(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name).and_then(|v| v.first())
    }

    /// A specific version, if present.
    pub fn get(&self, name: &str, version: &str) -> Option<&Skill> {
        self.skills
            .get(name)
            .and_then(|v| v.iter().find(|s| s.version == version))
    }

    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.skills.keys().cloned().collect();
        names.sort();
        names
    }

    /// Validates that all required tools of all registered skills exist in
    /// the tool registry.
    pub fn validate_dependencies(&self, tools: &ToolRegistry) -> Result<(), AiError> {
        for (name, versions) in &self.skills {
            for skill in versions {
                for required in &skill.required_tools {
                    if tools.get(required).is_none() {
                        return Err(AiError::Tool(ToolError::new(
                            name,
                            format!(
                                "skill `{name}` v{} requires tool `{required}` which is not registered",
                                skill.version
                            ),
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_registry_deduplicates_and_lists() {
        let registry = crate::default_tools();
        assert!(registry.len() >= 4);
        assert!(registry.get("calculator").is_some());
        assert!(registry.get("nope").is_none());
        assert!(registry.require("nope").is_err());
        let names = registry.names();
        assert_eq!(names.len(), registry.len());
    }

    #[test]
    fn skill_registry_versions_and_dependency_check() {
        let mut skills = SkillRegistry::new();
        skills.register(
            Skill::new("code-review", "1.0.0", "reviews code", "be a reviewer")
                .with_required_tools(&["http_get"]),
        );
        skills.register(Skill::new(
            "code-review",
            "0.9.0",
            "older",
            "older instructions",
        ));
        assert_eq!(skills.latest("code-review").unwrap().version, "1.0.0");
        assert!(skills.get("code-review", "0.9.0").is_some());
        assert!(skills.get("code-review", "9.9.9").is_none());

        let tools = crate::default_tools();
        assert!(skills.validate_dependencies(&tools).is_ok());

        skills.register(
            Skill::new("data-analysis", "1.0.0", "analyzes", "instructions")
                .with_required_tools(&["missing-tool"]),
        );
        let err = skills.validate_dependencies(&tools).unwrap_err();
        assert!(err.to_string().contains("missing-tool"), "{err}");
    }
}
