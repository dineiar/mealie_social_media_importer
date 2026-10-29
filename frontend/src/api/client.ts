import { readApiError } from "./error";

export interface ApiClientOptions {
  getKey(): string | null;
  onUnauthorized?(): void;
}

export interface ApiRequestInit extends Omit<RequestInit, "body"> {
  json?: unknown;
}

export type ApiClient = <T>(path: string, init?: ApiRequestInit) => Promise<T>;

export function createApiClient(options: ApiClientOptions): ApiClient {
  return async function apiFetch<T>(
    path: string,
    { json, headers: initHeaders, ...init }: ApiRequestInit = {},
  ): Promise<T> {
    const headers = new Headers(initHeaders);
    headers.set("Accept", "application/json");
    const key = options.getKey();
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
      const error = await readApiError(response);
      if (error.status === 401) {
        options.onUnauthorized?.();
      }
      throw error;
    }
    return response.json();
  };
}
