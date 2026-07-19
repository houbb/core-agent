use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, LOCATION, USER_AGENT};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::{
    FunctionTool, PermissionDecision, RawToolOutput, ToolContent, ToolDefinition, ToolError,
    ToolRegistration,
};

const MAX_QUERY_BYTES: usize = 4 * 1024;
const MAX_SEARCH_SOURCES: usize = 20;
const MAX_FETCH_BYTES: usize = 1024 * 1024;
const MAX_SEARCH_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSource {
    pub url: String,
    pub title: String,
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchRequest {
    pub query: String,
    pub allowed_domains: Vec<String>,
    pub max_results: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchResult {
    pub answer: String,
    pub sources: Vec<WebSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebFetchResult {
    pub url: String,
    pub content_type: String,
    pub content: String,
    pub bytes: usize,
    pub truncated: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum WebRuntimeError {
    #[error("web request is invalid: {0}")]
    Invalid(String),
    #[error("web request was denied: {0}")]
    Denied(String),
    #[error("web request was cancelled")]
    Cancelled,
    #[error("web provider failed: {0}")]
    Provider(String),
    #[error("web transport failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("web DNS resolution failed: {0}")]
    Dns(#[from] std::io::Error),
}

pub type WebRuntimeResult<T> = Result<T, WebRuntimeError>;

#[async_trait]
pub trait WebSearchProvider: Send + Sync {
    async fn search(
        &self,
        request: WebSearchRequest,
        cancellation: CancellationToken,
    ) -> WebRuntimeResult<WebSearchResult>;
}

#[derive(Debug, Clone, Default)]
pub struct WebDomainPolicy {
    allowed_domains: BTreeSet<String>,
    blocked_domains: BTreeSet<String>,
}

impl WebDomainPolicy {
    pub fn new(
        allowed_domains: impl IntoIterator<Item = String>,
        blocked_domains: impl IntoIterator<Item = String>,
    ) -> WebRuntimeResult<Self> {
        let allowed_domains = normalize_domains(allowed_domains)?;
        let mut blocked_domains = normalize_domains(blocked_domains)?;
        blocked_domains.extend(
            [
                "localhost",
                "metadata.google.internal",
                "metadata.azure.internal",
            ]
            .into_iter()
            .map(str::to_owned),
        );
        Ok(Self {
            allowed_domains,
            blocked_domains,
        })
    }

    pub fn unrestricted_public_web() -> Self {
        Self::new(Vec::<String>::new(), Vec::<String>::new())
            .expect("built-in domain policy must be valid")
    }

    fn permits(&self, domain: &str) -> bool {
        let domain = domain.trim_end_matches('.').to_ascii_lowercase();
        !self
            .blocked_domains
            .iter()
            .any(|blocked| domain_matches(&domain, blocked))
            && (self.allowed_domains.is_empty()
                || self
                    .allowed_domains
                    .iter()
                    .any(|allowed| domain_matches(&domain, allowed)))
    }

    fn validate_requested_domains(&self, domains: &[String]) -> WebRuntimeResult<Vec<String>> {
        let normalized = normalize_domains(domains.iter().cloned())?;
        for domain in &normalized {
            if !self.permits(domain) {
                return Err(WebRuntimeError::Denied(format!(
                    "domain is not allowed by policy: {domain}"
                )));
            }
        }
        Ok(normalized.into_iter().collect())
    }
}

pub struct OpenAiWebSearchProvider {
    api_key: String,
    model: String,
    endpoint: Url,
    client: reqwest::Client,
}

impl OpenAiWebSearchProvider {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> WebRuntimeResult<Self> {
        let api_key = api_key.into();
        let model = model.into();
        if api_key.trim().is_empty() || model.trim().is_empty() {
            return Err(WebRuntimeError::Invalid(
                "web search API key and model must not be empty".into(),
            ));
        }
        Ok(Self {
            api_key,
            model,
            endpoint: Url::parse("https://api.openai.com/v1/responses")
                .expect("static OpenAI endpoint must be valid"),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()?,
        })
    }

    pub fn with_endpoint(mut self, endpoint: Url) -> WebRuntimeResult<Self> {
        if endpoint.scheme() != "https" {
            return Err(WebRuntimeError::Invalid(
                "web search endpoint must use HTTPS".into(),
            ));
        }
        self.endpoint = endpoint;
        Ok(self)
    }

    pub fn from_environment() -> WebRuntimeResult<Option<Self>> {
        let Some(api_key) = std::env::var("CORE_AGENT_WEB_SEARCH_API_KEY")
            .ok()
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .filter(|value| !value.trim().is_empty())
        else {
            return Ok(None);
        };
        let model =
            std::env::var("CORE_AGENT_WEB_SEARCH_MODEL").unwrap_or_else(|_| "gpt-5-mini".into());
        let mut provider = Self::new(api_key, model)?;
        if let Ok(endpoint) = std::env::var("CORE_AGENT_WEB_SEARCH_ENDPOINT") {
            provider = provider.with_endpoint(
                Url::parse(&endpoint)
                    .map_err(|error| WebRuntimeError::Invalid(error.to_string()))?,
            )?;
        }
        Ok(Some(provider))
    }
}

#[async_trait]
impl WebSearchProvider for OpenAiWebSearchProvider {
    async fn search(
        &self,
        request: WebSearchRequest,
        cancellation: CancellationToken,
    ) -> WebRuntimeResult<WebSearchResult> {
        validate_search_request(&request)?;
        let mut web_tool = json!({"type": "web_search"});
        if !request.allowed_domains.is_empty() {
            web_tool["filters"] = json!({"allowed_domains": request.allowed_domains});
        }
        let payload = json!({
            "model": self.model,
            "input": request.query,
            "tools": [web_tool],
            "tool_choice": "auto",
            "include": ["web_search_call.action.sources"]
        });
        let response = tokio::select! {
            _ = cancellation.cancelled() => return Err(WebRuntimeError::Cancelled),
            response = self.client
                .post(self.endpoint.clone())
                .header(AUTHORIZATION, format!("Bearer {}", self.api_key))
                .header(CONTENT_TYPE, "application/json")
                .json(&payload)
                .send() => response?,
        };
        let status = response.status();
        let bytes =
            read_bounded_response(response, MAX_SEARCH_RESPONSE_BYTES, cancellation).await?;
        if !status.is_success() {
            return Err(WebRuntimeError::Provider(format!(
                "HTTP {status}: {}",
                truncate_chars(&String::from_utf8_lossy(&bytes), 2_000)
            )));
        }
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| WebRuntimeError::Provider(error.to_string()))?;
        parse_openai_search_response(&value, request.max_results)
    }
}

pub struct WebRuntime {
    provider: Arc<dyn WebSearchProvider>,
    policy: WebDomainPolicy,
}

impl WebRuntime {
    pub fn new(provider: Arc<dyn WebSearchProvider>, policy: WebDomainPolicy) -> Self {
        Self { provider, policy }
    }

    pub fn from_environment() -> WebRuntimeResult<Option<Self>> {
        Ok(
            OpenAiWebSearchProvider::from_environment()?.map(|provider| {
                Self::new(
                    Arc::new(provider),
                    WebDomainPolicy::unrestricted_public_web(),
                )
            }),
        )
    }

    pub async fn search(
        &self,
        mut request: WebSearchRequest,
        cancellation: CancellationToken,
    ) -> WebRuntimeResult<WebSearchResult> {
        validate_search_request(&request)?;
        request.allowed_domains = self
            .policy
            .validate_requested_domains(&request.allowed_domains)?;
        let mut result = self.provider.search(request.clone(), cancellation).await?;
        result.sources.retain(|source| {
            Url::parse(&source.url)
                .ok()
                .and_then(|url| url.host_str().map(str::to_owned))
                .is_some_and(|host| self.policy.permits(&host))
        });
        deduplicate_sources(&mut result.sources);
        result.sources.truncate(request.max_results);
        Ok(result)
    }

    pub async fn fetch(
        &self,
        url: &str,
        cancellation: CancellationToken,
    ) -> WebRuntimeResult<WebFetchResult> {
        let mut current = Url::parse(url)
            .map_err(|error| WebRuntimeError::Invalid(format!("invalid URL: {error}")))?;
        for redirect in 0..=MAX_REDIRECTS {
            let (host, address) = validate_and_resolve_url(&current, &self.policy).await?;
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(45))
                .redirect(reqwest::redirect::Policy::none())
                .resolve(&host, address)
                .build()?;
            let mut response = tokio::select! {
                _ = cancellation.cancelled() => return Err(WebRuntimeError::Cancelled),
                response = client
                    .get(current.clone())
                    .header(USER_AGENT, "core-agent-web/1.0")
                    .header(ACCEPT, "text/html, text/plain, application/json, application/xml;q=0.9")
                    .send() => response?,
            };
            if response.status().is_redirection() {
                if redirect == MAX_REDIRECTS {
                    return Err(WebRuntimeError::Denied(
                        "web fetch exceeded five redirects".into(),
                    ));
                }
                let location = response
                    .headers()
                    .get(LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .ok_or_else(|| {
                        WebRuntimeError::Provider("redirect did not include Location".into())
                    })?;
                current = current
                    .join(location)
                    .map_err(|error| WebRuntimeError::Invalid(error.to_string()))?;
                continue;
            }
            if !response.status().is_success() {
                return Err(WebRuntimeError::Provider(format!(
                    "HTTP {} from {}",
                    response.status(),
                    current
                )));
            }
            let content_type = response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("application/octet-stream")
                .split(';')
                .next()
                .unwrap_or("application/octet-stream")
                .trim()
                .to_ascii_lowercase();
            if !is_textual_content_type(&content_type) {
                return Err(WebRuntimeError::Denied(format!(
                    "unsupported web content type: {content_type}"
                )));
            }
            let mut bytes = Vec::new();
            let mut truncated = false;
            loop {
                let chunk = tokio::select! {
                    _ = cancellation.cancelled() => return Err(WebRuntimeError::Cancelled),
                    chunk = response.chunk() => chunk?,
                };
                let Some(chunk) = chunk else {
                    break;
                };
                let available = MAX_FETCH_BYTES.saturating_sub(bytes.len());
                let take = available.min(chunk.len());
                bytes.extend_from_slice(&chunk[..take]);
                if take < chunk.len() || bytes.len() == MAX_FETCH_BYTES {
                    truncated = true;
                    break;
                }
            }
            return Ok(WebFetchResult {
                url: current.to_string(),
                content_type,
                content: String::from_utf8_lossy(&bytes).into_owned(),
                bytes: bytes.len(),
                truncated,
            });
        }
        Err(WebRuntimeError::Provider(
            "web fetch redirect loop ended unexpectedly".into(),
        ))
    }
}

pub(crate) fn registrations(runtime: Arc<WebRuntime>) -> Vec<ToolRegistration> {
    let mut search_definition = ToolDefinition::new(
        "web",
        "web_search",
        "1.0.0",
        json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {"type": "string", "description": "Fresh-information search query"},
                "allowedDomains": {"type": "array", "items": {"type": "string"}, "maxItems": 20},
                "maxResults": {"type": "integer", "minimum": 1, "maximum": 20}
            },
            "additionalProperties": false
        }),
    );
    search_definition.description = "Search the public web for fresh information. Returns a synthesized answer plus canonical source URLs; cite those URLs in the response.".into();
    search_definition.category = "network.read".into();
    search_definition.default_permission = PermissionDecision::Ask;
    search_definition.timeout_ms = 65_000;
    let search_key = search_definition.key.clone();
    let search_runtime = runtime.clone();
    let search_tool = Arc::new(FunctionTool::new(search_key, move |request, context| {
        let runtime = search_runtime.clone();
        async move {
            let input: SearchToolInput = serde_json::from_value(request.parameters)
                .map_err(|error| ToolError::InvalidArgument(error.to_string()))?;
            let result = runtime
                .search(
                    WebSearchRequest {
                        query: input.query,
                        allowed_domains: input.allowed_domains,
                        max_results: input.max_results.unwrap_or(10).clamp(1, MAX_SEARCH_SOURCES),
                    },
                    context.cancellation,
                )
                .await
                .map_err(web_tool_error("web_search"))?;
            Ok(RawToolOutput {
                content: vec![ToolContent::Json(
                    serde_json::to_value(result)
                        .map_err(|error| ToolError::Serialization(error.to_string()))?,
                )],
                ..RawToolOutput::default()
            })
        }
    }));

    let mut fetch_definition = ToolDefinition::new(
        "web",
        "web_fetch",
        "1.0.0",
        json!({
            "type": "object",
            "required": ["url"],
            "properties": {
                "url": {"type": "string", "description": "Public HTTPS URL, normally selected from web_search sources"}
            },
            "additionalProperties": false
        }),
    );
    fetch_definition.description = "Fetch up to 1 MiB of public textual HTTPS content. Redirects and every resolved IP are revalidated to block SSRF and private-network access.".into();
    fetch_definition.category = "network.read".into();
    fetch_definition.default_permission = PermissionDecision::Ask;
    fetch_definition.timeout_ms = 50_000;
    let fetch_key = fetch_definition.key.clone();
    let fetch_tool = Arc::new(FunctionTool::new(fetch_key, move |request, context| {
        let runtime = runtime.clone();
        async move {
            let url = request
                .parameters
                .get("url")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::InvalidArgument("web_fetch url is required".into()))?;
            let result = runtime
                .fetch(url, context.cancellation)
                .await
                .map_err(web_tool_error("web_fetch"))?;
            Ok(RawToolOutput {
                content: vec![ToolContent::Json(
                    serde_json::to_value(result)
                        .map_err(|error| ToolError::Serialization(error.to_string()))?,
                )],
                ..RawToolOutput::default()
            })
        }
    }));

    vec![
        ToolRegistration::new(search_definition, search_tool),
        ToolRegistration::new(fetch_definition, fetch_tool),
    ]
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchToolInput {
    query: String,
    #[serde(default)]
    allowed_domains: Vec<String>,
    max_results: Option<usize>,
}

fn validate_search_request(request: &WebSearchRequest) -> WebRuntimeResult<()> {
    if request.query.trim().is_empty() || request.query.len() > MAX_QUERY_BYTES {
        return Err(WebRuntimeError::Invalid(
            "query must contain 1..=4096 bytes".into(),
        ));
    }
    if request.allowed_domains.len() > 20
        || request.max_results == 0
        || request.max_results > MAX_SEARCH_SOURCES
    {
        return Err(WebRuntimeError::Invalid(
            "allowedDomains/maxResults exceed supported bounds".into(),
        ));
    }
    Ok(())
}

fn parse_openai_search_response(
    value: &Value,
    max_results: usize,
) -> WebRuntimeResult<WebSearchResult> {
    let mut answer = String::new();
    let mut sources = Vec::new();
    for output in value
        .get("output")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if output.get("type").and_then(Value::as_str) == Some("message") {
            for content in output
                .get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if content.get("type").and_then(Value::as_str) == Some("output_text") {
                    if let Some(text) = content.get("text").and_then(Value::as_str) {
                        if !answer.is_empty() {
                            answer.push('\n');
                        }
                        answer.push_str(text);
                    }
                    for annotation in content
                        .get("annotations")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        if let Some(source) = source_from_value(annotation) {
                            sources.push(source);
                        }
                    }
                }
            }
        }
        for source in output
            .pointer("/action/sources")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(source) = source_from_value(source) {
                sources.push(source);
            }
        }
    }
    if answer.trim().is_empty() {
        return Err(WebRuntimeError::Provider(
            "web search response did not contain output_text".into(),
        ));
    }
    deduplicate_sources(&mut sources);
    sources.truncate(max_results);
    Ok(WebSearchResult { answer, sources })
}

