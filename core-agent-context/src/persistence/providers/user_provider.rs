//! UserProvider — 用户输入上下文提供者
//!
//! 将当前用户输入转换为 ContextSegment。

use async_trait::async_trait;

use crate::domain::context::{ContextSegment, ContextSource};
use crate::domain::context_reference::ReferenceLocator;
use crate::domain::slot::{ContextSlot, TokenCounter};
use crate::error::{ContextError, ContextResult};
use crate::infrastructure::{ContextProvider, ProviderContext};

/// UserProvider
///
/// 从 ProviderContext 的 extensions 中读取当前用户输入。
pub struct UserProvider;

impl UserProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for UserProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ContextProvider for UserProvider {
    fn name(&self) -> &str {
        "user-provider"
    }

    fn source(&self) -> ContextSource {
        ContextSource::User
    }

    fn slot(&self) -> ContextSlot {
        ContextSlot::User
    }

    async fn collect(&self, ctx: &ProviderContext) -> ContextResult<Vec<ContextSegment>> {
        // 从 extensions 中读取 user_input
        let user_input = match ctx.extensions.get("user_input") {
            Some(val) => val.as_str().ok_or_else(|| {
                ContextError::InvalidArgument("user_input extension must be a string".into())
            })?,
            None => return Ok(Vec::new()),
        };

        if user_input.is_empty() {
            return Ok(Vec::new());
        }

        let token_count = TokenCounter::estimate(user_input);
        let segment = ContextSegment::new(
            ContextSource::User,
            ContextSlot::User,
            serde_json::Value::String(user_input.to_owned()),
            token_count,
            ContextSlot::User.default_priority(),
        )
        .required(); // 用户输入不可裁剪

        let mut segments = vec![segment];
        // 添加文件引用和选择引用
        let ref_segments = collect_reference_segments(ctx)?;
        segments.extend(ref_segments);
        Ok(segments)
    }
}

/// 从 ProviderContext 的 references 中解析 File 和 Selection 引用，
/// 生成对应的 ContextSegment。
fn collect_reference_segments(ctx: &ProviderContext) -> ContextResult<Vec<ContextSegment>> {
    let mut segments = Vec::new();

    for reference in &ctx.references {
        match &reference.locator {
            ReferenceLocator::File { path, start_line, end_line } => {
                // 尝试读取文件内容
                let content = read_file_range(ctx, path, *start_line, *end_line)?;
                let token_count = TokenCounter::estimate(&content);
                let ref_content = serde_json::json!({
                    "path": path,
                    "start_line": start_line,
                    "end_line": end_line,
                    "content": content,
                    "reference_id": reference.id.to_string(),
                });
                let segment = ContextSegment::new(
                    ContextSource::Reference,
                    ContextSlot::Reference,
                    ref_content,
                    token_count,
                    ContextSlot::Reference.default_priority(),
                )
                .required()
                .with_meta("reference_id", reference.id.to_string())
                .with_meta("reference_type", "file")
                .with_meta("path", path.clone());
                segments.push(segment);
            }
            ReferenceLocator::Selection { content, source_path, .. } => {
                let token_count = TokenCounter::estimate(content);
                let ref_content = serde_json::json!({
                    "content": content,
                    "source_path": source_path,
                    "reference_id": reference.id.to_string(),
                });
                let segment = ContextSegment::new(
                    ContextSource::Reference,
                    ContextSlot::Reference,
                    ref_content,
                    token_count,
                    ContextSlot::Reference.default_priority(),
                )
                .required()
                .with_meta("reference_id", reference.id.to_string())
                .with_meta("reference_type", "selection");
                segments.push(segment);
            }
            _ => {} // Message 类型由 ConversationProvider 处理
        }
    }

    Ok(segments)
}

/// 读取文件指定行范围的内容
fn read_file_range(ctx: &ProviderContext, path: &str, start_line: Option<usize>, end_line: Option<usize>) -> ContextResult<String> {
    // 确定文件路径
    let base_dir = ctx.working_directory.as_deref().unwrap_or(".");
    let full_path = std::path::Path::new(base_dir).join(path);

    let content = std::fs::read_to_string(&full_path)
        .map_err(|e| ContextError::InvalidArgument(format!("Cannot read file {}: {}", full_path.display(), e)))?;

    // 如果指定了行范围，只提取指定行
    match (start_line, end_line) {
        (Some(start), Some(end)) if start <= end && start > 0 => {
            let lines: Vec<&str> = content.lines().collect();
            if start > lines.len() {
                return Err(ContextError::InvalidArgument(format!(
                    "Start line {} exceeds file length {} for {}",
                    start, lines.len(), path
                )));
            }
            let end = end.min(lines.len());
            let selected = lines[(start - 1)..end].join("\n");
            Ok(selected)
        }
        (Some(start), None) if start > 0 => {
            let lines: Vec<&str> = content.lines().collect();
            if start > lines.len() {
                return Err(ContextError::InvalidArgument(format!(
                    "Start line {} exceeds file length {} for {}",
                    start, lines.len(), path
                )));
            }
            let selected = lines[(start - 1)..].join("\n");
            Ok(selected)
        }
        _ => Ok(content), // 没有行范围，返回全部内容
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_agent_session::SqliteSessionStore;
    use std::collections::HashMap;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_user_provider_collect() {
        let store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
        let session_id = uuid::Uuid::new_v4();

        let mut extensions = HashMap::new();
        extensions.insert(
            "user_input".to_string(),
            serde_json::Value::String("Hello, agent!".to_string()),
        );

        let ctx = ProviderContext {
            session_id,
            conversation_id: None,
            session_store: store,
            system_prompt: None,
            working_directory: None,
            max_messages: None,
            extensions,
            references: Vec::new(),
        };

        let provider = UserProvider::new();
        let segments = provider.collect(&ctx).await.unwrap();

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].source, ContextSource::User);
        assert_eq!(segments[0].slot, ContextSlot::User);
        assert!(segments[0].required);
    }

    #[tokio::test]
    async fn test_user_provider_empty_when_no_input() {
        let store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
        let session_id = uuid::Uuid::new_v4();
        let ctx = ProviderContext::new(session_id, store);

        let provider = UserProvider::new();
        let segments = provider.collect(&ctx).await.unwrap();

        assert!(segments.is_empty());
    }

    #[tokio::test]
    async fn test_user_provider_rejects_non_string_input() {
        let store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
        let session_id = uuid::Uuid::new_v4();
        let mut context = ProviderContext::new(session_id, store);
        context
            .extensions
            .insert("user_input".into(), serde_json::json!({"unexpected": true}));

        assert!(matches!(
            UserProvider::new().collect(&context).await.unwrap_err(),
            ContextError::InvalidArgument(_)
        ));
    }
}
