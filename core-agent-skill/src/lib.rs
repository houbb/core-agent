//! Skill Runtime — discover, load, and execute reusable agent skill workflows.
//!
//! Skills are reusable, composable capabilities that combine multiple tools
//! into structured workflows. They are defined as SKILL.md files with YAML
//! frontmatter and are discovered from configured skill roots.
//!
//! Key concepts:
//! - SkillCatalog: discovers and indexes available skills from filesystem roots
//! - SkillDescriptor: metadata for a discovered skill (name, description, scope)
//! - LoadedSkill: a skill with its full instruction content loaded
//! - SkillExecutor: resolves a skill's tool dependencies and orchestrates execution

mod domain;
mod error;
mod executor;

pub use domain::*;
pub use error::{SkillError, SkillResult};
pub use executor::{
    DefaultSkillExecutor, ResolvedSkill, SkillExecutor, SkillOutput,
};

pub type SkillRuntime = SkillCatalog;