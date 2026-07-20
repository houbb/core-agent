//! ContextReference — 上下文注解/引用领域模型
//!
//! 用户可以通过选中代码、文件范围、历史消息等方式创建上下文引用，
//! 作为 Context 的补充输入，告诉 Agent "看这里"。
//!
//! 设计文档: design-docs/037-context-comment.md
//!
//! # 引用类型
//!
//! - File: 文件路径 + 行范围
//! - Selection: 选中的文本内容
//! - Message: 历史会话中的某条消息

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// 引用类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum ReferenceType {
    /// 文件引用（路径 + 行范围）
    File,
    /// 选择文本引用
    Selection,
    /// 历史消息引用
    Message,
}

impl ReferenceType {
    /// 返回稳定的大写类型名
    pub fn as_str(&self) -> &'static str {
        match self {
            ReferenceType::File => "FILE",
            ReferenceType::Selection => "SELECTION",
            ReferenceType::Message => "MESSAGE",
        }
    }
}

/// 引用定位器 — 精确描述引用来源
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReferenceLocator {
    /// 文件引用定位器
    File {
        /// 文件路径（相对于工作区或绝对路径）
        path: String,
        /// 起始行号（可选，1-indexed）
        start_line: Option<usize>,
        /// 结束行号（可选，1-indexed，包含）
        end_line: Option<usize>,
    },
    /// 选择文本引用定位器
    Selection {
        /// 选中的文本内容
        content: String,
        /// 来源文件路径（可选）
        source_path: Option<String>,
        /// 起始行号（可选）
        start_line: Option<usize>,
        /// 结束行号（可选）
        end_line: Option<usize>,
    },
    /// 历史消息引用定位器
    Message {
        /// 所属 Session ID
        session_id: Uuid,
        /// 所属 Conversation ID
        conversation_id: Uuid,
        /// 消息 ID
        message_id: Uuid,
    },
}

/// ContextReference — 上下文引用
///
/// 代表用户主动标记的一条上下文注解。
/// 每个引用携带类型、定位器、快照和元数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextReference {
    /// 引用唯一 ID
    pub id: Uuid,
    /// 引用类型
    pub reference_type: ReferenceType,
    /// 定位器
    pub locator: ReferenceLocator,
    /// 创建时的文本快照（可选，用于 Selection 和 File 的缓存内容）
    pub snapshot: Option<String>,
    /// 附加元数据
    pub metadata: HashMap<String, String>,
    /// 创建时间
    pub created_at: DateTime<Utc>,
}

impl ContextReference {
    /// 创建新的文件引用
    pub fn file(path: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            reference_type: ReferenceType::File,
            locator: ReferenceLocator::File {
                path: path.into(),
                start_line: None,
                end_line: None,
            },
            snapshot: None,
            metadata: HashMap::new(),
            created_at: Utc::now(),
        }
    }

    /// 创建新的选择文本引用
    pub fn selection(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            reference_type: ReferenceType::Selection,
            locator: ReferenceLocator::Selection {
                content: content.into(),
                source_path: None,
                start_line: None,
                end_line: None,
            },
            snapshot: None,
            metadata: HashMap::new(),
            created_at: Utc::now(),
        }
    }

    /// 创建新的消息引用
    pub fn message(session_id: Uuid, conversation_id: Uuid, message_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            reference_type: ReferenceType::Message,
            locator: ReferenceLocator::Message {
                session_id,
                conversation_id,
                message_id,
            },
            snapshot: None,
            metadata: HashMap::new(),
            created_at: Utc::now(),
        }
    }

    /// 设置文件行范围
    pub fn with_line_range(mut self, start: usize, end: usize) -> Self {
        if let ReferenceLocator::File { ref mut start_line, ref mut end_line, .. } = self.locator {
            *start_line = Some(start);
            *end_line = Some(end);
        }
        self
    }

    /// 设置选择来源路径
    pub fn with_source_path(mut self, path: impl Into<String>) -> Self {
        if let ReferenceLocator::Selection { ref mut source_path, .. } = self.locator {
            *source_path = Some(path.into());
        }
        self
    }

    /// 设置快照
    pub fn with_snapshot(mut self, snapshot: impl Into<String>) -> Self {
        self.snapshot = Some(snapshot.into());
        self
    }

    /// 添加元数据
    pub fn with_meta(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// ContextPackage — 上下文包
///
/// 用户问题 + 引用列表的聚合体，作为 Context 构建的输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPackage {
    /// 用户问题
    pub user_question: String,
    /// 引用列表
    pub references: Vec<ContextReference>,
    /// 元数据
    pub metadata: HashMap<String, String>,
}

impl ContextPackage {
    /// 创建新的上下文包
    pub fn new(user_question: impl Into<String>) -> Self {
        Self {
            user_question: user_question.into(),
            references: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// 添加引用
    pub fn add_reference(mut self, reference: ContextReference) -> Self {
        self.references.push(reference);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reference_type_as_str() {
        assert_eq!(ReferenceType::File.as_str(), "FILE");
        assert_eq!(ReferenceType::Selection.as_str(), "SELECTION");
        assert_eq!(ReferenceType::Message.as_str(), "MESSAGE");
    }

    #[test]
    fn test_file_reference_creation() {
        let r = ContextReference::file("src/main.rs")
            .with_line_range(10, 30)
            .with_meta("author", "test");
        assert_eq!(r.reference_type, ReferenceType::File);
        assert!(matches!(r.locator, ReferenceLocator::File { .. }));
        if let ReferenceLocator::File { ref path, start_line, end_line, .. } = r.locator {
            assert_eq!(path, "src/main.rs");
            assert_eq!(start_line, Some(10));
            assert_eq!(end_line, Some(30));
        }
        assert_eq!(r.metadata.get("author").unwrap(), "test");
    }

    #[test]
    fn test_selection_reference_creation() {
        let r = ContextReference::selection("selected text")
            .with_source_path("src/main.rs")
            .with_snapshot("snapshot content");
        assert_eq!(r.reference_type, ReferenceType::Selection);
        if let ReferenceLocator::Selection { ref content, ref source_path, .. } = r.locator {
            assert_eq!(content, "selected text");
            assert_eq!(source_path.as_deref(), Some("src/main.rs"));
        }
        assert_eq!(r.snapshot.as_deref(), Some("snapshot content"));
    }

    #[test]
    fn test_message_reference_creation() {
        let sid = Uuid::new_v4();
        let cid = Uuid::new_v4();
        let mid = Uuid::new_v4();
        let r = ContextReference::message(sid, cid, mid);
        assert_eq!(r.reference_type, ReferenceType::Message);
        if let ReferenceLocator::Message { session_id, conversation_id, message_id } = r.locator {
            assert_eq!(session_id, sid);
            assert_eq!(conversation_id, cid);
            assert_eq!(message_id, mid);
        }
    }

    #[test]
    fn test_context_package() {
        let pkg = ContextPackage::new("分析这段代码")
            .add_reference(ContextReference::file("src/main.rs").with_line_range(1, 50));
        assert_eq!(pkg.user_question, "分析这段代码");
        assert_eq!(pkg.references.len(), 1);
    }

    #[test]
    fn test_context_reference_serialization() {
        let r = ContextReference::file("test.rs").with_line_range(1, 10);
        let json = serde_json::to_string(&r).unwrap();
        let restored: ContextReference = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.reference_type, ReferenceType::File);
        if let ReferenceLocator::File { path, start_line, end_line, .. } = restored.locator {
            assert_eq!(path, "test.rs");
            assert_eq!(start_line, Some(1));
            assert_eq!(end_line, Some(10));
        }
    }
}