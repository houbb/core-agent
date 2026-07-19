import { reactive } from "vue";
import type { StudioApi } from "./studio-api";
import type { CreateAgentInput, StudioSection, StudioSnapshot } from "./studio-types";

const emptyStudio = (): StudioSnapshot => ({
  agents: [], workflows: [], prompts: [], memories: [], capabilities: [], knowledge: [], traces: [], models: [], panels: [],
});

export function createStudioController(api: StudioApi) {
  const state = reactive({
    section: "home" as StudioSection,
    loading: false,
    saving: false,
    error: "",
    snapshot: emptyStudio(),
  });

  async function load() {
    state.loading = true;
    state.error = "";
    try {
      state.snapshot = await api.load();
    } catch (error) {
      state.error = error instanceof Error ? error.message : "Unable to load Studio";
    } finally {
      state.loading = false;
    }
  }

  async function createAgent(input: CreateAgentInput) {
    if (state.saving) return;
    state.saving = true;
    state.error = "";
    try {
      const agent = await api.createAgent(input);
      const index = state.snapshot.agents.findIndex((item) => item.id === agent.id);
      if (index >= 0) state.snapshot.agents[index] = agent;
      else state.snapshot.agents.unshift(agent);
      state.section = "agents";
    } catch (error) {
      state.error = error instanceof Error ? error.message : "Unable to save Agent";
      throw error;
    } finally {
      state.saving = false;
    }
  }

  function select(section: StudioSection) {
    state.section = section;
  }

  return { state, load, createAgent, select };
}