fn source_from_value(value: &Value) -> Option<WebSource> {
    let url = truncate_chars(value.get("url")?.as_str()?, 4_096);
    let title = truncate_chars(
        value.get("title").and_then(Value::as_str).unwrap_or(&url),
        1_000,
    );
    let snippet = value
        .get("snippet")
        .or_else(|| value.get("text"))
        .and_then(Value::as_str)
        .map(|value| truncate_chars(value, 1_000));
    Some(WebSource {
        url,
        title,
        snippet,
    })
}

async fn read_bounded_response(
    mut response: reqwest::Response,
    max_bytes: usize,
    cancellation: CancellationToken,
) -> WebRuntimeResult<Vec<u8>> {
    let mut bytes = Vec::new();
    loop {
        let chunk = tokio::select! {
            _ = cancellation.cancelled() => return Err(WebRuntimeError::Cancelled),
            chunk = response.chunk() => chunk?,
        };
        let Some(chunk) = chunk else {
            return Ok(bytes);
        };
        if bytes.len().saturating_add(chunk.len()) > max_bytes {
            return Err(WebRuntimeError::Provider(format!(
                "response exceeds {} bytes",
                max_bytes
            )));
        }
        bytes.extend_from_slice(&chunk);
    }
}

fn deduplicate_sources(sources: &mut Vec<WebSource>) {
    let mut seen = BTreeSet::new();
    sources.retain(|source| seen.insert(source.url.clone()));
}

