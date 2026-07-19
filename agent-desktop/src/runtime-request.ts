import { invoke } from "@tauri-apps/api/core";

const MAX_RESPONSE_BYTES = 2 * 1024 * 1024;

export async function runtimeRequest<T>(path: string, init?: RequestInit, baseUrl?: string): Promise<T> {
  if (baseUrl) {
    const response = await fetch(`${baseUrl}${path}`, init);
    if (!response.ok) throw new Error(`Agent Runtime returned ${response.status}`);
    const declared = Number(response.headers.get("content-length") ?? 0);
    if (declared > MAX_RESPONSE_BYTES) throw new Error("Runtime response exceeds 2 MiB");
    const text = await response.text();
    if (text.length > MAX_RESPONSE_BYTES) throw new Error("Runtime response exceeds 2 MiB");
    return text ? JSON.parse(text) as T : undefined as T;
  }

  let body: unknown;
  if (typeof init?.body === "string" && init.body) body = JSON.parse(init.body);
  return invoke<T>("runtime_request", {
    request: {
      path,
      method: init?.method?.toUpperCase() ?? "GET",
      body,
    },
  });
}
