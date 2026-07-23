//! OpenAPI Platform — external system integration via REST API.
//!
//! Provides the API Gateway abstraction for external systems to call agents:
//!
//! - **ApiKey** — API key management with scopes, quotas, and expiration
//! - **RateLimit** — rate limiting model
//! - **Gateway trait** — authentication, authorization, and routing abstraction
//! - **OpenApiManager** — orchestrates API key lifecycle and request authentication
//!
//! This module is HTTP-framework-agnostic. To expose as a REST server,
//! implement the `Gateway` trait with your preferred framework (axum, actix, etc.).

mod domain;
mod error;
mod infrastructure;
mod manager;

pub use domain::*;
pub use error::{OpenApiError, OpenApiResult};
pub use infrastructure::*;
pub use manager::{InMemoryApiKeyStore, NoopRateLimiter, OpenApiManager};

pub type OpenApiRuntime = OpenApiManager;