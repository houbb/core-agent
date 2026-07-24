//! SkillExecutor — resolves skill definitions and generates executable prompt context.
//!
//! A Skill is a reusable workflow that combines multiple tools into structured
//! instructions. The SkillExecutor parses a SKILL.md, validates its tool
//! dependencies against the ToolManager, and produces a prompt that an Agent
//! can use to execute the skill.

use std::sync::Arc;

use async_trait::async_trait;

use core_agent_tool::ToolManager;

use crate::domain::{LoadedSkill, SkillDescriptor, SkillFrontmatter, SkillResult};
use crate::error::SkillError;

/// A resolved skill with verified tool references and structured instructions.
#[derive(Debug, Clone)]
pub struct ResolvedSkill {
    pub descriptor: SkillDescriptor,
    pub instructions: String,
    pub steps: Vec<String>,
    pub tools: Vec<String>,
    pub examples: Vec<String>,
}

/// The output of a skill execution preparation.
#[derive(Debug, Clone)]
pub struct SkillOutput {
    pub skill_name: String,
    pub resolved: ResolvedSkill,
    pub prompt: String,
}

/// SkillExecutor trait — resolves and prepares skills for execution.
#[async_trait]
pub trait SkillExecutor: Send + Sync {
    /// Resolve a skill: parse frontmatter, validate tool dependencies,
    /// extract instructions, and return a validated ResolvedSkill.
    async fn resolve(
        &self,
        skill: &LoadedSkill,
        tool_manager: &ToolManager,
    ) -> SkillResult<ResolvedSkill>;

    /// Build a structured Agent prompt from a resolved skill.
    fn build_prompt(&self, resolved: &ResolvedSkill, goal: &str) -> String;
}

/// Default implementation of SkillExecutor.
pub struct DefaultSkillExecutor {
    max_instructions_bytes: usize,
}

impl DefaultSkillExecutor {
    /// Create a new DefaultSkillExecutor with the default instruction limit.
    pub fn new() -> Self {
        Self {
            max_instructions_bytes: 64 * 1024,
        }
    }

    /// Create a new DefaultSkillExecutor with a custom instruction limit.
    pub fn with_max_instructions(max_bytes: usize) -> Self {
        Self {
            max_instructions_bytes: max_bytes,
        }
    }
}

impl Default for DefaultSkillExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SkillExecutor for DefaultSkillExecutor {
    async fn resolve(
        &self,
        skill: &LoadedSkill,
        tool_manager: &ToolManager,
    ) -> SkillResult<ResolvedSkill> {
        // Parse the frontmatter from the loaded skill content
        let frontmatter = parse_frontmatter_from_content(&skill.content)?;

        // Validate that all referenced tools exist in the ToolManager
        for tool_key in &frontmatter.tools {
            let exists = tool_manager
                .find(tool_key)
                .await
                .map_err(|e| SkillError::Validation(format!("tool lookup failed: {e}")))?;
            if exists.is_none() {
                return Err(SkillError::ToolNotFound(tool_key.clone()));
            }
        }

        // Extract instructions (body after frontmatter) and enforce size limit
        let instructions = extract_instructions(&skill.content, self.max_instructions_bytes)?;

        Ok(ResolvedSkill {
            descriptor: skill.descriptor.clone(),
            instructions,
            steps: frontmatter.steps,
            tools: frontmatter.tools,
            examples: frontmatter.examples,
        })
    }

