import { reactive } from "vue";
import type { CollaborationApi } from "./collaboration-api";
import type { CollaborationSection, CollaborationSnapshot } from "./collaboration-types";

const empty = (): CollaborationSnapshot => ({ projects: [], agents: [], members: [], tasks: [], reviews: [], approvals: [], knowledge: [], activity: [], notifications: [] });

export function createCollaborationController(api: CollaborationApi) {
  const state = reactive({ section: "home" as CollaborationSection, loading: false, deciding: "", error: "", projectId: undefined as string | undefined, snapshot: empty() });
  async function load() {
    state.loading = true; state.error = "";
    try { state.snapshot = await api.load(state.projectId); state.projectId ??= state.snapshot.projects[0]?.id; }
    catch (error) { state.error = error instanceof Error ? error.message : "Unable to load collaboration"; }
    finally { state.loading = false; }
  }
  async function decide(reviewId: string, decision: "APPROVE" | "REJECT") {
    if (state.deciding) return;
    state.deciding = reviewId; state.error = "";
    try { await api.decideReview(reviewId, decision, decision === "APPROVE" ? "Approved in Collaboration Workspace" : "Changes requested in Collaboration Workspace"); await load(); }
    catch (error) { state.error = error instanceof Error ? error.message : "Unable to decide review"; throw error; }
    finally { state.deciding = ""; }
  }
  function select(section: CollaborationSection) { state.section = section; }
  function selectProject(projectId: string) { state.projectId = projectId; return load(); }
  return { state, load, decide, select, selectProject };
}
