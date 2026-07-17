//! 内置 Provider 实现
//!
//! MVP 实现 4 个 Provider：
//! - SystemProvider：系统提示
//! - ConversationProvider：消息历史
//! - EnvironmentProvider：环境信息
//! - UserProvider：用户输入

pub mod system_provider;
pub mod conversation_provider;
pub mod environment_provider;
pub mod user_provider;

pub use system_provider::SystemProvider;
pub use conversation_provider::ConversationProvider;
pub use environment_provider::EnvironmentProvider;
pub use user_provider::UserProvider;