    fn build_prompt(&self, resolved: &ResolvedSkill, goal: &str) -> String {
        let mut prompt = String::new();

        prompt.push_str(&format!(
            "# Skill: {}\n\n{}\n\n",
            resolved.descriptor.name, resolved.descriptor.description
        ));

        prompt.push_str(&format!("## Goal\n\n{}\n\n", goal));

        if !resolved.instructions.is_empty() {
            prompt.push_str("## Instructions\n\n");
            prompt.push_str(&resolved.instructions);
            prompt.push('\n');
        }

        if !resolved.steps.is_empty() {
            prompt.push_str("\n## Steps\n\n");
            for (i, step) in resolved.steps.iter().enumerate() {
                prompt.push_str(&format!("{}. {}\n", i + 1, step));
            }
        }

        if !resolved.tools.is_empty() {
            prompt.push_str("\n## Available Tools\n\n");
            for tool in &resolved.tools {
                prompt.push_str(&format!("- `{}`\n", tool));
            }
        }

        if !resolved.examples.is_empty() {
            prompt.push_str("\n## Examples\n\n");
            for example in &resolved.examples {
                prompt.push_str(&format!("- {}\n", example));
            }
        }

        prompt
    }
}

// ── Internal helpers ──

fn parse_frontmatter_from_content(content: &str) -> SkillResult<SkillFrontmatter> {
    let mut lines = content.lines();
    if lines.next().map(str::trim) != Some("---") {
        return Err(SkillError::Validation(
            "SKILL.md content must start with YAML frontmatter".into(),
        ));
    }
    let mut frontmatter_lines = Vec::new();
    let mut closed = false;
    for line in lines.by_ref() {
        let trimmed = line.trim();
        if trimmed == "---" {
            closed = true;
            break;
        }
        frontmatter_lines.push(trimmed);
    }
    if !closed {
        return Err(SkillError::Validation(
            "SKILL.md frontmatter is not closed".into(),
        ));
    }
    let yaml_content = frontmatter_lines.join("\n");
    let frontmatter: SkillFrontmatter = serde_yaml::from_str(&yaml_content)?;
    Ok(frontmatter)
}

