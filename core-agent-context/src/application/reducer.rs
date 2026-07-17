//! SummaryReducer — 摘要 + 保留最近 N 条
//!
//! MVP 实现的核心 Reducer 策略：
//! 1. 保留所有 required segments（不可裁剪）
//! 2. 对 Conversation Slot：保留最近 N 条完整消息，超出部分压缩为一条摘要
//! 3. 对其他 Slot：按优先级从低到高裁剪，确保不超 Token 预算

use async_trait::async_trait;
use std::collections::HashMap;

use crate::domain::context::{ContextSegment, ContextSource};
use crate::domain::slot::{ContextSlot, TokenCounter};
use crate::error::ContextResult;
use crate::infrastructure::{ContextReducer, ReducerConfig};

/// SummaryReducer
pub struct SummaryReducer {
    config: ReducerConfig,
}

impl SummaryReducer {
    pub fn new(config: ReducerConfig) -> Self {
        Self { config }
    }
}

impl Default for SummaryReducer {
    fn default() -> Self {
        Self::new(ReducerConfig::default())
    }
}

#[async_trait]
impl ContextReducer for SummaryReducer {
    fn name(&self) -> &str {
        "summary-reducer"
    }

    async fn reduce(
        &self,
        segments: Vec<ContextSegment>,
        config: &ReducerConfig,
    ) -> ContextResult<Vec<ContextSegment>> {
        let mut result = Vec::new();
        let mut total_tokens = 0u64;

        // Step 1: 保留所有 required segments（如 System Prompt、Environment、User Input）
        let (required, optional): (Vec<_>, Vec<_>) =
            segments.into_iter().partition(|s| s.required);

        for seg in required {
            total_tokens += seg.token_count;
            result.push(seg);
        }

        // Step 2: 按 Slot 分组 optional segments
        let grouped: HashMap<ContextSlot, Vec<ContextSegment>> =
            optional.into_iter().fold(HashMap::new(), |mut acc, seg| {
                acc.entry(seg.slot).or_default().push(seg);
                acc
            });

        // Step 3: 对 Conversation Slot 特殊处理
        if let Some(conv_segments) = grouped.get(&ContextSlot::Conversation) {
            let (recent, older) = split_conversation(conv_segments, config.keep_recent_messages);

            // 保留最近 N 条
            for seg in &recent {
                total_tokens += seg.token_count;
                result.push(seg.clone());
            }

            // 压缩旧消息为摘要
            if !older.is_empty() && config.enable_summary {
                let summary_segment = summarize_older_messages(&older)?;
                total_tokens += summary_segment.token_count;
                result.push(summary_segment);
            }
        }

        // Step 4: 对其他 Slot，按预算裁剪
        for (slot, segs) in grouped.iter() {
            if *slot == ContextSlot::Conversation {
                continue; // 已处理
            }

            let slot_budget = config.slot_budgets.get(slot).copied().unwrap_or(0);
            let mut slot_tokens = 0u64;

            // 按优先级降序排序
            let mut sorted_segs = segs.clone();
            sorted_segs.sort_by_key(|s| -s.priority);

            for seg in &sorted_segs {
                // 检查 Slot 预算
                if slot_budget > 0 && slot_tokens + seg.token_count > slot_budget {
                    continue;
                }
                // 检查全局预算
                if config.max_total_tokens > 0
                    && total_tokens + seg.token_count > config.max_total_tokens
                {
                    continue;
                }
                slot_tokens += seg.token_count;
                total_tokens += seg.token_count;
                result.push(seg.clone());
            }
        }

        Ok(result)
    }
}

/// 将 Conversation segments 分为"最近 N 条"和"旧消息"
fn split_conversation(
    segments: &[ContextSegment],
    keep_recent: usize,
) -> (Vec<ContextSegment>, Vec<ContextSegment>) {
    if segments.is_empty() || keep_recent == 0 {
        return (Vec::new(), segments.to_vec());
    }

    // 按 priority 排序（越大越新）
    let mut sorted = segments.to_vec();
    sorted.sort_by_key(|s| s.priority);

    let split_at = sorted.len().saturating_sub(keep_recent);
    let older = sorted[..split_at].to_vec();
    let recent = sorted[split_at..].to_vec();

    (recent, older)
}

