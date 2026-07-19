<script setup lang="ts">
import { computed, onMounted } from "vue";
import { Activity, Bell, Bot, CheckSquare, FolderKanban, Home, Library, ShieldCheck, Users, X } from "lucide-vue-next";
import { HttpCollaborationApi } from "../collaboration-api";
import { createCollaborationController } from "../collaboration-controller";
import type { CollaborationSection } from "../collaboration-types";
import EmptyState from "./EmptyState.vue";
import PanelShell from "./PanelShell.vue";

const controller = createCollaborationController(new HttpCollaborationApi());
const navigation = [
  { id: "home", label: "Home", icon: Home }, { id: "projects", label: "Projects", icon: FolderKanban },
  { id: "agents", label: "Agents", icon: Bot }, { id: "team", label: "Team", icon: Users },
  { id: "tasks", label: "Tasks", icon: CheckSquare }, { id: "reviews", label: "Reviews", icon: ShieldCheck },
  { id: "approvals", label: "Approvals", icon: ShieldCheck }, { id: "knowledge", label: "Knowledge", icon: Library },
  { id: "activity", label: "Activity", icon: Activity }, { id: "notifications", label: "Notifications", icon: Bell },
] as const;
const pending = computed(() => controller.state.snapshot.approvals.filter((item) => item.state === "PENDING"));
onMounted(controller.load);
function select(section: CollaborationSection) { controller.select(section); }
</script>

