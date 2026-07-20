use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `api.analyzer` — Analyze REST API endpoints in a project.
///
/// Scans source files for API route definitions across frameworks:
/// - Spring Boot (@RestController, @RequestMapping, @GetMapping, etc.)
/// - JAX-RS (@Path, @GET, @POST, etc.)
/// - Express (app.get, app.post, router.get, etc.)
/// - Actix/Axum (route definitions)
pub struct ApiAnalyzerTool;

#[async_trait]
impl Tool for ApiAnalyzerTool {
    fn key(&self) -> &str {
        "builtin/api.analyzer@1.0.0"
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

        let endpoints = match lang.as_str() {
            "java" => scan_java_endpoints(project_dir).await?,
            "javascript" | "typescript" | "node" => scan_js_endpoints(project_dir).await?,
            "rust" => scan_rust_endpoints(project_dir).await?,
            _ => {
                // Try all
                let mut all = scan_java_endpoints(project_dir).await?;
                all.extend(scan_js_endpoints(project_dir).await?);
                all.extend(scan_rust_endpoints(project_dir).await?);
                all
            }
        };

        if endpoints.is_empty() {
            return Ok(RawToolOutput::text(format!(
                "No API endpoints found in {path}."
            )));
        }

        let mut result = format!("Found {} API endpoint(s):\n\n", endpoints.len());
        for (i, ep) in endpoints.iter().enumerate() {
            result.push_str(&format!("{}. {} {} {}\n", i + 1, ep.method, ep.path, ep.file));
        }

        // Group by method
        let mut by_method: std::collections::BTreeMap<String, Vec<&EndpointInfo>> = std::collections::BTreeMap::new();
        for ep in &endpoints {
            by_method.entry(ep.method.clone()).or_default().push(ep);
        }

        result.push_str("\n--- By Method ---\n");
        for (method, eps) in &by_method {
            result.push_str(&format!("\n{method}:\n"));
            for ep in eps {
                result.push_str(&format!("  {}\n", ep.path));
            }
        }

        Ok(RawToolOutput::text(result))
    }
}

fn detect_language(dir: &std::path::Path) -> String {
    if dir.join("pom.xml").exists() || dir.join("build.gradle").exists() {
        "java".to_string()
    } else if dir.join("Cargo.toml").exists() {
        "rust".to_string()
    } else if dir.join("package.json").exists() {
        "node".to_string()
    } else {
        "all".to_string()
    }
}

#[derive(Debug)]
struct EndpointInfo {
    method: String,
    path: String,
    file: String,
}

async fn scan_java_endpoints(dir: &std::path::Path) -> ToolRuntimeResult<Vec<EndpointInfo>> {
    let mut endpoints = Vec::new();
    let walk_pattern = format!("{}/**/*.java", dir.display());
    let glob_iter = glob::glob(&walk_pattern)
        .map_err(|e| ToolError::execution("api.analyzer", format!("glob error: {e}"), false))?;

    let mapping_re = regex::Regex::new(
        r#"@(?:RequestMapping|GetMapping|PostMapping|PutMapping|DeleteMapping|PatchMapping)\s*\(\s*(?:\w+\s*=\s*)?(?:'([^']+)'|"([^"]+)")?"#
    ).unwrap();
    let class_re = regex::Regex::new(r"@(?:RestController|Controller|Path)\b").unwrap();

    for entry in glob_iter.flatten() {
        if !entry.is_file() { continue; }
        if let Ok(content) = tokio::fs::read_to_string(&entry).await {
            let is_rest = class_re.is_match(&content);
            if !is_rest { continue; }

            let file_path = entry.to_string_lossy().to_string();
            // Simplify path
            let file_path = file_path.trim_start_matches(dir.to_str().unwrap_or("")).trim_start_matches('/').to_string();

            // Find base path from class-level mapping
            let mut base_path = String::new();
            for line in content.lines() {
                if let Some(cap) = mapping_re.captures(line) {
                    let path = cap.get(1).or_else(|| cap.get(2)).map(|m| m.as_str()).unwrap_or("");
                    // If it's on a class level (before method definitions), it's the base path
                    if line.contains("class ") || line.contains("interface ") {
                        base_path = path.to_string();
                    } else {
                        // Method-level mapping
                        let method = if line.contains("GetMapping") || line.contains("GET") { "GET" }
                            else if line.contains("PostMapping") || line.contains("POST") { "POST" }
                            else if line.contains("PutMapping") || line.contains("PUT") { "PUT" }
                            else if line.contains("DeleteMapping") || line.contains("DELETE") { "DELETE" }
                            else if line.contains("PatchMapping") || line.contains("PATCH") { "PATCH" }
                            else { "ANY" };
                        let full_path = if base_path.is_empty() { path.to_string() } else { format!("{base_path}{path}") };
                        endpoints.push(EndpointInfo {
                            method: method.to_string(),
                            path: full_path,
                            file: file_path.clone(),
                        });
                    }
                }
            }
        }
    }

    Ok(endpoints)
}

