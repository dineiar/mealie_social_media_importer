import { readApiError } from "./error";

export interface ApiClientOptions {
  getKey(): string | null;
  onUnauthorized?(): void;
}

export interface ApiRequestInit extends Omit<RequestInit, "body"> {
  json?: unknown;
  timeoutMs?: number;
}

export type ApiClient = <T>(path: string, init?: ApiRequestInit) => Promise<T>;

export function createApiClient(options: ApiClientOptions): ApiClient {
  return async function apiFetch<T>(
    path: string,
    {
      json,
      headers: initHeaders,
      timeoutMs,
      signal,
      ...init
    }: ApiRequestInit = {},
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

    const deadline = new AbortController();
    const timer =
      timeoutMs === undefined
        ? undefined
        : setTimeout(
            () =>
              deadline.abort(
                new DOMException(
                  `No answer within ${timeoutMs} ms`,
                  "TimeoutError",
                ),
              ),
            timeoutMs,
          );
    try {
      const response = await fetch(path, {
        ...init,
        signal: AbortSignal.any(
          signal ? [signal, deadline.signal] : [deadline.signal],
        ),
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
      return await response.json();
    } finally {
      clearTimeout(timer);
    }
  };
}
