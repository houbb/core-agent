//! ContextSerializer — Context 序列化扩展点。

use crate::domain::Context;
use crate::error::{ContextError, ContextResult};

/// 可插拔 Context 序列化器。
pub trait ContextSerializer: Send + Sync {
    /// 将 Context 编码为字节。
    fn serialize(&self, context: &Context) -> ContextResult<Vec<u8>>;
    /// 从字节恢复 Context。
    fn deserialize(&self, bytes: &[u8]) -> ContextResult<Context>;
}

/// 默认 JSON 序列化实现。
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonContextSerializer;

impl ContextSerializer for JsonContextSerializer {
    fn serialize(&self, context: &Context) -> ContextResult<Vec<u8>> {
        serde_json::to_vec(context).map_err(|error| ContextError::Serialization(error.to_string()))
    }

    fn deserialize(&self, bytes: &[u8]) -> ContextResult<Context> {
        serde_json::from_slice(bytes)
            .map_err(|error| ContextError::Serialization(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::DefaultComposer;
    use crate::infrastructure::ContextComposer;

    #[tokio::test]
    async fn json_serializer_round_trips_context() {
        let context = DefaultComposer::new()
            .compose(uuid::Uuid::new_v4(), None, Vec::new())
            .await
            .unwrap();
        let serializer = JsonContextSerializer;

        let restored = serializer
            .deserialize(&serializer.serialize(&context).unwrap())
            .unwrap();

        assert_eq!(restored.id, context.id);
        assert_eq!(restored.hash, context.hash);
    }
}
