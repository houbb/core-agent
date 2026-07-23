//! P9: Enterprise Agent Operating System — 端到端测试
//!
//! 覆盖完整的企业级 Agent 流程：
//! 1. Tenant → Organization → Department → Team → User 层级
//! 2. Security: RBAC + Secret 管理
//! 3. Policy: DataPolicy + ActionPolicy + Ask 决策
//! 4. Compliance: EvidenceChain + ComplianceRecord + Model Governance + Risk Assessment

use std::collections::BTreeSet;
use std::sync::Arc;

use chrono::Utc;
use core_agent_governance::{
    AgentIdentityCredential, AgentOwnership, AgentRiskAssessment, ComplianceDashboard,
    ComplianceRecord, ComplianceStandard, EnterpriseGovernanceManager, EnterprisePrincipal,
    EvidenceChain, GovernanceAsset, IdentityProviderKind, ModelGovernanceRecord, Permission,
    ResourceProtection, RiskDimension, RiskLevel, Role, RoleBinding, Secret,
};
use core_agent_platform::{
    ActionEnvironment, ActionPolicy, DataClassification, DataPolicy, Department, EnterpriseUser,
    ModelPolicy, PlatformManager, PlatformOrganization, PolicyEffect, PolicyRule, Team, Tenant,
    TenantPlan, TenantSettings, ToolPolicy,
};
use uuid::Uuid;

fn setup() -> (Arc<PlatformManager>, Arc<EnterpriseGovernanceManager>, Uuid, Uuid) {
    let platform = Arc::new(PlatformManager::builder().build());
    platform.start().unwrap();
    let governance = Arc::new(EnterpriseGovernanceManager::new(platform.clone()));

    // Create tenant
    let tenant = Tenant::new("acme", "Acme Corp", "admin");
    let tenant = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(platform.create_tenant(tenant))
        .unwrap();

    // Create an allow-all policy for enterprise operations
    let mut policy = core_agent_platform::PlatformPolicy::new(
        tenant.id,
        "enterprise-allow",
        "Enterprise Allow All",
        "system",
    );
    policy.rules.push(PolicyRule {
        id: Uuid::new_v4(),
        subjects: ["*".into()].into_iter().collect(),
        actions: ["*".into()].into_iter().collect(),
        resources: ["*".into()].into_iter().collect(),
        attributes: std::collections::BTreeMap::new(),
        effect: PolicyEffect::Allow,
        priority: 100,
    });
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(platform.create_policy(policy))
        .unwrap();

    // Create organization
    let org = PlatformOrganization::new(tenant.id, "eng", "Engineering", "admin");
    let org = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(platform.create_organization(org))
        .unwrap();

    // Bind principal for governance operations
    let principal = EnterprisePrincipal::new(
        tenant.id,
        "admin",
        IdentityProviderKind::LocalAdapter,
        "Admin User",
        "admin",
    );
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(governance.bind_principal(principal))
        .unwrap();

    (platform, governance, tenant.id, org.id)
}

// ─── Tenant Hierarchy E2E ─────────────────────────────────────────────────

#[test]
fn p9_tenant_hierarchy_e2e() {
    let (platform, _governance, tenant_id, org_id) = setup();

    // 1. Create Department
    let dept = Department::new(tenant_id, org_id, "platform", "Platform Dept", "admin");
    let dept = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(platform.create_department(dept))
        .unwrap();
    assert_eq!(dept.key, "platform");
    assert_eq!(dept.organization_id, org_id);

    // 2. Create Team
    let team = Team::new(tenant_id, org_id, "core", "Core Team", "admin");
    let mut team = team;
    team.department_id = Some(dept.id);
    let team = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(platform.create_team(team))
        .unwrap();
    assert_eq!(team.key, "core");
    assert_eq!(team.department_id, Some(dept.id));

    // 3. Create Enterprise User
    let mut user = EnterpriseUser::new(tenant_id, "alice", "Alice", "admin");
    user.email = "alice@acme.com".into();
    user.department_ids.insert(dept.id);
    user.team_ids.insert(team.id);
    let user = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(platform.create_user(user))
        .unwrap();
    assert_eq!(user.external_subject, "alice");
    assert!(user.department_ids.contains(&dept.id));

    // 4. Verify hierarchy via list methods
    let depts = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(platform.list_departments(tenant_id, org_id))
        .unwrap();
    assert_eq!(depts.len(), 1);

    let teams = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(platform.list_teams(tenant_id, org_id, Some(dept.id)))
        .unwrap();
    assert_eq!(teams.len(), 1);

    let users = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(platform.list_users(tenant_id))
        .unwrap();
    assert_eq!(users.len(), 1);
}

