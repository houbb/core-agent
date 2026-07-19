//! Shared product-stage contracts for AgentOS experience surfaces.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProductPhase {
    TerminalMvp,
    ProfessionalCli,
    DeveloperDesktop,
    AgentStudio,
    TeamCollaboration,
    EnterprisePlatform,
    #[serde(alias = "AGENT_OPERATING_SYSTEM")]
    AgentEcosystem,
}

impl ProductPhase {
    pub const ALL: [Self; 7] = [
        Self::TerminalMvp,
        Self::ProfessionalCli,
        Self::DeveloperDesktop,
        Self::AgentStudio,
        Self::TeamCollaboration,
        Self::EnterprisePlatform,
        Self::AgentEcosystem,
    ];

    pub fn previous(self) -> Option<Self> {
        match self {
            Self::TerminalMvp => None,
            Self::ProfessionalCli => Some(Self::TerminalMvp),
            Self::DeveloperDesktop => Some(Self::ProfessionalCli),
            Self::AgentStudio => Some(Self::DeveloperDesktop),
            Self::TeamCollaboration => Some(Self::AgentStudio),
            Self::EnterprisePlatform => Some(Self::TeamCollaboration),
            Self::AgentEcosystem => Some(Self::EnterprisePlatform),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExperienceSurface {
    Cli,
    Desktop,
    Web,
    Ide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProductCapability {
    AgentLoop,
    ToolCalling,
    Workspace,
    Session,
    Context,
    Memory,
    MultiModel,
    ProjectIntelligence,
    CommandSystem,
    ExtensionFoundation,
    DesktopWorkbench,
    AgentBuilder,
    WorkflowBuilder,
    MemoryStudio,
    TeamCollaboration,
    ReviewAndAudit,
    MultiTenantGovernance,
    SecurityAndCost,
    Marketplace,
    DeveloperSdk,
    PublishingCenter,
    TemplateCenter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseDefinition {
    pub phase: ProductPhase,
    pub surfaces: BTreeSet<ExperienceSurface>,
    pub required_capabilities: BTreeSet<ProductCapability>,
}

impl PhaseDefinition {
    pub fn for_phase(phase: ProductPhase) -> Self {
        use ExperienceSurface::*;
        use ProductCapability::*;
        let (surfaces, required_capabilities) = match phase {
            ProductPhase::TerminalMvp => (
                [Cli].into_iter().collect(),
                [
                    AgentLoop,
                    ToolCalling,
                    Workspace,
                    Session,
                    Context,
                    Memory,
                    MultiModel,
                ]
                .into_iter()
                .collect(),
            ),
            ProductPhase::ProfessionalCli => (
                [Cli].into_iter().collect(),
                [ProjectIntelligence, CommandSystem, ExtensionFoundation]
                    .into_iter()
                    .collect(),
            ),
            ProductPhase::DeveloperDesktop => (
                [Cli, Desktop].into_iter().collect(),
                [DesktopWorkbench].into_iter().collect(),
            ),
            ProductPhase::AgentStudio => (
                [Desktop, Web].into_iter().collect(),
                [AgentBuilder, WorkflowBuilder, MemoryStudio]
                    .into_iter()
                    .collect(),
            ),
            ProductPhase::TeamCollaboration => (
                [Desktop, Web].into_iter().collect(),
                [TeamCollaboration, ReviewAndAudit].into_iter().collect(),
            ),
            ProductPhase::EnterprisePlatform => (
                [Desktop, Web].into_iter().collect(),
                [MultiTenantGovernance, SecurityAndCost]
                    .into_iter()
                    .collect(),
            ),
            ProductPhase::AgentEcosystem => (
                [Cli, Desktop, Web, Ide].into_iter().collect(),
                [Marketplace, DeveloperSdk, PublishingCenter, TemplateCenter]
                    .into_iter()
                    .collect(),
            ),
        };
        Self {
            phase,
            surfaces,
            required_capabilities,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseReadiness {
    pub phase: ProductPhase,
    pub ready: bool,
    pub unmet_predecessor: Option<ProductPhase>,
    pub missing_capabilities: BTreeSet<ProductCapability>,
}

pub fn evaluate_readiness(
    phase: ProductPhase,
    completed_phases: &BTreeSet<ProductPhase>,
    implemented_capabilities: &BTreeSet<ProductCapability>,
) -> PhaseReadiness {
    let definition = PhaseDefinition::for_phase(phase);
    let unmet_predecessor = phase
        .previous()
        .filter(|predecessor| !completed_phases.contains(predecessor));
    let missing_capabilities = definition
        .required_capabilities
        .difference(implemented_capabilities)
        .copied()
        .collect::<BTreeSet<_>>();
    PhaseReadiness {
        phase,
        ready: unmet_predecessor.is_none() && missing_capabilities.is_empty(),
        unmet_predecessor,
        missing_capabilities,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roadmap_contains_all_seven_ordered_phases() {
        assert_eq!(ProductPhase::ALL.len(), 7);
        assert_eq!(
            ProductPhase::ProfessionalCli.previous(),
            Some(ProductPhase::TerminalMvp)
        );
    }

    #[test]
    fn readiness_reports_capability_and_predecessor_gaps() {
        let readiness = evaluate_readiness(
            ProductPhase::ProfessionalCli,
            &BTreeSet::new(),
            &BTreeSet::new(),
        );
        assert!(!readiness.ready);
        assert_eq!(readiness.unmet_predecessor, Some(ProductPhase::TerminalMvp));
        assert_eq!(readiness.missing_capabilities.len(), 3);
    }

    #[test]
    fn phase_is_ready_only_when_contract_is_complete() {
        let definition = PhaseDefinition::for_phase(ProductPhase::TerminalMvp);
        let readiness = evaluate_readiness(
            ProductPhase::TerminalMvp,
            &BTreeSet::new(),
            &definition.required_capabilities,
        );
        assert!(readiness.ready);
    }
}