/// 将旧消息压缩为一条摘要
fn summarize_older_messages(older: &[ContextSegment]) -> ContextResult<ContextSegment> {
    let mut summary_parts = Vec::new();

    for seg in older {
        let content_text = seg
            .content
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let role = seg
            .content
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("UNKNOWN");

        if !content_text.is_empty() {
            summary_parts.push(format!("[{}]: {}", role, content_text));
        }
    }

    let summary_text = format!(
        "[Summary of earlier conversation ({} messages)]\n{}",
        older.len(),
        summary_parts.join("\n")
    );

    let token_count = TokenCounter::estimate(&summary_text);

    Ok(ContextSegment::new(
        ContextSource::System,
        ContextSlot::Conversation,
        serde_json::Value::String(summary_text),
        token_count,
        ContextSlot::Conversation.default_priority() - 1, // 比正常消息优先级稍低
    )
    .required() // 摘要不可再被裁剪
    .with_meta("reduced", "true")
    .with_meta("original_message_count", older.len().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_conv_segment(priority: i32, content: &str) -> ContextSegment {
        let content_json = serde_json::json!({
            "role": "USER",
            "content": content,
        });
        ContextSegment::new(
            ContextSource::System,
            ContextSlot::Conversation,
            content_json,
            TokenCounter::estimate(content),
            priority,
        )
    }

    #[test]
    fn test_split_conversation_keep_recent() {
        let segments: Vec<ContextSegment> = (0..10)
            .map(|i| make_conv_segment(i, &format!("msg {}", i)))
            .collect();

        let (recent, older) = split_conversation(&segments, 3);
        assert_eq!(recent.len(), 3);
        assert_eq!(older.len(), 7);
    }

    #[test]
    fn test_split_conversation_keep_all() {
        let segments: Vec<ContextSegment> = (0..3)
            .map(|i| make_conv_segment(i, &format!("msg {}", i)))
            .collect();

        let (recent, older) = split_conversation(&segments, 10);
        assert_eq!(recent.len(), 3);
        assert_eq!(older.len(), 0);
    }

    #[test]
    fn test_split_conversation_empty() {
        let segments: Vec<ContextSegment> = Vec::new();
        let (recent, older) = split_conversation(&segments, 5);
        assert!(recent.is_empty());
        assert!(older.is_empty());
    }

    #[test]
    fn test_summarize_older_messages() {
        let older: Vec<ContextSegment> = (0..5)
            .map(|i| make_conv_segment(i, &format!("old msg {}", i)))
            .collect();

        let summary = summarize_older_messages(&older).unwrap();
        assert!(summary.required);
        assert_eq!(
            summary
                .metadata
                .get("original_message_count")
                .unwrap(),
            "5"
        );
        let content_str = summary.content.as_str().unwrap();
        assert!(content_str.contains("[Summary of earlier conversation"));
        assert!(content_str.contains("5 messages"));
    }

    #[tokio::test]
    async fn test_summary_reducer_preserves_required() {
        let required_seg = ContextSegment::new(
            ContextSource::System,
            ContextSlot::System,
            serde_json::Value::String("system prompt".into()),
            10,
            100,
        )
        .required();

        let optional_seg = make_conv_segment(0, "conversation msg");

        let reducer = SummaryReducer::default();
        let result = reducer
            .reduce(
                vec![required_seg.clone(), optional_seg],
                &ReducerConfig::default(),
            )
            .await
            .unwrap();

        // 必须保留 required segment
        assert!(result.iter().any(|s| s.required));
        assert_eq!(result[0].source, ContextSource::System);
    }

    #[tokio::test]
    async fn test_summary_reducer_caps_conversation() {
        let segments: Vec<ContextSegment> = (0..50)
            .map(|i| make_conv_segment(i, &format!("msg {}", i)))
            .collect();

        let config = ReducerConfig {
            keep_recent_messages: 5,
            enable_summary: true,
            ..ReducerConfig::default()
        };

        let reducer = SummaryReducer::new(config.clone());
        let result = reducer.reduce(segments, &config).await.unwrap();

        // 最近 5 条 + 1 条摘要
        let conv_count = result
            .iter()
            .filter(|s| s.slot == ContextSlot::Conversation)
            .count();
        assert_eq!(conv_count, 6); // 5 recent + 1 summary
    }
}