// ─── RBAC + Secret E2E ────────────────────────────────────────────────────

#[test]
fn p9_rbac_and_secret_e2e() {
    let (_platform, governance, tenant_id, _org_id) = setup();

    // 1. Create Role
    let role = Role::new(tenant_id, "dev", "Developer", "admin");
    let mut role = role;
    role.permissions.insert("code.read".into());
    role.permissions.insert("code.write".into());
    let role = governance.create_role(role).unwrap();
    assert!(role.has_permission("code.read"));

    // 2. Create Permission
    let perm = Permission::new(tenant_id, "code.read", "Read Code", "read", "code", "admin");
    governance.create_permission(perm).unwrap();

    // 3. Bind principal to role
    // First create a principal
    let principal = EnterprisePrincipal::new(
        tenant_id,
        "dev-user",
        IdentityProviderKind::LocalAdapter,
        "Dev User",
        "admin",
    );
    let principal = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(governance.bind_principal(principal))
        .unwrap();
    let binding = RoleBinding::new(tenant_id, principal.id, role.id, "admin");
    governance.bind_role(binding).unwrap();

    // 4. Secret lifecycle
    let secret = Secret::new(
        tenant_id,
        "db-password",
        "DB Password",
        vec![1, 2, 3, 4],
        "admin",
        "admin",
    );
    governance.store_secret(secret).unwrap();

    let read = governance
        .read_secret(tenant_id, "db-password", "admin")
        .unwrap();
    assert_eq!(read.encrypted_value, vec![1, 2, 3, 4]);

    // Rotate
    governance
        .rotate_secret(tenant_id, "db-password", vec![5, 6, 7, 8], "admin")
        .unwrap();
    let rotated = governance
        .read_secret(tenant_id, "db-password", "admin")
        .unwrap();
    assert_eq!(rotated.encrypted_value, vec![5, 6, 7, 8]);
    assert!(rotated.last_rotated_at.is_some());

    // Delete
    governance
        .delete_secret(tenant_id, "db-password", "admin")
        .unwrap();
    assert!(governance
        .read_secret(tenant_id, "db-password", "admin")
        .is_err());
}

// ─── Agent Identity + Resource Security E2E ───────────────────────────────

#[test]
fn p9_agent_identity_and_resource_security_e2e() {
    let (_platform, governance, tenant_id, _org_id) = setup();

    // 1. Issue agent credential
    let agent_id = Uuid::new_v4();
    let cred = AgentIdentityCredential::new(
        agent_id,
        tenant_id,
        "ssh-rsa AAAAB3NzaC1yc2E...",
        "RSA-4096",
        "admin",
    );
    let cred = governance.issue_agent_credential(cred).unwrap();
    assert!(!cred.revoked);

    // 2. Verify agent identity
    let valid = governance
        .verify_agent_identity(agent_id, cred.id)
        .unwrap();
    assert!(valid);

    // 3. Revoke and verify
    governance
        .revoke_agent_credential(cred.id, "admin")
        .unwrap();
    let revoked = governance
        .verify_agent_identity(agent_id, cred.id)
        .unwrap();
    assert!(!revoked);

    // 4. Resource protection
    let mut prot = ResourceProtection::new(tenant_id, "database", "salary-db", "admin");
    prot.required_permissions.insert("db.read".into());
    governance.define_resource_protection(prot).unwrap();

    let mut perms = BTreeSet::new();
    perms.insert("db.read".into());
    let allowed = governance
        .check_resource_access(tenant_id, "database", "salary-db", "db.read", &perms)
        .unwrap();
    assert!(allowed);

    let mut no_perms = BTreeSet::new();
    let denied = governance
        .check_resource_access(tenant_id, "database", "salary-db", "db.write", &no_perms)
        .unwrap();
    assert!(!denied);
}

