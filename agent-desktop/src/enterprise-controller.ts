import { reactive } from "vue";
import type { EnterpriseApi } from "./enterprise-api";
import type { EnterpriseSection, EnterpriseSnapshot } from "./enterprise-types";

const empty = (): EnterpriseSnapshot => ({ organizations: [], principals: [], assets: [], policies: [], costs: [], audits: [], operations: [] });

export function createEnterpriseController(api: EnterpriseApi) {
  const state = reactive({ section: "dashboard" as EnterpriseSection, loading: false, mutating: "", error: "", snapshot: empty() });
  async function load() {
    state.loading = true; state.error = "";
    try { state.snapshot = await api.load(); }
    catch (error) { state.error = error instanceof Error ? error.message : "Unable to load enterprise governance"; }
    finally { state.loading = false; }
  }
  async function mutate(assetId: string, operation: () => Promise<void>) {
    if (state.mutating) return;
    state.mutating = assetId; state.error = "";
    try { await operation(); await load(); }
    catch (error) { state.error = error instanceof Error ? error.message : "Enterprise governance action failed"; throw error; }
    finally { state.mutating = ""; }
  }
  const approve = (assetId: string) => mutate(assetId, () => api.approveAsset(assetId, "Approved in Enterprise Governance Workspace"));
  const transition = (assetId: string, target: "PRODUCTION" | "SUSPENDED" | "RETIRED") => mutate(assetId, () => api.transitionAsset(assetId, target));
  function select(section: EnterpriseSection) { state.section = section; }
  return { state, load, approve, transition, select };
}
