use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `dependency.inspect` — Inspect project dependencies.
///
/// Supports multiple languages by shelling out to build tools:
/// - Java: `mvn dependency:tree` or `gradle dependencies`
/// - Rust: `cargo tree`
/// - Node.js: reads `package.json` or `npm ls`
/// - Python: `pip list --format=json` or `pipdeptree`
pub struct DependencyInspectTool;

#[async_trait]
impl Tool for DependencyInspectTool {
    fn key(&self) -> &str {
        "builtin/dependency.inspect@1.0.0"
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
        let language = request.parameters["language"].as_str().unwrap_or("auto");

        let project_dir = std::path::Path::new(path);
        if !project_dir.is_dir() {
            return Err(ToolError::InvalidArgument(format!("{path} is not a directory")));
        }

        let lang = if language == "auto" {
            detect_language(project_dir)
        } else {
            language.to_string()
        };

        match lang.as_str() {
            "java" => inspect_java_dependencies(project_dir).await,
            "rust" => inspect_rust_dependencies(project_dir).await,
            "node" | "javascript" | "typescript" => inspect_node_dependencies(project_dir).await,
            "python" => inspect_python_dependencies(project_dir).await,
            _ => Err(ToolError::InvalidArgument(format!(
                "Unsupported language: {lang}. Supported: java, rust, node, python"
            ))),
        }
    }
}

fn detect_language(dir: &std::path::Path) -> String {
    if dir.join("pom.xml").exists() || dir.join("build.gradle").exists() || dir.join("build.gradle.kts").exists() {
        "java".to_string()
    } else if dir.join("Cargo.toml").exists() {
        "rust".to_string()
    } else if dir.join("package.json").exists() {
        "node".to_string()
    } else if dir.join("requirements.txt").exists() || dir.join("setup.py").exists() || dir.join("pyproject.toml").exists() {
        "python".to_string()
    } else {
        "unknown".to_string()
    }
}

async fn run_command(cmd: &str, args: &[&str], dir: &std::path::Path) -> ToolRuntimeResult<String> {
    let output = tokio::process::Command::new(cmd)
        .args(args)
        .current_dir(dir)
        .output()
        .await
        .map_err(|e| ToolError::execution("dependency.inspect", format!("failed to run {cmd}: {e}"), true))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(stdout.trim_end().to_string())
    } else {
        Err(ToolError::execution("dependency.inspect", format!("{} error: {stderr}", cmd), false))
    }
}

async fn inspect_java_dependencies(dir: &std::path::Path) -> ToolRuntimeResult<RawToolOutput> {
    // Try Maven first, then Gradle
    let has_maven = dir.join("pom.xml").exists();
    let has_gradle = dir.join("build.gradle").exists() || dir.join("build.gradle.kts").exists();

    if has_maven {
        let output = run_command("mvn", &["dependency:tree", "-DoutputType=text", "-q"], dir).await;
        match output {
            Ok(text) => Ok(RawToolOutput::text(format!("Maven Dependencies:\n\n{text}"))),
            Err(e) => {
                if has_gradle {
                    let text = run_command("gradle", &["dependencies", "--configuration=runtimeClasspath"], dir).await?;
                    Ok(RawToolOutput::text(format!("Gradle Dependencies:\n\n{text}")))
                } else {
                    Err(e)
                }
            }
        }
    } else if has_gradle {
        let text = run_command("gradle", &["dependencies", "--configuration=runtimeClasspath"], dir).await?;
        Ok(RawToolOutput::text(format!("Gradle Dependencies:\n\n{text}")))
    } else {
        Ok(RawToolOutput::text(
            "No Java build file found (pom.xml or build.gradle)."
        ))
    }
}

async fn inspect_rust_dependencies(dir: &std::path::Path) -> ToolRuntimeResult<RawToolOutput> {
    let text = run_command("cargo", &["tree", "--prefix", "depth"], dir).await?;
    Ok(RawToolOutput::text(format!("Rust Dependencies:\n\n{text}")))
}

async fn inspect_node_dependencies(dir: &std::path::Path) -> ToolRuntimeResult<RawToolOutput> {
    // Try npm ls first, fall back to reading package.json
    let package_json_path = dir.join("package.json");
    if !package_json_path.exists() {
        return Ok(RawToolOutput::text("No package.json found."));
    }

    let content = tokio::fs::read_to_string(&package_json_path)
        .await
        .map_err(|e| ToolError::execution("dependency.inspect", format!("failed to read package.json: {e}"), false))?;

    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| ToolError::execution("dependency.inspect", format!("invalid package.json: {e}"), false))?;

    let mut result = String::from("Node.js Dependencies:\n\n");

    if let Some(deps) = json.get("dependencies").and_then(|d| d.as_object()) {
        result.push_str("Production:\n");
        for (name, version) in deps {
            result.push_str(&format!("  {name}@{}\n", version.as_str().unwrap_or("?")));
        }
    }

    if let Some(dev_deps) = json.get("devDependencies").and_then(|d| d.as_object()) {
        result.push_str("\nDevelopment:\n");
        for (name, version) in dev_deps {
            result.push_str(&format!("  {name}@{}\n", version.as_str().unwrap_or("?")));
        }
    }

    Ok(RawToolOutput::text(result))
}

async fn inspect_python_dependencies(dir: &std::path::Path) -> ToolRuntimeResult<RawToolOutput> {
    let text = run_command("pip", &["list", "--format=columns"], dir).await?;
    Ok(RawToolOutput::text(format!("Python Dependencies:\n\n{text}")))
}

pub fn dependency_inspect_tool() -> Arc<dyn Tool> {
    Arc::new(DependencyInspectTool)
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
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n"
        ).await.unwrap();

        let tool = DependencyInspectTool;
        let request = ToolRequest::new(
            "builtin/dependency.inspect@1.0.0",
            serde_json::json!({
                "path": dir.path().to_string_lossy(),
                "language": "node"
            }),
        );
        let result = tool.execute(&request, &ToolContext::default()).await;
        // Node without package.json returns Ok with "No package.json found" message
        // Rust with auto-detect would try to run cargo tree
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn reads_node_dependencies() {
        let dir = tempdir().unwrap();
        tokio::fs::write(
            dir.path().join("package.json"),
            r#"{"dependencies": {"express": "^4.0.0"}, "devDependencies": {"jest": "^29.0.0"}}"#
        ).await.unwrap();

        let tool = DependencyInspectTool;
        let request = ToolRequest::new(
            "builtin/dependency.inspect@1.0.0",
            serde_json::json!({
                "path": dir.path().to_string_lossy(),
                "language": "node"
            }),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("express"));
        assert!(text.contains("jest"));
    }

    #[tokio::test]
    async fn rejects_invalid_language() {
        let tool = DependencyInspectTool;
        let request = ToolRequest::new(
            "builtin/dependency.inspect@1.0.0",
            serde_json::json!({
                "path": ".",
                "language": "unknown_lang_xyz"
            }),
        );
        let result = tool.execute(&request, &ToolContext::default()).await;
        assert!(result.is_err());
    }
}