fn extract_instructions(content: &str, max_bytes: usize) -> SkillResult<String> {
    // Find the body after the closing `---`
    let mut after_frontmatter = false;
    let mut found_close = false;
    let mut instructions = String::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if !found_close {
            if trimmed == "---" {
                if after_frontmatter {
                    found_close = true;
                } else {
                    after_frontmatter = true;
                }
            }
            continue;
        }
        // After closing `---` delimiter
        if !instructions.is_empty() {
            instructions.push('\n');
        }
        instructions.push_str(line);
    }

    if instructions.len() > max_bytes {
        instructions.truncate(max_bytes);
        instructions.push_str("\n\n[instruction truncated by size limit]");
    }

    Ok(instructions.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use chrono::Utc;
    use uuid::Uuid;
    use core_agent_tool::{
        ToolManager, ToolManagerBuilder, BuiltinToolProvider, ToolDefinition,
        ToolProviderDefinition, ToolProviderKind,
    };

    fn create_skill_content(name: &str, description: &str, tools: &[&str]) -> String {
        let tools_yaml = if tools.is_empty() {
            String::new()
        } else {
            let tools_list = tools
                .iter()
                .map(|t| format!("  - \"{}\"", t))
                .collect::<Vec<_>>()
                .join("\n");
            format!("tools:\n{}\n", tools_list)
        };
        format!(
            "---\nname: {name}\ndescription: {description}\n{tools_yaml}\
             steps:\n  - \"Step one\"\n  - \"Step two\"\nexamples:\n  - \"Example usage\"\n---\n\n\
             # Instructions\n\nDo the following steps in order.\n\n1. First do this\n2. Then do that\n"
        )
    }

    fn create_loaded_skill(name: &str, description: &str, tools: &[&str]) -> LoadedSkill {
        let content = create_skill_content(name, description, tools);
        let now = Utc::now();
        LoadedSkill {
            descriptor: crate::domain::SkillDescriptor {
                id: Uuid::new_v4(),
                name: name.into(),
                description: description.into(),
                scope: crate::domain::SkillScope::Project,
                path: std::path::PathBuf::from(name),
                precedence: 100,
                content_sha256: format!("{:x}", sha2::Sha256::digest(content.as_bytes())),
                bytes: content.len(),
                tool_count: tools.len(),
                created_at: now,
                updated_at: now,
            },
            content,
        }
    }

    fn build_tool_manager() -> ToolManager {
        let provider = BuiltinToolProvider::new();
        let manager = ToolManagerBuilder::default().build();
        // We need to load the builtin provider into the manager
        // Use tokio runtime directly
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            manager.load_provider(&provider).await.unwrap();
        });
        manager
    }

    #[tokio::test]
    async fn executor_resolves_valid_skill() {
        let skill = create_loaded_skill("test-skill", "A test skill", &["builtin/file.read@1.0.0"]);
        let tool_manager = build_tool_manager();
        let executor = DefaultSkillExecutor::new();

        let resolved = executor.resolve(&skill, &tool_manager).await.unwrap();

        assert_eq!(resolved.descriptor.name, "test-skill");
        assert_eq!(resolved.tools.len(), 1);
        assert_eq!(resolved.tools[0], "builtin/file.read@1.0.0");
        assert!(resolved.instructions.contains("Do the following steps"));
        assert_eq!(resolved.steps.len(), 2);
        assert_eq!(resolved.examples.len(), 1);
    }

    #[tokio::test]
    async fn executor_rejects_missing_tool() {
        let skill = create_loaded_skill(
            "broken-skill", "A skill with missing tools",
            &["builtin/nonexistent.tool@1.0.0"],
        );
        let tool_manager = build_tool_manager();
        let executor = DefaultSkillExecutor::new();

        let result = executor.resolve(&skill, &tool_manager).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            SkillError::ToolNotFound(key) => assert_eq!(key, "builtin/nonexistent.tool@1.0.0"),
            other => panic!("expected ToolNotFound, got: {other}"),
        }
    }

    #[tokio::test]
    async fn executor_generates_prompt() {
        let skill = create_loaded_skill("review-skill", "Code review skill", &["builtin/file.read@1.0.0"]);
        let tool_manager = build_tool_manager();
        let executor = DefaultSkillExecutor::new();

        let resolved = executor.resolve(&skill, &tool_manager).await.unwrap();
        let prompt = executor.build_prompt(&resolved, "Review the main.rs file");

        assert!(prompt.contains("review-skill"));
        assert!(prompt.contains("Code review skill"));
        assert!(prompt.contains("Review the main.rs file"));
        assert!(prompt.contains("Instructions"));
        assert!(prompt.contains("Steps"));
        assert!(prompt.contains("Available Tools"));
        assert!(prompt.contains("builtin/file.read@1.0.0"));
    }

    #[tokio::test]
    async fn executor_resolves_skill_with_no_tools() {
        let skill = create_loaded_skill("instruction-only", "No tools needed", &[]);
        let tool_manager = build_tool_manager();
        let executor = DefaultSkillExecutor::new();

        let resolved = executor.resolve(&skill, &tool_manager).await.unwrap();
        assert!(resolved.tools.is_empty());
        assert!(resolved.instructions.contains("Do the following steps"));
    }

    #[tokio::test]
    async fn executor_rejects_invalid_frontmatter() {
        let content = "No frontmatter here\n\nJust some text";
        let now = Utc::now();
        let skill = LoadedSkill {
            descriptor: crate::domain::SkillDescriptor {
                id: Uuid::new_v4(),
                name: "bad".into(),
                description: "bad".into(),
                scope: crate::domain::SkillScope::Project,
                path: std::path::PathBuf::from("bad"),
                precedence: 100,
                content_sha256: format!("{:x}", sha2::Sha256::digest(content.as_bytes())),
                bytes: content.len(),
                tool_count: 0,
                created_at: now,
                updated_at: now,
            },
            content: content.into(),
        };
        let tool_manager = build_tool_manager();
        let executor = DefaultSkillExecutor::new();

        let result = executor.resolve(&skill, &tool_manager).await;
        assert!(result.is_err());
    }
}