<template>
  <section class="collaboration-workspace">
    <aside class="collaboration-nav">
      <div class="studio-heading"><span class="eyebrow">Team Asset</span><h2>Collaboration</h2></div>
      <select v-model="controller.state.projectId" aria-label="Active project" @change="controller.selectProject(controller.state.projectId!)">
        <option v-if="!controller.state.snapshot.projects.length" :value="undefined">No project</option>
        <option v-for="project in controller.state.snapshot.projects" :key="project.id" :value="project.id">{{ project.name }}</option>
      </select>
      <button v-for="item in navigation" :key="item.id" :class="{ active: controller.state.section === item.id }" @click="select(item.id)">
        <component :is="item.icon" :size="15" />{{ item.label }}
        <span v-if="item.id === 'notifications' && controller.state.snapshot.notifications.length" class="badge badge-accent">{{ controller.state.snapshot.notifications.length }}</span>
      </button>
    </aside>
    <div class="collaboration-content">
      <div v-if="controller.state.error" class="error-banner"><span>{{ controller.state.error }}</span><button class="button button-emphasis" @click="controller.load">Retry</button></div>
      <template v-if="controller.state.section === 'home'">
        <header class="collaboration-hero"><div><span class="eyebrow">Today across the Agent team</span><h2>Shared work, visible decisions.</h2><p>Tasks, Agent outcomes, reviews, approvals and knowledge converge into one project activity stream.</p></div><span class="badge badge-accent">{{ pending.length }} waiting approval</span></header>
        <div class="studio-metrics">
          <article><span>Running Tasks</span><strong>{{ controller.state.snapshot.tasks.filter((item) => item.state === 'RUNNING').length }}</strong><small>{{ controller.state.snapshot.tasks.length }} total</small></article>
          <article><span>Shared Agents</span><strong>{{ controller.state.snapshot.agents.length }}</strong><small>Project registry</small></article>
          <article><span>Pending Reviews</span><strong>{{ pending.length }}</strong><small>Human decisions</small></article>
          <article><span>Knowledge</span><strong>{{ controller.state.snapshot.knowledge.length }}</strong><small>Versioned assets</small></article>
        </div>
        <div class="collaboration-home-grid">
          <PanelShell title="Activity Stream" :count="controller.state.snapshot.activity.length">
            <ol class="activity-list"><li v-for="item in controller.state.snapshot.activity" :key="item.id"><i /><div><strong>{{ item.summary }}</strong><span>{{ item.subject }} · {{ item.kind }}</span></div><time>{{ item.occurredAt }}</time></li></ol>
            <EmptyState v-if="!controller.state.snapshot.activity.length" title="No team activity" detail="Agent, task, review and knowledge events will converge here." />
          </PanelShell>
          <PanelShell title="Waiting Approval" :count="pending.length">
            <article v-for="review in pending" :key="review.id" class="approval-card"><span class="badge badge-accent">Risk {{ review.risk }}</span><h3>{{ review.taskTitle }}</h3><p>{{ review.summary }}</p><div><button class="button button-emphasis" :disabled="controller.state.deciding === review.id" @click="controller.decide(review.id, 'REJECT')"><X :size="13" />Request changes</button><button class="button button-primary" :disabled="controller.state.deciding === review.id" @click="controller.decide(review.id, 'APPROVE')">Approve</button></div></article>
            <EmptyState v-if="!pending.length" title="Approval queue clear" detail="High-risk Agent actions and reviews will wait here." />
          </PanelShell>
        </div>
      </template>

      <PanelShell v-else-if="controller.state.section === 'projects'" title="Project Center" :count="controller.state.snapshot.projects.length">
        <div class="asset-grid"><article v-for="project in controller.state.snapshot.projects" :key="project.id" class="info-card"><span class="badge badge-neutral">{{ project.state }}</span><h3>{{ project.name }}</h3><p>{{ project.members }} members · {{ project.agents }} Agents · {{ project.tasks }} tasks</p><small>Knowledge {{ project.knowledge }}</small></article></div>
        <EmptyState v-if="!controller.state.snapshot.projects.length" title="No shared projects" detail="Create a project through the Collaboration API." />
      </PanelShell>
      <PanelShell v-else-if="controller.state.section === 'agents'" title="Agent Registry" :count="controller.state.snapshot.agents.length">
        <div class="asset-grid"><article v-for="agent in controller.state.snapshot.agents" :key="agent.id" class="info-card"><span class="badge badge-accent">{{ agent.state }}</span><h3>{{ agent.name }}</h3><p>{{ agent.owner }} · {{ agent.model }}</p><small>Version {{ agent.version }}</small></article></div>
        <EmptyState v-if="!controller.state.snapshot.agents.length" title="No shared Agents" detail="Attach Studio Agent assets to this project." />
      </PanelShell>
      <PanelShell v-else-if="controller.state.section === 'team'" title="Team" :count="controller.state.snapshot.members.length">
        <div class="member-list"><article v-for="member in controller.state.snapshot.members" :key="member.id"><span class="member-avatar">{{ member.name.slice(0, 1).toUpperCase() }}</span><span><strong>{{ member.name }}</strong><small>{{ member.role }}</small></span><span class="badge badge-neutral">{{ member.state }}</span></article></div>
        <EmptyState v-if="!controller.state.snapshot.members.length" title="No project members" detail="Project membership controls collaboration actions." />
      </PanelShell>
      <PanelShell v-else-if="controller.state.section === 'tasks'" title="Task Center" :count="controller.state.snapshot.tasks.length">
        <div class="task-list"><article v-for="task in controller.state.snapshot.tasks" :key="task.id"><header><span>#{{ task.number }}</span><strong>{{ task.title }}</strong><span class="badge badge-neutral">{{ task.state }}</span></header><div class="progress-track"><i :style="{ transform: `scaleX(${task.progress / 100})` }" /></div><footer><span>{{ task.assignee }}</span><span>{{ task.progress }}%</span></footer></article></div>
        <EmptyState v-if="!controller.state.snapshot.tasks.length" title="No shared tasks" detail="Agent and human work will be tracked here." />
      </PanelShell>
      <PanelShell v-else-if="controller.state.section === 'reviews' || controller.state.section === 'approvals'" :title="controller.state.section === 'reviews' ? 'Review Center' : 'Approval Center'" :count="(controller.state.section === 'reviews' ? controller.state.snapshot.reviews : controller.state.snapshot.approvals).length">
        <div class="review-list"><article v-for="review in (controller.state.section === 'reviews' ? controller.state.snapshot.reviews : controller.state.snapshot.approvals)" :key="review.id"><header><span class="badge badge-accent">{{ review.risk }}</span><span class="badge badge-neutral">{{ review.state }}</span></header><h3>{{ review.taskTitle }}</h3><p>{{ review.summary }}</p><small>Created by {{ review.createdBy }} · Reviewer {{ review.reviewer ?? 'unassigned' }}</small><div v-if="review.state === 'PENDING'" class="review-actions"><button class="button button-emphasis" @click="controller.decide(review.id, 'REJECT')">Reject</button><button class="button button-primary" @click="controller.decide(review.id, 'APPROVE')">Approve</button></div></article></div>
        <EmptyState v-if="!(controller.state.section === 'reviews' ? controller.state.snapshot.reviews : controller.state.snapshot.approvals).length" title="Queue is empty" detail="Completed Agent tasks enter review here." />
      </PanelShell>
      <PanelShell v-else-if="controller.state.section === 'knowledge'" title="Knowledge Center" :count="controller.state.snapshot.knowledge.length">
        <div class="asset-grid"><article v-for="item in controller.state.snapshot.knowledge" :key="item.id" class="info-card"><span class="badge badge-neutral">{{ item.kind }}</span><h3>{{ item.title }}</h3><p>{{ item.summary }}</p><small>v{{ item.version }} · {{ item.state }}</small></article></div>
        <EmptyState v-if="!controller.state.snapshot.knowledge.length" title="No project knowledge" detail="Architecture, runbook, API and RCA assets will appear here." />
      </PanelShell>
      <PanelShell v-else :title="controller.state.section === 'notifications' ? 'Notification Center' : 'Activity Stream'" :count="(controller.state.section === 'notifications' ? controller.state.snapshot.notifications : controller.state.snapshot.activity).length">
        <ol class="activity-list"><li v-for="item in (controller.state.section === 'notifications' ? controller.state.snapshot.notifications : controller.state.snapshot.activity)" :key="item.id"><i /><div><strong>{{ item.summary }}</strong><span>{{ item.subject }} · {{ item.entityType }}</span></div><time>{{ item.occurredAt }}</time></li></ol>
        <EmptyState v-if="!(controller.state.section === 'notifications' ? controller.state.snapshot.notifications : controller.state.snapshot.activity).length" title="No activity" detail="Collaboration events will appear here." />
      </PanelShell>
    </div>
  </section>
</template>