// ─── DataPolicy + ActionPolicy E2E ────────────────────────────────────────

#[test]
fn p9_data_and_action_policy_e2e() {
    let (platform, _governance, tenant_id, _org_id) = setup();

    // 1. DataPolicy
    let dp = DataPolicy::new(
        tenant_id,
        "salary-data",
        "Salary Data Policy",
        DataClassification::Confidential,
        "admin",
    );
    let mut dp = dp;
    dp.resource_pattern = "salary/*".into();
    dp.allowed_actions.insert("read.self".into());
    dp.denied_actions.insert("read.all".into());
    dp.denied_actions.insert("write".into());

    let dp = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(platform.create_data_policy(dp))
        .unwrap();
    assert_eq!(dp.data_classification, DataClassification::Confidential);

    // Verify evaluate
    assert_eq!(
        dp.evaluate(
            DataClassification::Confidential,
            "salary/employee1",
            "read.self"
        ),
        PolicyEffect::Allow
    );
    assert_eq!(
        dp.evaluate(
            DataClassification::Confidential,
            "salary/employee1",
            "read.all"
        ),
        PolicyEffect::Deny
    );
    assert_eq!(
        dp.evaluate(DataClassification::Public, "salary/employee1", "read.all"),
        PolicyEffect::Allow // Not classified = not restricted
    );

    // 2. ActionPolicy
    let ap = ActionPolicy::new(
        tenant_id,
        "prod-deploy",
        "Production Deploy",
        "deploy.*",
        ActionEnvironment::Production,
        "admin",
    );
    let mut ap = ap;
    ap.required_approval = true;
    ap.risk_level = core_agent_platform::ActionRiskLevel::High;

    let ap = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(platform.create_action_policy(ap))
        .unwrap();
    assert!(ap.required_approval);
    assert!(ap.matches("deploy.release", ActionEnvironment::Production));
    assert!(!ap.matches("deploy.release", ActionEnvironment::Development));
}

// ─── Compliance E2E ───────────────────────────────────────────────────────

#[test]
fn p9_compliance_e2e() {
    let (_platform, governance, tenant_id, _org_id) = setup();

    // 1. Evidence Chain
    let audit_id = Uuid::new_v4();
    let chain = EvidenceChain::new(tenant_id, audit_id, "admin", None);
    assert!(chain.verify_chain());
    let chain = governance.append_evidence(chain).unwrap();
    assert!(!chain.chain_hash.is_empty());

    // Chain the next evidence
    let audit_id2 = Uuid::new_v4();
    let chain2 = EvidenceChain::new(tenant_id, audit_id2, "admin", Some(chain.id));
    assert!(chain2.verify_chain());
    governance.append_evidence(chain2).unwrap();

    let chains = governance.list_evidence_chains(tenant_id).unwrap();
    assert_eq!(chains.len(), 2);

    // 2. Compliance Record
    let mut record = ComplianceRecord::new(
        tenant_id,
        "agent",
        "agent-1",
        ComplianceStandard::Iso27001,
        "admin",
    );
    record.rule_name = "A.6.1.2".into();
    record.status = core_agent_governance::ComplianceStatus::Compliant;
    record.evidence_ids.insert(chain.id);
    governance.create_compliance_record(record).unwrap();

    let records = governance
        .list_compliance_records(tenant_id, Some(ComplianceStandard::Iso27001))
        .unwrap();
    assert_eq!(records.len(), 1);

    // 3. Compliance Snapshot
    let snapshot = governance.compliance_snapshot(tenant_id).unwrap();
    assert_eq!(snapshot.total_resources, 1);
    assert_eq!(snapshot.compliant, 1);
}

