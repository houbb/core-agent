use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `decompiler.decompile` — Decompile Java class files or JAR archives.
///
/// Uses `javap` (bundled with JDK) to disassemble class files.
/// For JAR files, lists the contents and allows decompiling specific classes.
pub struct DecompilerDecompileTool;

#[async_trait]
impl Tool for DecompilerDecompileTool {
    fn key(&self) -> &str {
        "builtin/decompiler.decompile@1.0.0"
    }

    async fn execute(
        &self,
        request: &ToolRequest,
        _context: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput> {
        let path = request.parameters["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("path is required".into()))?;
        if path.is_empty() {
            return Err(ToolError::InvalidArgument("path must not be empty".into()));
        }

        let class_name = request.parameters["class"].as_str();
        let verbose = request.parameters["verbose"].as_bool().unwrap_or(false);

        let file_path = std::path::Path::new(path);

        if !file_path.exists() {
            return Err(ToolError::execution("decompiler.decompile", format!("path not found: {path}"), false));
        }

        if file_path.extension().and_then(|e| e.to_str()) == Some("jar") {
            // JAR file: list contents or decompile specific class
            if let Some(class) = class_name {
                decompile_class_from_jar(file_path, class, verbose).await
            } else {
                list_jar_contents(file_path).await
            }
        } else if file_path.extension().and_then(|e| e.to_str()) == Some("class") {
            // Single class file
            decompile_class_file(file_path, verbose).await
        } else {
            Err(ToolError::InvalidArgument(format!(
                "Unsupported file type: {}. Expected .class or .jar file.",
                file_path.display()
            )))
        }
    }
}

async fn decompile_class_file(path: &std::path::Path, verbose: bool) -> ToolRuntimeResult<RawToolOutput> {
    let mut args = vec!["-c", "-p"];
    if verbose {
        args.push("-verbose");
    }
    args.push(path.to_str().unwrap_or(""));

    let output = tokio::process::Command::new("javap")
        .args(&args)
        .output()
        .await
        .map_err(|e| ToolError::execution(
            "decompiler.decompile",
            format!("failed to run javap: {e}. Is JDK installed?", ),
            true,
        ))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(RawToolOutput::text(format!(
            "Decompiled: {}\n\n{}",
            path.display(),
            stdout.trim_end(),
        )))
    } else {
        Err(ToolError::execution("decompiler.decompile", stderr, false))
    }
}

async fn decompile_class_from_jar(jar_path: &std::path::Path, class_name: &str, verbose: bool) -> ToolRuntimeResult<RawToolOutput> {
    let mut args = vec!["-c", "-p", "-classpath"];
    if verbose {
        args.push("-verbose");
    }
    args.push(jar_path.to_str().unwrap_or(""));
    args.push(class_name);

    let output = tokio::process::Command::new("javap")
        .args(&args)
        .output()
        .await
        .map_err(|e| ToolError::execution(
            "decompiler.decompile",
            format!("failed to run javap: {e}. Is JDK installed?"),
            true,
        ))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        let jar_display = jar_path.to_string_lossy();
        Ok(RawToolOutput::text(format!(
            "Decompiled {class_name} from {jar_display}:\n\n{}",
            stdout.trim_end(),
        )))
    } else {
        Err(ToolError::execution("decompiler.decompile", stderr, false))
    }
}

async fn list_jar_contents(jar_path: &std::path::Path) -> ToolRuntimeResult<RawToolOutput> {
    let output = tokio::process::Command::new("jar")
        .args(["tf", jar_path.to_str().unwrap_or("")])
        .output()
        .await
        .map_err(|e| ToolError::execution(
            "decompiler.decompile",
            format!("failed to run jar: {e}. Is JDK installed?"),
            true,
        ))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        let classes: Vec<&str> = stdout.lines()
            .filter(|l| l.ends_with(".class"))
            .collect();
        let _others: Vec<&str> = stdout.lines()
            .filter(|l| !l.ends_with(".class"))
            .collect();

        let mut result = format!("JAR Contents: {}\n\n", jar_path.display());
        result.push_str(&format!("Total entries: {}\n", stdout.lines().count()));
        result.push_str(&format!("Classes: {}\n\n", classes.len()));

        if !classes.is_empty() {
            result.push_str("Classes:\n");
            for c in classes.iter().take(50) {
                result.push_str(&format!("  {c}\n"));
            }
            if classes.len() > 50 {
                result.push_str(&format!("  ... and {} more\n", classes.len() - 50));
            }
        }

        Ok(RawToolOutput::text(result))
    } else {
        Err(ToolError::execution("decompiler.decompile", stderr, false))
    }
}

pub fn decompiler_decompile_tool() -> Arc<dyn Tool> {
    Arc::new(DecompilerDecompileTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;

    #[tokio::test]
    async fn rejects_empty_path() {
        let tool = DecompilerDecompileTool;
        let request = ToolRequest::new(
            "builtin/decompiler.decompile@1.0.0",
            serde_json::json!({"path": ""}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn rejects_missing_file() {
        let tool = DecompilerDecompileTool;
        let request = ToolRequest::new(
            "builtin/decompiler.decompile@1.0.0",
            serde_json::json!({"path": "/nonexistent/Test.class"}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn rejects_unsupported_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        tokio::fs::write(&path, "not a class file").await.unwrap();

        let tool = DecompilerDecompileTool;
        let request = ToolRequest::new(
            "builtin/decompiler.decompile@1.0.0",
            serde_json::json!({"path": path.to_string_lossy()}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await;
        assert!(result.is_err());
    }
}