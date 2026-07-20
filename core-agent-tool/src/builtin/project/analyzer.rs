use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `project.analyzer` — Analyze project structure and identify framework.
///
/// Scans the project root directory, identifies build system and framework,
/// and outputs a structured project map.
pub struct ProjectAnalyzerTool;

#[async_trait]
impl Tool for ProjectAnalyzerTool {
    fn key(&self) -> &str {
        "builtin/project.analyzer@1.0.0"
    }

    async fn execute(
        &self,
        request: &ToolRequest,
        _context: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput> {
        let path = request.parameters["path"]
            .as_str()
            .filter(|p| !p.is_empty())
            .unwrap_or(".");

        let project_dir = std::path::Path::new(path);
        if !project_dir.is_dir() {
            return Err(ToolError::InvalidArgument(format!("{path} is not a directory")));
        }

        let mut result = String::new();
        result.push_str(&format!("Project Analysis: {}\n\n", project_dir.canonicalize().unwrap_or_else(|_| project_dir.to_path_buf()).display()));

        // Detect build system
        result.push_str("Build System:\n");
        let mut build_detected = false;
        if project_dir.join("Cargo.toml").exists() {
            result.push_str("  Type: Rust (Cargo)\n");
            if let Ok(content) = tokio::fs::read_to_string(project_dir.join("Cargo.toml")).await {
                for line in content.lines() {
                    if line.trim().starts_with("name") {
                        result.push_str(&format!("  Package: {}\n", line.split('=').nth(1).unwrap_or("?").trim().trim_matches('"')));
                        break;
                    }
                }
                // Check if workspace
                if content.contains("[workspace]") {
                    result.push_str("  Layout: Workspace\n");
                } else {
                    result.push_str("  Layout: Single Package\n");
                }
            }
            build_detected = true;
        }
        if project_dir.join("pom.xml").exists() {
            result.push_str("  Type: Java (Maven)\n");
            if let Ok(content) = tokio::fs::read_to_string(project_dir.join("pom.xml")).await {
                if let Some(artifact) = content.lines()
                    .find(|l| l.trim().starts_with("<artifactId>"))
                    .map(|l| l.trim().trim_start_matches("<artifactId>").trim_end_matches("</artifactId>").trim())
                {
                    result.push_str(&format!("  Artifact: {artifact}\n"));
                }
            }
            build_detected = true;
        }
        if project_dir.join("build.gradle").exists() || project_dir.join("build.gradle.kts").exists() {
            result.push_str("  Type: Java/Kotlin (Gradle)\n");
            build_detected = true;
        }
        if project_dir.join("package.json").exists() {
            result.push_str("  Type: Node.js (npm)\n");
            if let Ok(content) = tokio::fs::read_to_string(project_dir.join("package.json")).await {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(name) = json.get("name").and_then(|n| n.as_str()) {
                        result.push_str(&format!("  Package: {name}\n"));
                    }
                }
            }
            build_detected = true;
        }
        if project_dir.join("setup.py").exists() || project_dir.join("pyproject.toml").exists() {
            result.push_str("  Type: Python\n");
            build_detected = true;
        }
        if !build_detected {
            result.push_str("  Unknown build system\n");
        }

        // Detect framework
        result.push_str("\nFramework Detection:\n");
        let mut framework_detected = false;

        // Scan for common framework signatures
        let mut file_count = 0u64;
        let mut dir_count = 0u64;
        let mut walk = tokio::fs::read_dir(project_dir).await
            .map_err(|e| ToolError::execution("project.analyzer", format!("failed to read directory: {e}"), false))?;

        while let Ok(Some(entry)) = walk.next_entry().await {
            if entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
                dir_count += 1;
            } else {
                file_count += 1;
            }
        }

        // Check for common framework indicators
        if project_dir.join("src/main/java").exists() || project_dir.join("src/main/kotlin").exists() {
            if project_dir.join("src/main/resources").exists() {
                result.push_str("  Spring Boot / Java Standard\n");
                framework_detected = true;
            }
        }
        if project_dir.join("src/main.rs").exists() || project_dir.join("src/lib.rs").exists() {
            if let Ok(content) = std::fs::read_to_string(project_dir.join("Cargo.toml")) {
                if content.contains("actix-web") || content.contains("axum") || content.contains("rocket") || content.contains("warp") {
                    result.push_str("  Rust Web Framework (actix/axum/rocket/warp)\n");
                    framework_detected = true;
                }
            }
        }
        if project_dir.join("app.py").exists() || project_dir.join("manage.py").exists() {
            result.push_str("  Python Web Framework (Django/Flask)\n");
            framework_detected = true;
        }
        if !framework_detected {
            result.push_str("  Generic / Unknown\n");
        }

        // Directory structure
        result.push_str(&format!("\nStructure:\n  {} files, {} directories (top-level)\n", file_count, dir_count));

        // Source directories
        let src_dirs = ["src", "src/main/java", "src/main/kotlin", "src/main/resources", "app", "lib", "components", "pages", "api"];
        result.push_str("\nSource Directories:\n");
        for dir_name in &src_dirs {
            let dir_path = project_dir.join(dir_name);
            if dir_path.is_dir() {
                let count = count_files(&dir_path).await;
                result.push_str(&format!("  {dir_name}/ ({count} files)\n"));
            }
        }

        Ok(RawToolOutput::text(result))
    }
}

async fn count_files(dir: &std::path::Path) -> u64 {
    let mut count = 0u64;
    let walk_pattern = format!("{}/**/*", dir.display());
    if let Ok(glob_iter) = glob::glob(&walk_pattern) {
        for entry in glob_iter.flatten() {
            if entry.is_file() {
                count += 1;
            }
        }
    }
    count
}

pub fn project_analyzer_tool() -> Arc<dyn Tool> {
    Arc::new(ProjectAnalyzerTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn detects_rust_project() {
        let dir = tempdir().unwrap();
        tokio::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test-project\"\nversion = \"0.1.0\"\n"
        ).await.unwrap();
        tokio::fs::create_dir_all(dir.path().join("src")).await.unwrap();
        tokio::fs::write(dir.path().join("src/main.rs"), "fn main() {}").await.unwrap();

        let tool = ProjectAnalyzerTool;
        let request = ToolRequest::new(
            "builtin/project.analyzer@1.0.0",
            serde_json::json!({"path": dir.path().to_string_lossy()}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("Cargo"));
        assert!(text.contains("test-project"));
    }
}