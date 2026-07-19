import { invoke } from "@tauri-apps/api/core";
import type { UiPreference } from "./types";

export async function loadPreferences(): Promise<UiPreference[]> {
  try {
    return await invoke<UiPreference[]>("list_preferences");
  } catch {
    return [];
  }
}

export async function savePreference(
  key: string,
  kind: string,
  value: unknown,
  expectedVersion?: number,
): Promise<UiPreference> {
  return invoke<UiPreference>("save_preference", {
    request: { key, kind, value, expectedVersion },
  });
}
