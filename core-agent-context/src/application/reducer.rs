//! SummaryReducer — 兼容名称下的确定性 Last-N Reducer
//!
//! MVP 实现的核心 Reducer 策略：
//! 1. 保留所有 required segments（不可裁剪）
//! 2. 对 Conversation Slot：保留最近 N 条完整消息，并继续按 Token 预算保留最新消息
//! 3. 对其他 Slot：按优先级从高到低裁剪，确保不超 Token 预算
//! 4. 仅当显式开启 `enable_summary` 时生成兼容性的提取式摘要

use async_trait::async_trait;
use std::cmp::Reverse;
use std::collections::HashMap;

use crate::domain::context::{ContextSegment, ContextSource};
use crate::domain::slot::{ContextSlot, TokenCounter};
use crate::error::{ContextError, ContextResult};
use crate::infrastructure::{ContextReducer, ReducerConfig};

/// SummaryReducer
pub struct SummaryReducer;

impl SummaryReducer {
    /// 保留旧构造函数签名；运行配置由 Pipeline 在每次请求时传入。
    pub fn new(_config: ReducerConfig) -> Self {
        Self
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
        let source_tokens = segments.iter().fold(0_u64, |total, segment| {
            total.saturating_add(segment.token_count)
        });
        let trigger_tokens = config
            .max_total_tokens
            .saturating_mul(u64::from(config.trigger_percent))
            / 100;
        let compression_triggered = config.max_total_tokens > 0 && source_tokens >= trigger_tokens;
        let mut grouped: HashMap<ContextSlot, Vec<ContextSegment>> = HashMap::new();
        for segment in segments {
            grouped.entry(segment.slot).or_default().push(segment);
        }

        let mut result = Vec::new();
        let mut total_tokens = 0u64;
        let mut slot_tokens = HashMap::<ContextSlot, u64>::new();
        let mut slot_order: Vec<_> = ContextSlot::ORDERED
            .into_iter()
            .filter(|slot| grouped.contains_key(slot))
            .collect();
        slot_order.sort_by_key(|slot| {
            Reverse(
                grouped
                    .get(slot)
                    .and_then(|segments| segments.iter().map(|segment| segment.priority).max())
                    .unwrap_or_else(|| slot.default_priority()),
            )
        });

        // Required 内容先统一计入，确保 optional 内容不会挤占其预算。
        for slot in &slot_order {
            let Some(slot_segments) = grouped.get_mut(slot) else {
                continue;
            };
            let mut index = 0;
            while index < slot_segments.len() {
                if slot_segments[index].required {
                    let segment = slot_segments.remove(index);
                    require_capacity(&segment, config, &mut total_tokens, &mut slot_tokens)?;
                    result.push(segment);
                } else {
                    index += 1;
                }
            }
        }

        // Optional 内容按稳定 Slot 优先级裁剪。
        for slot in slot_order {
            let Some(mut optional) = grouped.remove(&slot) else {
                continue;
            };
            if optional.is_empty() {
                continue;
            }

            if slot == ContextSlot::Conversation {
                optional.sort_by_key(conversation_position);
                let keep_recent = if compression_triggered {
                    config.keep_recent_messages
                } else {
                    usize::MAX
                };
                let (recent, mut older) = split_conversation(&optional, keep_recent);
                let mut accepted = Vec::new();

                // 从最新消息开始选择，最后再恢复时间正序。
                for segment in recent.into_iter().rev() {
                    if accept_if_fits(&segment, config, &mut total_tokens, &mut slot_tokens) {
                        accepted.push(segment);
                    } else {
                        older.push(segment);
                    }
                }
                accepted.reverse();

                if compression_triggered && config.enable_summary && !older.is_empty() {
                    let summary = summarize_older_messages(&older)?;
                    if accept_if_fits(&summary, config, &mut total_tokens, &mut slot_tokens) {
                        result.push(summary);
                    }
                }
                result.extend(accepted);
                continue;
            }

            optional.sort_by_key(|segment| Reverse(segment.priority));
            for segment in optional {
                if accept_if_fits(&segment, config, &mut total_tokens, &mut slot_tokens) {
                    result.push(segment);
                }
            }
        }

        Ok(result)
    }
}

