import { reactive } from "vue";
import type { EcosystemApi } from "./ecosystem-api";
import type { EcosystemSection, EcosystemSnapshot } from "./ecosystem-types";

const empty = (): EcosystemSnapshot => ({ packages: [], publishers: [], installs: [], reviews: [], sdks: [], updates: [] });
export function createEcosystemController(api: EcosystemApi) {
  const state = reactive({ section: "marketplace" as EcosystemSection, loading: false, mutating: "", error: "", snapshot: empty() });
  async function load() { state.loading = true; state.error = ""; try { state.snapshot = await api.load(); } catch (error) { state.error = error instanceof Error ? error.message : "Unable to load ecosystem"; } finally { state.loading = false; } }
  async function mutate(packageId: string, operation: () => Promise<void>) { if (state.mutating) return; state.mutating = packageId; state.error = ""; try { await operation(); await load(); } catch (error) { state.error = error instanceof Error ? error.message : "Ecosystem action failed"; throw error; } finally { state.mutating = ""; } }
  const install = (packageId: string) => mutate(packageId, () => api.install(packageId));
  const submit = (packageId: string) => mutate(packageId, () => api.submit(packageId));
  function select(section: EcosystemSection) { state.section = section; }
  return { state, load, install, submit, select };
}
