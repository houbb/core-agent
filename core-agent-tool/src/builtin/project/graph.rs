use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `architecture.graph` — Generate architecture dependency graph in JSON format.
///
/// Analyzes module dependencies and produces a graph representation
/// suitable for visualization.
pub struct ArchitectureGraphTool;

#[async_trait]
impl Tool for ArchitectureGraphTool {
    fn key(&self) -> &str {
        "builtin/architecture.graph@1.0.0"
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
        let format = request.parameters["format"].as_str().unwrap_or("json");

        let project_dir = std::path::Path::new(path);
        if !project_dir.is_dir() {
            return Err(ToolError::InvalidArgument(format!("{path} is not a directory")));
        }

        // Detect language and build graph
        let mut nodes: Vec<serde_json::Value> = Vec::new();
        let mut edges: Vec<serde_json::Value> = Vec::new();

        if project_dir.join("Cargo.toml").exists() {
            build_rust_graph(project_dir, &mut nodes, &mut edges).await?;
        } else if project_dir.join("pom.xml").exists() {
            build_maven_graph(project_dir, &mut nodes, &mut edges).await?;
        } else if project_dir.join("package.json").exists() {
            build_node_graph(project_dir, &mut nodes, &mut edges).await?;
        } else {
            // Generic: scan top-level directories as modules
            let mut read_dir = tokio::fs::read_dir(project_dir).await
                .map_err(|e| ToolError::execution("architecture.graph", format!("{e}"), false))?;
            let mut module_names = Vec::new();
            while let Ok(Some(entry)) = read_dir.next_entry().await {
                if entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if !name.starts_with('.') && !name.starts_with("target") && !name.starts_with("node_modules") {
                        module_names.push(name.clone());
                        nodes.push(serde_json::json!({
                            "id": name,
                            "label": name,
                            "type": "module"
                        }));
                    }
                }
            }
            // Add top-level files as nodes
            let mut read_dir2 = tokio::fs::read_dir(project_dir).await
                .map_err(|e| ToolError::execution("architecture.graph", format!("{e}"), false))?;
            while let Ok(Some(entry)) = read_dir2.next_entry().await {
                if entry.file_type().await.map(|t| t.is_file()).unwrap_or(false) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.ends_with(".toml") || name.ends_with(".json") || name.ends_with(".yaml") || name.ends_with(".yml") {
                        nodes.push(serde_json::json!({
                            "id": name,
                            "label": name,
                            "type": "config"
                        }));
                    }
                }
            }
            // Connect root config files to modules
            for node in &nodes {
                if node["type"] == "config" {
                    for module in &module_names {
                        edges.push(serde_json::json!({
                            "source": node["id"],
                            "target": module,
                            "label": "configures"
                        }));
                    }
                }
            }
        }

        let graph = serde_json::json!({
            "nodes": nodes,
            "edges": edges,
            "metadata": {
                "project": project_dir.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(),
                "node_count": nodes.len(),
                "edge_count": edges.len()
            }
        });

        match format {
            "json" => Ok(RawToolOutput::json(graph)),
            "text" | "plain" => {
                let mut text = format!("Architecture Graph: {} nodes, {} edges\n\n", nodes.len(), edges.len());
                text.push_str("Nodes:\n");
                for node in &nodes {
                    text.push_str(&format!("  [{type}] {id}\n",
                        type = node["type"].as_str().unwrap_or("?"),
                        id = node["id"].as_str().unwrap_or("?")));
                }
                if !edges.is_empty() {
                    text.push_str("\nEdges:\n");
                    for edge in &edges {
                        text.push_str(&format!("  {} -> {}: {}\n",
                            edge["source"].as_str().unwrap_or("?"),
                            edge["target"].as_str().unwrap_or("?"),
                            edge["label"].as_str().unwrap_or("")));
                    }
                }
                Ok(RawToolOutput::text(text))
            }
            _ => Ok(RawToolOutput::json(graph)),
        }
    }
}

