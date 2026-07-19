use std::fs;
use std::sync::{Arc, Mutex};

use agent_cli::{
    CliConfig, CliError, CliResult, CommandDefinition, CommandRegistry, ProfessionalAgentClient,
    ProfessionalApplication, ProfessionalRequest, ProfessionalResponse, ProfileState,
    ProjectSnapshot, TerminalHistory,
};
use async_trait::async_trait;
use tempfile::tempdir;

#[derive(Default)]
struct MockProfessionalClient {
    indexed: Mutex<Vec<ProjectSnapshot>>,
    requests: Mutex<Vec<ProfessionalRequest>>,
}

#[async_trait]
impl ProfessionalAgentClient for MockProfessionalClient {
    async fn index_project(
        &self,
        project: ProjectSnapshot,
        profile: &str,
    ) -> CliResult<ProfessionalResponse> {
        self.indexed.lock().unwrap().push(project);
        Ok(ProfessionalResponse {
            summary: format!("Project indexed for {profile}"),
            items: vec!["Rust detected".into()],
            data: serde_json::Value::Null,
        })
    }

    async fn execute_professional(
        &self,
        request: ProfessionalRequest,
    ) -> CliResult<ProfessionalResponse> {
        let name = request.invocation.name.clone();
        self.requests.lock().unwrap().push(request);
        Ok(ProfessionalResponse {
            summary: format!("{name} completed"),
            items: vec!["No critical issues".into()],
            data: serde_json::Value::Null,
        })
    }
}

#[test]
fn project_snapshot_detects_root_markers_modules_and_git_branch() {
    let directory = tempdir().unwrap();
    fs::write(directory.path().join("Cargo.toml"), "[package]").unwrap();
    fs::write(directory.path().join("Dockerfile"), "FROM scratch").unwrap();
    fs::create_dir(directory.path().join(".git")).unwrap();
    fs::write(
        directory.path().join(".git/HEAD"),
        "ref: refs/heads/feature/auth\n",
    )
    .unwrap();
    fs::create_dir(directory.path().join("web")).unwrap();
    fs::write(directory.path().join("web/package.json"), "{}").unwrap();

    let snapshot = ProjectSnapshot::scan(directory.path()).unwrap();

    assert!(snapshot.languages.contains("Rust"));
    assert!(snapshot.build_systems.contains("Cargo"));
    assert!(snapshot.frameworks.contains("Container"));
    assert_eq!(snapshot.git_branch.as_deref(), Some("feature/auth"));
    assert_eq!(snapshot.modules, vec!["web"]);
}

#[test]
fn command_registry_parses_quotes_completes_and_rejects_duplicates() {
    let mut registry = CommandRegistry::with_builtins();
    let invocation = registry.parse("/plan \"refactor auth module\"").unwrap();
    assert_eq!(invocation.name, "plan");
    assert_eq!(invocation.arguments, vec!["refactor auth module"]);
    assert!(registry.complete("/pro").contains(&"/project".into()));
    assert!(registry.parse("/unknown").is_err());
    assert!(registry
        .register(CommandDefinition {
            name: "plan".into(),
            summary: "duplicate".into(),
            usage: "/plan".into(),
            minimum_arguments: 0,
            maximum_arguments: 0,
        })
        .is_err());
}

#[tokio::test]
async fn profile_project_review_and_private_history_form_one_flow() {
    let directory = tempdir().unwrap();
    CliConfig::initialize(directory.path()).unwrap();
    fs::write(directory.path().join("Cargo.toml"), "[package]").unwrap();
    let client = Arc::new(MockProfessionalClient::default());
    let application = ProfessionalApplication::new(directory.path(), client.clone());

    assert_eq!(
        application
            .execute_line("/profile architect")
            .await
            .unwrap(),
        vec!["Profile: architect"]
    );
    assert!(application.execute_line("/project").await.unwrap()[0].contains("architect"));
    assert_eq!(
        application.execute_line("/review").await.unwrap()[0],
        "review completed"
    );

    assert_eq!(
        ProfileState::load(directory.path()).unwrap().active,
        "architect"
    );
    assert_eq!(client.indexed.lock().unwrap().len(), 1);
    let requests = client.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].profile, "architect");
    drop(requests);
    assert_eq!(
        TerminalHistory::load(directory.path()).unwrap().commands,
        vec!["/profile architect", "/project", "/review"]
    );
}

#[test]
fn history_refuses_plain_prompts_and_profile_refuses_unsafe_identity() {
    let directory = tempdir().unwrap();
    let mut history = TerminalHistory::default();
    assert!(matches!(
        history.record_command(directory.path(), "my password is x"),
        Err(CliError::InvalidArgument(_))
    ));
    assert!(ProfileState::set(directory.path(), "../architect").is_err());
}