// ─── Model Governance + Risk Assessment E2E ───────────────────────────────

#[test]
fn p9_model_governance_and_risk_e2e() {
    let (_platform, governance, tenant_id, _org_id) = setup();

    // 1. Model Governance
    let agent_id = Uuid::new_v4();
    let record = ModelGovernanceRecord::new(
        tenant_id,
        agent_id,
        "claude-opus-5",
        "5.0",
        "What is the meaning of life?",
        "42",
        "admin",
    );
    let record = governance.record_model_use(record).unwrap();
    assert!(!record.prompt_hash.is_empty());
    assert!(!record.output_hash.is_empty());

    let records = governance
        .query_model_usage(tenant_id, Some(agent_id))
        .unwrap();
    assert_eq!(records.len(), 1);

    // 2. Risk Assessment
    let mut assessment = AgentRiskAssessment::new(tenant_id, agent_id, "admin");
    // Score based on dimensions
    assessment.dimensions.insert(RiskDimension::DataAccess, 80);
    assessment.dimensions.insert(RiskDimension::ToolAccess, 60);
    assessment.dimensions.insert(RiskDimension::NetworkAccess, 30);
    assessment.risk_score = 70;
    assessment.risk_level = AgentRiskAssessment::compute_risk_level(70);
    assert_eq!(assessment.risk_level, RiskLevel::High);

    governance.assess_agent_risk(assessment).unwrap();

    let assessments = governance.list_risk_assessments(tenant_id).unwrap();
    assert_eq!(assessments.len(), 1);
    assert_eq!(assessments[0].risk_level, RiskLevel::High);
}

// ─── Full Enterprise Snapshot E2E ─────────────────────────────────────────

#[test]
fn p9_enterprise_snapshot_e2e() {
    let (_platform, governance, tenant_id, _org_id) = setup();

    // Seed some data
    governance
        .create_role(Role::new(tenant_id, "viewer", "Viewer", "admin"))
        .unwrap();
    governance
        .store_secret(Secret::new(
            tenant_id,
            "api-key",
            "API Key",
            vec![1],
            "admin",
            "admin",
        ))
        .unwrap();
    governance
        .record_model_use(ModelGovernanceRecord::new(
            tenant_id,
            Uuid::new_v4(),
            "gpt-5",
            "1.0",
            "test",
            "result",
            "admin",
        ))
        .unwrap();

    // Full snapshot
    let snap = governance.snapshot(tenant_id).unwrap();
    assert_eq!(snap.roles, 1);
    assert_eq!(snap.secrets, 1);
    assert_eq!(snap.model_governance_records, 1);
    assert!(snap.principals > 0); // admin principal was created in setup
}

// ─── ToolPolicy + ModelPolicy E2E ─────────────────────────────────────────

#[test]
fn p9_tool_and_model_policy_e2e() {
    let (platform, _governance, tenant_id, _org_id) = setup();

    // 1. ToolPolicy
    let tp = ToolPolicy::new(tenant_id, "shell-tools", "Shell Tools Policy", "admin");
    let mut tp = tp;
    tp.tool_pattern = "shell/*".into();
    tp.allowed_tools.insert("shell/ls".into());
    tp.denied_tools.insert("shell/rm".into());
    tp.denied_tools.insert("shell/exec".into());

    let tp = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(platform.create_tool_policy(tp))
        .unwrap();

    assert_eq!(
        tp.evaluate("shell/ls", "shell"),
        PolicyEffect::Allow
    );
    assert_eq!(
        tp.evaluate("shell/rm", "shell"),
        PolicyEffect::Deny
    );
    assert_eq!(
        tp.evaluate("shell/exec", "shell"),
        PolicyEffect::Deny
    );
    // Non-matching pattern = not restricted
    assert_eq!(
        tp.evaluate("git.commit", "git"),
        PolicyEffect::Allow
    );

    // 2. ModelPolicy
    let mp = ModelPolicy::new(tenant_id, "model-restrict", "Model Restriction", "admin");
    let mut mp = mp;
    mp.allowed_providers.insert("internal".into());
    mp.denied_providers.insert("external".into());
    mp.allowed_models.insert("internal/gpt-4".into());
    mp.denied_models.insert("external/claude-5".into());

    let mp = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(platform.create_model_policy(mp))
        .unwrap();

    assert_eq!(
        mp.evaluate("internal", "internal/gpt-4"),
        PolicyEffect::Allow
    );
    assert_eq!(
        mp.evaluate("external", "external/claude-5"),
        PolicyEffect::Deny
    );
    assert_eq!(
        mp.evaluate("external", "external/gpt-5"),
        PolicyEffect::Deny  // denied via provider
    );
    assert_eq!(
        mp.evaluate("internal", "internal/llama-3"),
        PolicyEffect::Allow  // allowed via provider
    );

    // List policies
    let tps = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(platform.list_tool_policies(tenant_id))
        .unwrap();
    assert_eq!(tps.len(), 1);

    let mps = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(platform.list_model_policies(tenant_id))
        .unwrap();
    assert_eq!(mps.len(), 1);
}

