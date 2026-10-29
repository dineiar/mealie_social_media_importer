import { readApiError } from "./error";

export interface KeySource {
  getKey(): string | null;
}

export interface ApiRequestInit extends Omit<RequestInit, "body"> {
  json?: unknown;
}

export type ApiClient = <T>(path: string, init?: ApiRequestInit) => Promise<T>;

export function createApiClient(keySource: KeySource): ApiClient {
  return async function apiFetch<T>(
    path: string,
    { json, headers: initHeaders, ...init }: ApiRequestInit = {},
  ): Promise<T> {
    const headers = new Headers(initHeaders);
    headers.set("Accept", "application/json");
    const key = keySource.getKey();
    if (key !== null) {
      headers.set("Authorization", `Bearer ${key}`);
    }
    if (json !== undefined) {
      headers.set("Content-Type", "application/json");
    }

    const response = await fetch(path, {
      ...init,
      headers,
      body: json === undefined ? null : JSON.stringify(json),
    });
    if (!response.ok) {
      throw await readApiError(response);
    }
    return response.json();
  };
}