async fn validate_and_resolve_url(
    url: &Url,
    policy: &WebDomainPolicy,
) -> WebRuntimeResult<(String, SocketAddr)> {
    if url.scheme() != "https" {
        return Err(WebRuntimeError::Denied(
            "web_fetch only permits HTTPS".into(),
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(WebRuntimeError::Denied(
            "URLs with embedded credentials are not allowed".into(),
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| WebRuntimeError::Invalid("URL has no host".into()))?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if !policy.permits(&host) {
        return Err(WebRuntimeError::Denied(format!(
            "domain is not allowed by policy: {host}"
        )));
    }
    let port = url
        .port_or_known_default()
        .ok_or_else(|| WebRuntimeError::Invalid("URL has no resolvable port".into()))?;
    let addresses = tokio::net::lookup_host((host.as_str(), port))
        .await?
        .collect::<Vec<_>>();
    if addresses.is_empty() {
        return Err(WebRuntimeError::Dns(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "host resolved to no addresses",
        )));
    }
    if addresses.iter().any(|address| !is_public_ip(address.ip())) {
        return Err(WebRuntimeError::Denied(
            "host resolves to a private, local, reserved or metadata-network address".into(),
        ));
    }
    Ok((host, addresses[0]))
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => public_ipv4(ip),
        IpAddr::V6(ip) => public_ipv6(ip),
    }
}

fn public_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0
        || a == 10
        || a == 127
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 192 && b == 168)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || a >= 224)
}