fn require_capacity(
    segment: &ContextSegment,
    config: &ReducerConfig,
    total_tokens: &mut u64,
    slot_tokens: &mut HashMap<ContextSlot, u64>,
) -> ContextResult<()> {
    if !accept_if_fits(segment, config, total_tokens, slot_tokens) {
        return Err(ContextError::TokenBudgetExceeded(format!(
            "required {} slot needs {} tokens (global limit {}, slot limit {})",
            segment.slot.as_str(),
            segment.token_count,
            config.max_total_tokens,
            config.slot_budgets.get(&segment.slot).copied().unwrap_or(0)
        )));
    }
    Ok(())
}

fn accept_if_fits(
    segment: &ContextSegment,
    config: &ReducerConfig,
    total_tokens: &mut u64,
    slot_tokens: &mut HashMap<ContextSlot, u64>,
) -> bool {
    let current_slot_tokens = slot_tokens.get(&segment.slot).copied().unwrap_or(0);
    let Some(next_total) = total_tokens.checked_add(segment.token_count) else {
        return false;
    };
    let Some(next_slot) = current_slot_tokens.checked_add(segment.token_count) else {
        return false;
    };
    let slot_budget = config.slot_budgets.get(&segment.slot).copied().unwrap_or(0);

    if config.max_total_tokens > 0 && next_total > config.max_total_tokens {
        return false;
    }
    if slot_budget > 0 && next_slot > slot_budget {
        return false;
    }

    *total_tokens = next_total;
    slot_tokens.insert(segment.slot, next_slot);
    true
}

/// 将 Conversation segments 分为"最近 N 条"和"旧消息"
fn split_conversation(
    segments: &[ContextSegment],
    keep_recent: usize,
) -> (Vec<ContextSegment>, Vec<ContextSegment>) {
    if segments.is_empty() || keep_recent == 0 {
        return (Vec::new(), segments.to_vec());
    }

    // 优先使用 Provider 给出的稳定消息位置；兼容旧 Provider 的 priority。
    let mut sorted = segments.to_vec();
    sorted.sort_by_key(conversation_position);

    let split_at = sorted.len().saturating_sub(keep_recent);
    let older = sorted[..split_at].to_vec();
    let recent = sorted[split_at..].to_vec();

    (recent, older)
}

fn conversation_position(segment: &ContextSegment) -> i64 {
    segment
        .metadata
        .get("message_index")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(i64::from(segment.priority))
}