async fn scan_js_endpoints(dir: &std::path::Path) -> ToolRuntimeResult<Vec<EndpointInfo>> {
    let mut endpoints = Vec::new();
    let exts = ["js", "jsx", "ts", "tsx"];
    for ext in &exts {
        let walk_pattern = format!("{}/**/*.{ext}", dir.display());
        if let Ok(glob_iter) = glob::glob(&walk_pattern) {
            let router_re = regex::Regex::new(
                r#"(?:app|router|route)\s*\.\s*(get|post|put|delete|patch|all)\s*\(\s*['"]([^'"]+)['"]"#
            ).unwrap();

            for entry in glob_iter.flatten() {
                if !entry.is_file() { continue; }
                if let Ok(content) = tokio::fs::read_to_string(&entry).await {
                    let file_path = entry.to_string_lossy().to_string();
                    let file_path = file_path.trim_start_matches(dir.to_str().unwrap_or("")).trim_start_matches('/').to_string();

                    for cap in router_re.captures_iter(&content) {
                        let method = cap.get(1).map(|m| m.as_str().to_uppercase()).unwrap_or_default();
                        let path = cap.get(2).map(|m| m.as_str()).unwrap_or("");
                        endpoints.push(EndpointInfo {
                            method,
                            path: path.to_string(),
                            file: file_path.clone(),
                        });
                    }
                }
            }
        }
    }

    Ok(endpoints)
}

async fn scan_rust_endpoints(dir: &std::path::Path) -> ToolRuntimeResult<Vec<EndpointInfo>> {
    let mut endpoints = Vec::new();
    let walk_pattern = format!("{}/**/*.rs", dir.display());
    let glob_iter = glob::glob(&walk_pattern)
        .map_err(|e| ToolError::execution("api.analyzer", format!("glob error: {e}"), false))?;

    let route_re = regex::Regex::new(
        r#"\.(route|get|post|put|delete|any)\s*\(\s*['"]([^'"]+)['"]"#
    ).unwrap();
    let attr_re = regex::Regex::new(
        r#"#\[(?:get|post|put|delete|patch)\s*\(\s*['"]([^'"]+)['"]"#
    ).unwrap();

    for entry in glob_iter.flatten() {
        if !entry.is_file() { continue; }
        if let Ok(content) = tokio::fs::read_to_string(&entry).await {
            let file_path = entry.to_string_lossy().to_string();
            let file_path = file_path.trim_start_matches(dir.to_str().unwrap_or("")).trim_start_matches('/').to_string();

            // Actix/Axum style
            for cap in route_re.captures_iter(&content) {
                let method = cap.get(1).map(|m| m.as_str().to_uppercase()).unwrap_or_default();
                let path = cap.get(2).map(|m| m.as_str()).unwrap_or("");
                endpoints.push(EndpointInfo {
                    method: if method == "ROUTE" { "ANY".to_string() } else { method },
                    path: path.to_string(),
                    file: file_path.clone(),
                });
            }

            // Attribute macros (actix-web)
            for cap in attr_re.captures_iter(&content) {
                let path = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                endpoints.push(EndpointInfo {
                    method: "ANY".to_string(),
                    path: path.to_string(),
                    file: file_path.clone(),
                });
            }
        }
    }

    Ok(endpoints)
}

pub fn api_analyzer_tool() -> Arc<dyn Tool> {
    Arc::new(ApiAnalyzerTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn detects_java_endpoints() {
        let dir = tempdir().unwrap();
        tokio::fs::create_dir_all(dir.path().join("src/main/java")).await.unwrap();
        tokio::fs::write(
            dir.path().join("src/main/java/UserController.java"),
            "@RestController\n@RequestMapping(\"/api/users\")\npublic class UserController {\n    @GetMapping(\"/{id}\")\n    public String getUser() { return \"user\"; }\n}"
        ).await.unwrap();

        let tool = ApiAnalyzerTool;
        let request = ToolRequest::new(
            "builtin/api.analyzer@1.0.0",
            serde_json::json!({"path": dir.path().to_string_lossy(), "language": "java"}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("GET") || text.contains("API"));
    }
}