// ─── Agent Ownership E2E ──────────────────────────────────────────────────

#[test]
fn p9_agent_ownership_e2e() {
    let (_platform, governance, tenant_id, _org_id) = setup();

    let agent_id = Uuid::new_v4();

    // Create principal first
    let principal = EnterprisePrincipal::new(
        tenant_id,
        "owner",
        IdentityProviderKind::LocalAdapter,
        "Owner",
        "admin",
    );
    let principal = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(governance.bind_principal(principal))
        .unwrap();

    // Register agent ownership
    let ownership = AgentOwnership::new(tenant_id, agent_id, "rca-agent", "owner", principal.id);
    let mut ownership = ownership;
    ownership.allow_self_serve = true;
    governance.register_agent_ownership(ownership).unwrap();

    let list = governance.list_agent_ownerships(tenant_id).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].agent_key, "rca-agent");

    // Check access: owner should have access
    let mut roles = BTreeSet::new();
    let access = governance
        .check_agent_access(tenant_id, agent_id, &principal.id, &roles)
        .unwrap();
    assert!(access);

    // Check access: unauthorized user should not have access
    let other_id = Uuid::new_v4();
    let no_access = governance
        .check_agent_access(tenant_id, agent_id, &other_id, &roles)
        .unwrap();
    assert!(!no_access);
}

// ─── Compliance Dashboard E2E ─────────────────────────────────────────────

#[test]
fn p9_compliance_dashboard_e2e() {
    let (_platform, governance, tenant_id, _org_id) = setup();

    // Seed compliance records
    let mut record1 = ComplianceRecord::new(
        tenant_id,
        "agent",
        "agent-1",
        ComplianceStandard::Iso27001,
        "admin",
    );
    record1.rule_name = "A.6.1.2".into();
    record1.status = core_agent_governance::ComplianceStatus::Compliant;
    governance.create_compliance_record(record1).unwrap();

    let mut record2 = ComplianceRecord::new(
        tenant_id,
        "agent",
        "agent-2",
        ComplianceStandard::Soc2,
        "admin",
    );
    record2.rule_name = "CC-1".into();
    record2.status = core_agent_governance::ComplianceStatus::NonCompliant;
    governance.create_compliance_record(record2).unwrap();

    // Seed risk assessments
    let mut assessment = AgentRiskAssessment::new(tenant_id, Uuid::new_v4(), "admin");
    assessment.risk_score = 85;
    assessment.risk_level = AgentRiskAssessment::compute_risk_level(85);
    governance.assess_agent_risk(assessment).unwrap();

    // Get dashboard
    let dashboard = governance.compliance_dashboard(tenant_id).unwrap();
    assert_eq!(dashboard.total_compliance_records, 2);
    assert_eq!(dashboard.compliant_count, 1);
    assert_eq!(dashboard.non_compliant_count, 1);
    assert_eq!(dashboard.high_risk_agents, 1);
    assert!(dashboard.by_standard.contains_key("Iso27001"));
    assert!(dashboard.by_standard.contains_key("Soc2"));
}