/// 将旧消息压缩为一条摘要
fn summarize_older_messages(older: &[ContextSegment]) -> ContextResult<ContextSegment> {
    let mut summary_parts = Vec::new();
    let mut summary_characters = 0_usize;

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
            let excerpt = content_text.chars().take(160).collect::<String>();
            let part = format!("[{role}]: {excerpt}");
            let characters = part.chars().count();
            if summary_characters.saturating_add(characters) > 4_000 {
                break;
            }
            summary_characters = summary_characters.saturating_add(characters);
            summary_parts.push(part);
        }
    }

    let summary_text = format!(
        "[Summary of earlier conversation ({} messages)]\n[{} bounded excerpts]\n{}",
        older.len(),
        summary_parts.len(),
        summary_parts.join("\n")
    );

    let token_count = TokenCounter::estimate(&summary_text);

    Ok(ContextSegment::new(
        ContextSource::Conversation,
        ContextSlot::Conversation,
        serde_json::Value::String(summary_text),
        token_count,
        ContextSlot::Conversation.default_priority() - 1, // 比正常消息优先级稍低
    )
    .with_meta("reduced", "true")
    .with_meta("original_message_count", older.len().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::DefaultComposer;
    use crate::infrastructure::ContextComposer;

    fn make_conv_segment(priority: i32, content: &str) -> ContextSegment {
        let content_json = serde_json::json!({
            "role": "USER",
            "content": content,
        });
        ContextSegment::new(
            ContextSource::Conversation,
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
        assert!(!summary.required);
        assert_eq!(summary.source, ContextSource::Conversation);
        assert_eq!(summary.metadata.get("original_message_count").unwrap(), "5");
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

        let reducer = SummaryReducer;
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
            max_total_tokens: 1_000,
            keep_recent_messages: 5,
            enable_summary: true,
            trigger_percent: 1,
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

    #[tokio::test]
    async fn compression_waits_until_the_configured_threshold() {
        let segments: Vec<ContextSegment> = (0..3)
            .map(|i| make_conv_segment(i, &format!("message {i}")))
            .collect();
        let config = ReducerConfig {
            max_total_tokens: 1_000,
            keep_recent_messages: 1,
            enable_summary: true,
            trigger_percent: 80,
            ..ReducerConfig::default()
        };

        let result = SummaryReducer.reduce(segments, &config).await.unwrap();

        assert_eq!(result.len(), 3);
        assert!(result
            .iter()
            .all(|segment| segment.metadata.get("reduced").is_none()));
    }

    #[tokio::test]
    async fn test_reducer_keeps_newest_messages_within_budget() {
        let segments: Vec<ContextSegment> = (0..5)
            .map(|i| make_conv_segment(i, &format!("m{}", i)))
            .collect();
        let config = ReducerConfig {
            max_total_tokens: 2,
            keep_recent_messages: 5,
            enable_summary: false,
            ..ReducerConfig::default()
        };

        let result = SummaryReducer.reduce(segments, &config).await.unwrap();

        let contents: Vec<_> = result
            .iter()
            .filter_map(|segment| segment.content.get("content")?.as_str())
            .collect();
        assert_eq!(contents, vec!["m3", "m4"]);
    }

    #[tokio::test]
    async fn test_required_content_over_budget_is_explicit_error() {
        let segment = ContextSegment::new(
            ContextSource::System,
            ContextSlot::System,
            serde_json::Value::String("required".into()),
            10,
            100,
        )
        .required();
        let config = ReducerConfig {
            max_total_tokens: 5,
            ..ReducerConfig::default()
        };

        let error = SummaryReducer
            .reduce(vec![segment], &config)
            .await
            .unwrap_err();

        assert!(matches!(error, ContextError::TokenBudgetExceeded(_)));
    }

    #[tokio::test]
    async fn test_conversation_total_survives_when_messages_are_trimmed() {
        let metadata = ContextSegment::new(
            ContextSource::Conversation,
            ContextSlot::Conversation,
            serde_json::Value::Null,
            0,
            60,
        )
        .required()
        .with_meta("conversation_meta", "true")
        .with_meta("conversation_total", "5")
        .with_meta("message_index", "-1");
        let message = ContextSegment::new(
            ContextSource::Conversation,
            ContextSlot::Conversation,
            serde_json::json!({
                "id": uuid::Uuid::new_v4().to_string(),
                "role": "USER",
                "content": "too large",
                "created_at": chrono::Utc::now().to_rfc3339(),
            }),
            10,
            60,
        )
        .with_meta("message_id", uuid::Uuid::new_v4().to_string())
        .with_meta("message_index", "4")
        .with_meta("conversation_total", "5");
        let config = ReducerConfig {
            max_total_tokens: 1,
            keep_recent_messages: 5,
            ..ReducerConfig::default()
        };

        let reduced = SummaryReducer
            .reduce(vec![metadata, message], &config)
            .await
            .unwrap();
        let context = DefaultComposer::new()
            .compose(uuid::Uuid::new_v4(), None, reduced)
            .await
            .unwrap();

        assert!(context.conversation.messages.is_empty());
        assert_eq!(context.conversation.total_count, 5);
    }
}
