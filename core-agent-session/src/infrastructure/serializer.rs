//! Session 序列化扩展点。

use crate::domain::Session;
use crate::error::{SessionError, SessionResult};

/// Session 序列化格式接口。
pub trait SessionSerializer: Send + Sync {
    fn serialize(&self, session: &Session) -> SessionResult<Vec<u8>>;
    fn deserialize(&self, data: &[u8]) -> SessionResult<Session>;
}

/// 默认 JSON 序列化实现。
#[derive(Debug, Default)]
pub struct JsonSessionSerializer;

impl SessionSerializer for JsonSessionSerializer {
    fn serialize(&self, session: &Session) -> SessionResult<Vec<u8>> {
        serde_json::to_vec(session).map_err(|error| SessionError::Serialization(error.to_string()))
    }

    fn deserialize(&self, data: &[u8]) -> SessionResult<Session> {
        serde_json::from_slice(data).map_err(|error| SessionError::Serialization(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_serializer_round_trips_session() {
        let serializer = JsonSessionSerializer;
        let session = Session::new("Round trip");

        let encoded = serializer.serialize(&session).unwrap();
        let decoded = serializer.deserialize(&encoded).unwrap();

        assert_eq!(decoded.id, session.id);
        assert_eq!(decoded.title, session.title);
        assert_eq!(decoded.state, session.state);
    }
}
