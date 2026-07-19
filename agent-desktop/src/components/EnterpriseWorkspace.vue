<script setup lang="ts">
import { computed, onMounted } from "vue";
import { Activity, BarChart3, Building2, CircleDollarSign, FolderCog, Gauge, KeyRound, Library, Settings, ShieldCheck } from "lucide-vue-next";
import { HttpEnterpriseApi } from "../enterprise-api";
import { createEnterpriseController } from "../enterprise-controller";
import type { EnterpriseSection } from "../enterprise-types";
import EmptyState from "./EmptyState.vue";
import PanelShell from "./PanelShell.vue";

const controller = createEnterpriseController(new HttpEnterpriseApi());
const navigation = [
  { id: "dashboard", label: "Dashboard", icon: Gauge }, { id: "organization", label: "Organization", icon: Building2 },
  { id: "identity", label: "Identity", icon: KeyRound }, { id: "assets", label: "AI Assets", icon: Library },
  { id: "governance", label: "Governance", icon: ShieldCheck }, { id: "policies", label: "Policies", icon: FolderCog },
  { id: "cost", label: "Cost", icon: CircleDollarSign }, { id: "audit", label: "Audit", icon: Activity },
  { id: "operation", label: "Operation", icon: BarChart3 }, { id: "settings", label: "Settings", icon: Settings },
] as const;
const highRisk = computed(() => controller.state.snapshot.assets.filter((item) => item.riskScore >= 70));
const pending = computed(() => controller.state.snapshot.assets.filter((item) => item.state === "REVIEWED"));
const costTotals = computed(() => {
  const totals = new Map<string, bigint>();
  for (const item of controller.state.snapshot.costs) totals.set(item.currency, (totals.get(item.currency) ?? 0n) + BigInt(item.amountMicros));
  return [...totals].map(([currency, micros]) => ({ currency, micros: micros.toString() }));
});
onMounted(controller.load);
function select(section: EnterpriseSection) { controller.select(section); }
</script>