fn public_ipv6(ip: Ipv6Addr) -> bool {
    if let Some(ipv4) = ip.to_ipv4_mapped() {
        return public_ipv4(ipv4);
    }
    let segments = ip.segments();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] & 0xff00) == 0xff00
        || (segments[0] == 0x2001 && segments[1] == 0x0db8))
}

fn normalize_domains(
    domains: impl IntoIterator<Item = String>,
) -> WebRuntimeResult<BTreeSet<String>> {
    let mut normalized = BTreeSet::new();
    for domain in domains {
        let domain = domain
            .trim()
            .trim_start_matches("*.")
            .trim_end_matches('.')
            .to_ascii_lowercase();
        if domain.is_empty()
            || domain.len() > 253
            || domain.contains('/')
            || domain.contains(':')
            || domain.chars().any(char::is_whitespace)
        {
            return Err(WebRuntimeError::Invalid(format!(
                "invalid domain filter: {domain}"
            )));
        }
        normalized.insert(domain);
    }
    Ok(normalized)
}

fn domain_matches(domain: &str, suffix: &str) -> bool {
    domain == suffix || domain.ends_with(&format!(".{suffix}"))
}

fn is_textual_content_type(content_type: &str) -> bool {
    content_type.starts_with("text/")
        || matches!(
            content_type,
            "application/json"
                | "application/ld+json"
                | "application/xml"
                | "application/xhtml+xml"
                | "application/rss+xml"
                | "application/atom+xml"
        )
}

