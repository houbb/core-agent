use core_agent::{AgentProfile, CreateAgentRequest, EnterpriseRuntimes};
use serde::Serialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{DesktopError, DesktopResult, DesktopState, RuntimeRequest};

#[tauri::command]
pub(crate) async fn runtime_request(
    state: tauri::State<'_, DesktopState>,
    request: RuntimeRequest,
) -> DesktopResult<Value> {
    let _operation = state.runtime_operation.lock().await;
    validate_request(&request)?;
    let path = request
        .path
        .split('?')
        .next()
        .unwrap_or(&request.path)
        .to_owned();
    let agent = state.agent().await;
    let runtimes = agent.runtimes();

    if request.method == "POST" && path == "/api/agent" {
        return create_agent(&agent, request.body).await;
    }
    if request.method != "GET" {
        return Err(DesktopError::NotFound(format!("{} {path}", request.method)));
    }

    match path.as_str() {
        "/api/agent" => {
            let agents = runtimes.agents.list().await.map_err(agent_error)?;
            items(agents.into_iter().map(agent_asset).collect::<Vec<_>>())
        }
        "/api/workflow" => items(
            runtimes
                .workflows
                .list_workflows()
                .await
                .map_err(agent_error)?,
        ),
        "/api/prompt" | "/api/knowledge" | "/api/trace" => empty_items(),
        "/api/memory" => items(runtimes.memory.list("project").await.map_err(agent_error)?),
        "/api/capability" => {
            let tools = agent.tools().list().await.map_err(agent_error)?;
            items(
                tools
                    .into_iter()
                    .map(|tool| {
                        json!({
                            "id": tool.id,
                            "name": tool.name,
                            "version": tool.version,
                            "state": if tool.enabled { "READY" } else { "DISABLED" },
                            "description": tool.description,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        }
        "/api/model" => {
            let profiles = agent.models().list_profiles().await.map_err(agent_error)?;
            items(
                profiles
                    .into_iter()
                    .map(|profile| {
                        json!({
                            "id": profile.id,
                            "name": profile.key,
                            "version": profile.model,
                            "state": if profile.enabled { "READY" } else { "DISABLED" },
                            "description": format!("{} / {}", profile.provider, profile.model),
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        }
        "/api/visual/catalog" => Ok(json!({
            "panels": runtimes.visual.catalog().map_err(agent_error)?.panels
        })),
        "/api/platform/health" => Ok(json!([{
            "component": "platform",
            "healthy": true,
            "state": format!("{:?}", runtimes.platform.status().map_err(agent_error)?),
        }])),
        "/api/platform/audit" => Ok(json!(runtimes
            .platform
            .list_audits(runtimes.tenant_id)
            .await
            .map_err(agent_error)?)),
        path if path.starts_with("/api/collaboration/") => {
            collaboration_response(path, &request, runtimes).await
        }
        path if path.starts_with("/api/enterprise/") => enterprise_response(path, runtimes).await,
        path if path.starts_with("/api/ecosystem/") => ecosystem_response(path, runtimes),
        _ => Err(DesktopError::NotFound(path)),
    }
}

async fn create_agent(
    agent: &core_agent::EnterpriseAgent,
    body: Option<Value>,
) -> DesktopResult<Value> {
    let body = body.ok_or_else(|| DesktopError::Validation("Agent body is required".into()))?;
    let name = required_string(&body, "name")?;
    let mut profile = AgentProfile::new(
        format!("studio-{}", Uuid::new_v4().simple()),
        required_string(&body, "role")?,
    );
    profile.model_key = Some(required_string(&body, "model")?.into());
    profile.memory_key = Some(required_string(&body, "memory")?.into());
    profile.toolset = body
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    let profile = agent
        .runtimes()
        .agents
        .register_profile(profile, "desktop-user")
        .await
        .map_err(agent_error)?;
    let mut request = CreateAgentRequest::new(name, profile.id);
    request.actor = "desktop-user".into();
    let value = agent
        .runtimes()
        .agents
        .create(request)
        .await
        .map_err(agent_error)?;
    Ok(agent_asset(value))
}

async fn collaboration_response(
    path: &str,
    request: &RuntimeRequest,
    runtimes: &EnterpriseRuntimes,
) -> DesktopResult<Value> {
    let project_id = project_id(request, runtimes);
    match path {
        "/api/collaboration/projects" => {
            let mut values = Vec::new();
            for project in runtimes.collaboration.projects().map_err(agent_error)? {
                values.push(json!({
                    "id": project.id,
                    "name": project.name,
                    "state": project.state,
                    "members": project.members.len(),
                    "agents": project.agent_ids.len(),
                    "tasks": runtimes.collaboration.tasks(project.id).map_err(agent_error)?.len(),
                    "knowledge": format!("{} assets", runtimes.collaboration.knowledge(project.id).map_err(agent_error)?.len()),
                }));
            }
            items(values)
        }
        "/api/collaboration/agents" => {
            let agents = runtimes.agents.list().await.map_err(agent_error)?;
            items(
                agents
                    .into_iter()
                    .map(|agent| {
                        json!({
                            "id": agent.id,
                            "name": agent.name,
                            "owner": agent.actor,
                            "version": agent.version.to_string(),
                            "model": agent.profile.model_key.unwrap_or_else(|| "default".into()),
                            "state": agent.state,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        }
        "/api/collaboration/members" => {
            let project = runtimes
                .collaboration
                .projects()
                .map_err(agent_error)?
                .into_iter()
                .find(|project| project.id == project_id);
            items(project.map_or_else(Vec::new, |project| {
                project
                    .members
                    .into_iter()
                    .map(|(name, role)| {
                        json!({"id": name, "name": name, "role": role, "state": "ACTIVE"})
                    })
                    .collect()
            }))
        }
        "/api/collaboration/tasks" => {
            let tasks = runtimes
                .collaboration
                .tasks(project_id)
                .map_err(agent_error)?;
            items(
                tasks
                    .into_iter()
                    .map(|task| {
                        json!({
                            "id": task.id,
                            "number": task.number,
                            "title": task.title,
                            "state": task.state,
                            "assignee": task.assignee,
                            "ownerAgent": task.owner_agent_id,
                            "reviewer": task.reviewer,
                            "progress": task.progress,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        }
        "/api/collaboration/reviews" | "/api/collaboration/approvals" => {
            let reviews = runtimes
                .collaboration
                .reviews(project_id)
                .map_err(agent_error)?;
            items(
                reviews
                    .into_iter()
                    .map(|review| {
                        json!({
                            "id": review.id,
                            "taskId": review.task_id,
                            "taskTitle": format!("Task {}", review.task_id),
                            "state": review.state,
                            "risk": review.risk,
                            "summary": review.summary,
                            "reviewer": review.actor,
                            "createdBy": review.actor,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        }
        "/api/collaboration/knowledge" => items(
            runtimes
                .collaboration
                .knowledge(project_id)
                .map_err(agent_error)?,
        ),
        "/api/collaboration/activity" | "/api/collaboration/notifications" => {
            let activity = runtimes
                .collaboration
                .activities(project_id)
                .map_err(agent_error)?;
            items(
                activity
                    .into_iter()
                    .map(|value| {
                        json!({
                            "id": value.id,
                            "kind": value.kind,
                            "subject": value.subject,
                            "summary": value.summary,
                            "entityType": value.entity_type,
                            "entityId": value.entity_id,
                            "occurredAt": value.occurred_at,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        }
        _ => Err(DesktopError::NotFound(path.into())),
    }
}

async fn enterprise_response(path: &str, runtimes: &EnterpriseRuntimes) -> DesktopResult<Value> {
    match path {
        "/api/enterprise/organizations" => {
            let organizations = runtimes
                .platform
                .list_organizations(runtimes.tenant_id)
                .await
                .map_err(agent_error)?;
            let asset_count = runtimes
                .governance
                .assets(runtimes.tenant_id)
                .map_err(agent_error)?
                .len();
            items(
                organizations
                    .into_iter()
                    .map(|value| {
                        json!({
                            "id": value.id,
                            "name": value.name,
                            "key": value.key,
                            "state": "ACTIVE",
                            "members": 1,
                            "assets": asset_count,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        }
        "/api/enterprise/identity" => {
            let principals = runtimes
                .governance
                .principals(runtimes.tenant_id)
                .map_err(agent_error)?;
            items(
                principals
                    .into_iter()
                    .map(|value| {
                        json!({
                            "id": value.id,
                            "externalSubject": value.external_subject,
                            "displayName": value.display_name,
                            "provider": value.provider,
                            "roles": value.roles,
                            "groups": value.groups,
                            "state": value.state,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        }
        "/api/enterprise/assets" => {
            let assets = runtimes
                .governance
                .assets(runtimes.tenant_id)
                .map_err(agent_error)?;
            items(
                assets
                    .into_iter()
                    .map(|value| {
                        json!({
                            "id": value.id,
                            "key": value.key,
                            "name": value.name,
                            "assetType": value.asset_type,
                            "assetVersion": value.asset_version,
                            "ownerSubject": value.owner_subject,
                            "classification": value.classification,
                            "environment": value.environment,
                            "state": value.state,
                            "riskScore": value.risk_score,
                            "approvals": value.approvals.len(),
                            "requiredApprovals": value.required_approvals,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        }
        "/api/enterprise/policies" => {
            let policies = runtimes
                .platform
                .list_policies(runtimes.tenant_id)
                .await
                .map_err(agent_error)?;
            items(
                policies
                    .into_iter()
                    .map(|value| {
                        json!({
                            "id": value.id,
                            "name": value.name,
                            "key": value.key,
                            "state": if value.enabled { "ACTIVE" } else { "DISABLED" },
                            "rules": value.rules.len(),
                            "scope": value.organization_id,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        }
        "/api/enterprise/costs" => {
            let costs = runtimes
                .governance
                .costs(runtimes.tenant_id)
                .map_err(agent_error)?;
            items(
                costs
                    .into_iter()
                    .map(|value| {
                        json!({
                            "id": value.id,
                            "eventKey": value.event_key,
                            "project": value.project_id,
                            "agent": value.agent_id,
                            "model": value.model_key,
                            "currency": value.currency,
                            "amountMicros": value.amount_micros.to_string(),
                            "inputTokens": value.input_tokens,
                            "outputTokens": value.output_tokens,
                            "occurredAt": value.occurred_at,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        }
        "/api/enterprise/audits" => items(
            runtimes
                .platform
                .list_audits(runtimes.tenant_id)
                .await
                .map_err(agent_error)?,
        ),
        "/api/enterprise/operations" => items(vec![json!({
            "component": "enterprise-agent",
            "state": format!("{:?}", runtimes.kernel.status().map_err(agent_error)?),
            "message": "All Runtime modules are embedded in this process",
            "checkedAt": chrono::Utc::now(),
        })]),
        _ => empty_items(),
    }
}

fn ecosystem_response(path: &str, runtimes: &EnterpriseRuntimes) -> DesktopResult<Value> {
    match path {
        "/api/ecosystem/packages" => {
            let packages = runtimes
                .ecosystem
                .packages(runtimes.tenant_id, false)
                .map_err(agent_error)?;
            items(
                packages
                    .into_iter()
                    .map(|value| {
                        json!({
                            "id": value.id,
                            "key": value.key,
                            "name": value.name,
                            "packageVersion": value.package_version,
                            "kind": value.kind,
                            "description": value.description,
                            "publisher": value.publisher_id,
                            "state": value.state,
                            "requiredCapabilities": value.required_capabilities,
                            "downloads": value.download_count,
                            "rating": value.average_rating(),
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        }
        "/api/ecosystem/publishers" => {
            let publishers = runtimes
                .ecosystem
                .publishers(runtimes.tenant_id)
                .map_err(agent_error)?;
            items(
                publishers
                    .into_iter()
                    .map(|value| {
                        json!({
                            "id": value.id,
                            "key": value.key,
                            "name": value.name,
                            "state": value.state,
                            "packages": 0,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        }
        _ => empty_items(),
    }
}

fn validate_request(request: &RuntimeRequest) -> DesktopResult<()> {
    if !matches!(request.method.as_str(), "GET" | "POST" | "PATCH" | "DELETE")
        || !request.path.starts_with("/api/")
        || request.path.len() > 2_048
        || request.path.contains("..")
        || request.path.contains('#')
    {
        return Err(DesktopError::Validation(
            "Runtime request path or method is invalid".into(),
        ));
    }
    if request
        .body
        .as_ref()
        .is_some_and(|body| serde_json::to_vec(body).is_ok_and(|value| value.len() > 1024 * 1024))
    {
        return Err(DesktopError::Validation(
            "Runtime request body exceeds 1 MiB".into(),
        ));
    }
    Ok(())
}

fn items(value: impl Serialize) -> DesktopResult<Value> {
    Ok(json!({"items": value}))
}

fn empty_items() -> DesktopResult<Value> {
    items(Vec::<Value>::new())
}

fn agent_asset(agent: core_agent::Agent) -> Value {
    json!({
        "id": agent.id,
        "name": agent.name,
        "version": agent.version.to_string(),
        "state": agent.state,
        "description": agent.profile.description,
    })
}

fn project_id(request: &RuntimeRequest, runtimes: &EnterpriseRuntimes) -> Uuid {
    request
        .path
        .split_once('?')
        .and_then(|(_, query)| {
            query
                .split('&')
                .find_map(|part| part.strip_prefix("projectId="))
        })
        .and_then(|value| Uuid::parse_str(value).ok())
        .unwrap_or(runtimes.collaboration_project_id)
}

fn required_string<'a>(value: &'a Value, key: &str) -> DesktopResult<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty() && value.len() <= 256)
        .ok_or_else(|| DesktopError::Validation(format!("Agent {key} is invalid")))
}

fn agent_error(error: impl std::fmt::Display) -> DesktopError {
    DesktopError::Agent(error.to_string())
}
