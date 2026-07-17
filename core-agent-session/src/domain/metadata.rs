//! Metadata — 可扩展的元数据容器
//!
//! 不要以后不断加字段。统一用 Metadata。
//! 任何 Runtime 都可以扩展，而不用修改核心数据模型。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 元数据容器 — JSON 扩展点
///
/// 示例：
/// ```json
/// {
///   "language": "java",
///   "theme": "dark",
///   "temperature": 0.7,
///   "model": "gpt-5",
///   "tags": ["refactor", "urgent"]
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Metadata {
    #[serde(flatten)]
    inner: HashMap<String, serde_json::Value>,
}

impl Metadata {
    /// 创建空的 Metadata
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    /// 设置键值对
    pub fn set<K: Into<String>, V: Serialize>(&mut self, key: K, value: V) -> Result<(), serde_json::Error> {
        self.inner.insert(key.into(), serde_json::to_value(value)?);
        Ok(())
    }

    /// 获取值
    pub fn get<V: serde::de::DeserializeOwned>(&self, key: &str) -> Option<V> {
        self.inner
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// 移除键
    pub fn remove(&mut self, key: &str) -> Option<serde_json::Value> {
        self.inner.remove(key)
    }

    /// 检查键是否存在
    pub fn contains_key(&self, key: &str) -> bool {
        self.inner.contains_key(key)
    }

    /// 返回键值对数量
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// 获取所有键
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.inner.keys()
    }
}

impl std::fmt::Display for Metadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string(&self.inner).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_set_get() {
        let mut meta = Metadata::new();
        meta.set("language", "rust").unwrap();
        meta.set("temperature", 0.7).unwrap();

        assert_eq!(meta.get::<String>("language").unwrap(), "rust");
        assert_eq!(meta.get::<f64>("temperature").unwrap(), 0.7);
    }

    #[test]
    fn test_metadata_remove() {
        let mut meta = Metadata::new();
        meta.set("key", "value").unwrap();
        assert!(meta.contains_key("key"));

        meta.remove("key");
        assert!(!meta.contains_key("key"));
    }

    #[test]
    fn test_metadata_default_is_empty() {
        let meta = Metadata::default();
        assert!(meta.is_empty());
    }

    #[test]
    fn test_metadata_serialize_deserialize() {
        let mut meta = Metadata::new();
        meta.set("model", "gpt-5").unwrap();
        meta.set("tags", vec!["refactor", "urgent"]).unwrap();

        let json = serde_json::to_string(&meta).unwrap();
        let restored: Metadata = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.get::<String>("model").unwrap(), "gpt-5");
    }
}