fn truncate_chars(value: &str, max: usize) -> String {
    let mut truncated = value.chars().take(max).collect::<String>();
    if value.chars().count() > max {
        truncated.push('…');
    }
    truncated
}

fn web_tool_error(tool: &'static str) -> impl FnOnce(WebRuntimeError) -> ToolError + Copy {
    move |error| match error {
        WebRuntimeError::Invalid(message) => ToolError::InvalidArgument(message),
        WebRuntimeError::Denied(message) => ToolError::PermissionDenied(message),
        WebRuntimeError::Cancelled => ToolError::Cancelled(tool.into()),
        error => ToolError::execution(tool, error.to_string(), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_openai_answer_and_deduplicates_citations() {
        let response = json!({
            "output": [{
                "type": "message",
                "content": [{
                    "type": "output_text",
                    "text": "Current answer",
                    "annotations": [
                        {"type":"url_citation","url":"https://example.com/a","title":"A"},
                        {"type":"url_citation","url":"https://example.com/a","title":"A duplicate"}
                    ]
                }]
            }]
        });
        let parsed = parse_openai_search_response(&response, 10).unwrap();
        assert_eq!(parsed.answer, "Current answer");
        assert_eq!(parsed.sources.len(), 1);
        assert_eq!(parsed.sources[0].url, "https://example.com/a");
    }

    #[test]
    fn domain_policy_is_suffix_safe_and_blocks_local_hosts() {
        let policy = WebDomainPolicy::new(vec!["example.com".into()], Vec::new()).unwrap();
        assert!(policy.permits("docs.example.com"));
        assert!(!policy.permits("example.com.evil.test"));
        assert!(!policy.permits("localhost"));
    }

    #[test]
    fn private_and_reserved_addresses_are_denied() {
        assert!(!is_public_ip("127.0.0.1".parse().unwrap()));
        assert!(!is_public_ip("169.254.169.254".parse().unwrap()));
        assert!(!is_public_ip("10.1.2.3".parse().unwrap()));
        assert!(!is_public_ip("::1".parse().unwrap()));
        assert!(!is_public_ip("2001:db8::1".parse().unwrap()));
        assert!(is_public_ip("8.8.8.8".parse().unwrap()));
    }

    #[tokio::test]
    async fn fetch_rejects_non_https_before_network_access() {
        struct EmptyProvider;
        #[async_trait]
        impl WebSearchProvider for EmptyProvider {
            async fn search(
                &self,
                _request: WebSearchRequest,
                _cancellation: CancellationToken,
            ) -> WebRuntimeResult<WebSearchResult> {
                unreachable!()
            }
        }
        let runtime = WebRuntime::new(
            Arc::new(EmptyProvider),
            WebDomainPolicy::unrestricted_public_web(),
        );
        let error = runtime
            .fetch("http://example.com", CancellationToken::new())
            .await
            .unwrap_err();
        assert!(matches!(error, WebRuntimeError::Denied(_)));
    }
}