<template>
  <section class="enterprise-workspace">
    <aside class="enterprise-nav">
      <div class="studio-heading"><span class="eyebrow">Enterprise AI</span><h2>Control Plane</h2></div>
      <button v-for="item in navigation" :key="item.id" :class="{ active: controller.state.section === item.id }" @click="select(item.id)">
        <component :is="item.icon" :size="15" />{{ item.label }}
        <span v-if="item.id === 'governance' && pending.length" class="badge badge-accent">{{ pending.length }}</span>
      </button>
    </aside>

    <div class="enterprise-content">
      <div v-if="controller.state.error" class="error-banner" role="alert"><span>{{ controller.state.error }}</span><button class="button button-emphasis" @click="controller.load">Retry</button></div>
      <template v-if="controller.state.section === 'dashboard'">
        <header class="enterprise-hero">
          <div><span class="eyebrow">Governed AgentOS</span><h2>AI assets under one enterprise control plane.</h2><p>Identity references, policy decisions, approvals, lifecycle, cost and operations remain visible without exposing credentials or prompt bodies.</p></div>
          <span class="badge badge-accent">{{ pending.length }} decisions pending</span>
        </header>
        <div class="studio-metrics">
          <article><span>Production Assets</span><strong>{{ controller.state.snapshot.assets.filter((item) => item.state === 'PRODUCTION').length }}</strong><small>{{ controller.state.snapshot.assets.length }} registered</small></article>
          <article><span>High Risk</span><strong>{{ highRisk.length }}</strong><small>Score 70 or above</small></article>
          <article><span>Principals</span><strong>{{ controller.state.snapshot.principals.length }}</strong><small>External IdP bindings</small></article>
          <article><span>Policy Decisions</span><strong>{{ controller.state.snapshot.audits.length }}</strong><small>Current audit window</small></article>
        </div>
        <div class="enterprise-dashboard-grid">
          <PanelShell title="Governance Queue" :count="pending.length">
            <article v-for="asset in pending" :key="asset.id" class="governance-card">
              <header><span class="badge badge-accent">Risk {{ asset.riskScore }}</span><span class="badge badge-neutral">{{ asset.classification }}</span></header>
              <h3>{{ asset.name }}</h3><p>{{ asset.assetType }} · {{ asset.key }}@{{ asset.assetVersion }} · owner {{ asset.ownerSubject }}</p>
              <footer><span>{{ asset.approvals }}/{{ asset.requiredApprovals }} approvals</span><button class="button button-primary" :disabled="controller.state.mutating === asset.id" @click="controller.approve(asset.id)">Approve</button></footer>
            </article>
            <EmptyState v-if="!pending.length" title="Governance queue clear" detail="Assets submitted for independent approval will appear here." />
          </PanelShell>
          <PanelShell title="Cost Ledger" :count="controller.state.snapshot.costs.length">
            <div class="cost-summary"><article v-for="total in costTotals" :key="total.currency"><span>{{ total.currency }}</span><strong>{{ total.micros }}</strong><small>micros, exact integer</small></article></div>
            <EmptyState v-if="!costTotals.length" title="No cost events" detail="Idempotent model and Agent usage events will appear here." />
          </PanelShell>
        </div>
      </template>

      <PanelShell v-else-if="controller.state.section === 'organization'" title="Organization Directory" :count="controller.state.snapshot.organizations.length">
        <div class="asset-grid"><article v-for="item in controller.state.snapshot.organizations" :key="item.id" class="info-card"><span class="badge badge-neutral">{{ item.state }}</span><h3>{{ item.name }}</h3><p>{{ item.key }}</p><small>{{ item.members }} principals · {{ item.assets }} AI assets</small></article></div>
        <EmptyState v-if="!controller.state.snapshot.organizations.length" title="No organizations" detail="Tenant-scoped organizations from Platform Runtime appear here." />
      </PanelShell>

      <PanelShell v-else-if="controller.state.section === 'identity'" title="Enterprise Identity" :count="controller.state.snapshot.principals.length">
        <div class="enterprise-table-wrap"><table class="visual-table"><thead><tr><th>Principal</th><th>Provider</th><th>Roles</th><th>Groups</th><th>State</th></tr></thead><tbody><tr v-for="item in controller.state.snapshot.principals" :key="item.id"><td><strong>{{ item.displayName }}</strong><small>{{ item.externalSubject }}</small></td><td>{{ item.provider }}</td><td>{{ item.roles.join(', ') || '—' }}</td><td>{{ item.groups.join(', ') || '—' }}</td><td><span class="badge badge-neutral">{{ item.state }}</span></td></tr></tbody></table></div>
        <EmptyState v-if="!controller.state.snapshot.principals.length" title="No identity bindings" detail="Verified deployment IdP subjects can be mapped to enterprise principals." />
      </PanelShell>

      <PanelShell v-else-if="controller.state.section === 'assets' || controller.state.section === 'governance'" :title="controller.state.section === 'assets' ? 'AI Asset Registry' : 'Governance Center'" :count="(controller.state.section === 'governance' ? pending : controller.state.snapshot.assets).length">
        <div class="enterprise-table-wrap"><table class="visual-table"><thead><tr><th>Asset</th><th>Classification</th><th>Risk</th><th>Lifecycle</th><th>Approval</th><th>Action</th></tr></thead><tbody><tr v-for="asset in (controller.state.section === 'governance' ? pending : controller.state.snapshot.assets)" :key="asset.id"><td><strong>{{ asset.name }}</strong><small>{{ asset.assetType }} · {{ asset.key }}@{{ asset.assetVersion }}</small></td><td>{{ asset.classification }}</td><td><span class="risk-score" :class="{ high: asset.riskScore >= 70 }">{{ asset.riskScore }}</span></td><td><span class="badge badge-neutral">{{ asset.state }}</span></td><td>{{ asset.approvals }}/{{ asset.requiredApprovals }}</td><td><div class="visual-actions"><button v-if="asset.state === 'REVIEWED'" class="button button-primary" @click="controller.approve(asset.id)">Approve</button><button v-if="asset.state === 'APPROVED'" class="button button-primary" @click="controller.transition(asset.id, 'PRODUCTION')">Promote</button><button v-if="asset.state === 'PRODUCTION'" class="button button-danger" @click="controller.transition(asset.id, 'SUSPENDED')">Suspend</button></div></td></tr></tbody></table></div>
        <EmptyState v-if="!(controller.state.section === 'governance' ? pending : controller.state.snapshot.assets).length" title="No governed assets" detail="Versioned Agents, Models, Prompts, Workflows and Knowledge appear here." />
      </PanelShell>

      <PanelShell v-else-if="controller.state.section === 'policies'" title="Policy Center" :count="controller.state.snapshot.policies.length">
        <div class="asset-grid"><article v-for="item in controller.state.snapshot.policies" :key="item.id" class="info-card"><span class="badge badge-neutral">{{ item.state }}</span><h3>{{ item.name }}</h3><p>{{ item.key }} · {{ item.scope }}</p><small>{{ item.rules }} deterministic rules</small></article></div>
        <EmptyState v-if="!controller.state.snapshot.policies.length" title="No enterprise policies" detail="Platform default-deny policies will appear here." />
      </PanelShell>

      <PanelShell v-else-if="controller.state.section === 'cost'" title="Cost and Usage" :count="controller.state.snapshot.costs.length">
        <div class="enterprise-table-wrap"><table class="visual-table"><thead><tr><th>Event</th><th>Scope</th><th>Model</th><th>Usage</th><th>Exact cost</th><th>Occurred</th></tr></thead><tbody><tr v-for="item in controller.state.snapshot.costs" :key="item.id"><td>{{ item.eventKey }}</td><td>{{ item.project ?? item.agent ?? 'tenant' }}</td><td>{{ item.model ?? '—' }}</td><td>{{ item.inputTokens }} in / {{ item.outputTokens }} out</td><td>{{ item.amountMicros }} {{ item.currency }} micros</td><td>{{ item.occurredAt }}</td></tr></tbody></table></div>
        <EmptyState v-if="!controller.state.snapshot.costs.length" title="No usage ledger" detail="Exact integer cost events are grouped without billing claims." />
      </PanelShell>

      <PanelShell v-else-if="controller.state.section === 'audit'" title="Immutable Audit" :count="controller.state.snapshot.audits.length">
        <ol class="activity-list"><li v-for="item in controller.state.snapshot.audits" :key="item.id"><i :class="{ denied: item.decision !== 'ALLOWED' }" /><div><strong>{{ item.action }} · {{ item.resource }}</strong><span>{{ item.subject }} · {{ item.decision }} · {{ item.reason }}</span></div><time>{{ item.occurredAt }}</time></li></ol>
        <EmptyState v-if="!controller.state.snapshot.audits.length" title="No policy decisions" detail="Allowed and denied enterprise actions are audited by Platform Runtime." />
      </PanelShell>

      <PanelShell v-else-if="controller.state.section === 'operation'" title="Enterprise Operations" :count="controller.state.snapshot.operations.length">
        <div class="operation-grid"><article v-for="item in controller.state.snapshot.operations" :key="item.component"><i :class="{ healthy: item.state === 'HEALTHY' }" /><span><strong>{{ item.component }}</strong><small>{{ item.message }}</small></span><span class="badge badge-neutral">{{ item.state }}</span></article></div>
        <EmptyState v-if="!controller.state.snapshot.operations.length" title="No operation signals" detail="Runtime health checks and bounded status messages will appear here." />
      </PanelShell>

      <PanelShell v-else title="Enterprise Settings">
        <div class="settings-list"><div><span><strong>Identity trust</strong><small>External verification remains owned by the deployment IdP adapter</small></span><span class="badge badge-neutral">Reference only</span></div><div><span><strong>Secrets</strong><small>Credentials and prompt bodies are never rendered in this workspace</small></span><span class="badge badge-accent">Redacted</span></div><div><span><strong>Billing</strong><small>Cost ledger is usage evidence, not invoice settlement</small></span><span class="badge badge-neutral">Not configured</span></div></div>
      </PanelShell>
    </div>
  </section>
</template>