async fn build_rust_graph(
    dir: &std::path::Path,
    nodes: &mut Vec<serde_json::Value>,
    edges: &mut Vec<serde_json::Value>,
) -> ToolRuntimeResult<()> {
    // Add Cargo.toml as root
    nodes.push(serde_json::json!({"id": "Cargo.toml", "label": "Cargo.toml", "type": "config"}));

    // Check for workspace members
    let cargo_content = tokio::fs::read_to_string(dir.join("Cargo.toml")).await
        .map_err(|e| ToolError::execution("architecture.graph", format!("{e}"), false))?;

    if cargo_content.contains("[workspace]") {
        nodes.push(serde_json::json!({"id": "workspace", "label": "Workspace", "type": "module"}));
        edges.push(serde_json::json!({"source": "Cargo.toml", "target": "workspace", "label": "defines"}));

        // Find workspace members by scanning dirs with Cargo.toml
        if let Ok(entries) = tokio::fs::read_dir(dir).await {
            use tokio::fs::ReadDir;
            let mut entries: ReadDir = entries;
            while let Ok(Some(entry)) = entries.next_entry().await {
                if entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
                    let member_dir = entry.path();
                    if member_dir.join("Cargo.toml").exists() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        nodes.push(serde_json::json!({"id": name, "label": name, "type": "crate"}));
                        edges.push(serde_json::json!({"source": "workspace", "target": name, "label": "contains"}));
                    }
                }
            }
        }
    } else {
        // Single crate
        let bin_crates = ["src/main.rs", "src/lib.rs"];
        for bin in &bin_crates {
            if dir.join(bin).exists() {
                let name = bin.trim_start_matches("src/").trim_end_matches(".rs");
                nodes.push(serde_json::json!({"id": name, "label": name, "type": "crate"}));
                edges.push(serde_json::json!({"source": "Cargo.toml", "target": name, "label": "builds"}));
            }
        }
    }

    Ok(())
}

async fn build_maven_graph(
    dir: &std::path::Path,
    nodes: &mut Vec<serde_json::Value>,
    edges: &mut Vec<serde_json::Value>,
) -> ToolRuntimeResult<()> {
    nodes.push(serde_json::json!({"id": "pom.xml", "label": "pom.xml", "type": "config"}));

    // Check for modules
    let pom_content = tokio::fs::read_to_string(dir.join("pom.xml")).await
        .map_err(|e| ToolError::execution("architecture.graph", format!("{e}"), false))?;

    if pom_content.contains("<module>") {
        nodes.push(serde_json::json!({"id": "parent", "label": "Parent POM", "type": "module"}));
        edges.push(serde_json::json!({"source": "pom.xml", "target": "parent", "label": "defines"}));

        // Find modules
        if let Ok(entries) = tokio::fs::read_dir(dir).await {
            use tokio::fs::ReadDir;
            let mut entries: ReadDir = entries;
            while let Ok(Some(entry)) = entries.next_entry().await {
                if entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if entry.path().join("pom.xml").exists() {
                        nodes.push(serde_json::json!({"id": name, "label": name, "type": "module"}));
                        edges.push(serde_json::json!({"source": "parent", "target": name, "label": "contains"}));
                    }
                }
            }
        }
    }

    Ok(())
}

async fn build_node_graph(
    dir: &std::path::Path,
    nodes: &mut Vec<serde_json::Value>,
    edges: &mut Vec<serde_json::Value>,
) -> ToolRuntimeResult<()> {
    nodes.push(serde_json::json!({"id": "package.json", "label": "package.json", "type": "config"}));

    // Check for monorepo (workspaces)
    let pkg_content = tokio::fs::read_to_string(dir.join("package.json")).await
        .map_err(|e| ToolError::execution("architecture.graph", format!("{e}"), false))?;

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&pkg_content) {
        let has_workspaces = json.get("workspaces").is_some();

        let src_dirs = ["src", "app", "lib", "components", "pages", "api"];
        for src_dir in &src_dirs {
            if dir.join(src_dir).is_dir() {
                nodes.push(serde_json::json!({"id": src_dir, "label": src_dir, "type": "module"}));
                edges.push(serde_json::json!({"source": "package.json", "target": src_dir, "label": "contains"}));
            }
        }

        if has_workspaces {
            edges.push(serde_json::json!({"source": "package.json", "target": "workspaces", "label": "defines"}));
            nodes.push(serde_json::json!({"id": "workspaces", "label": "Workspaces", "type": "module"}));
        }
    }

    Ok(())
}

pub fn architecture_graph_tool() -> Arc<dyn Tool> {
    Arc::new(ArchitectureGraphTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn generates_rust_graph() {
        let dir = tempdir().unwrap();
        tokio::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\n"
        ).await.unwrap();
        tokio::fs::create_dir_all(dir.path().join("src")).await.unwrap();
        tokio::fs::write(dir.path().join("src/main.rs"), "fn main() {}").await.unwrap();

        let tool = ArchitectureGraphTool;
        let request = ToolRequest::new(
            "builtin/architecture.graph@1.0.0",
            serde_json::json!({"path": dir.path().to_string_lossy(), "format": "text"}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("Architecture Graph"));
    }
}