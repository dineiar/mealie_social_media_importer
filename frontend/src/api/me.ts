import type { ApiClient } from "./client";
import type { Me } from "./types/Me";

// The backend gives Mealie 10 s for the user lookup and 10 s for the group lookup, one after the other.
// Waiting longer means the client only gives up on a backend that has stopped answering.
const ME_TIMEOUT_MS = 25_000;

type Fields = Record<string, unknown>;

function isFields(value: unknown): value is Fields {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isTextOrNull(value: unknown): value is string | null {
  return typeof value === "string" || value === null;
}

export function parseMe(body: unknown): Me {
  if (isFields(body) && isFields(body.ai)) {
    const { id, username, full_name, group_slug, ai } = body;
    if (
      typeof id === "string" &&
      isTextOrNull(username) &&
      isTextOrNull(full_name) &&
      typeof group_slug === "string" &&
      typeof ai.enabled === "boolean" &&
      typeof ai.image_enabled === "boolean"
    ) {
      return {
        id,
        username,
        full_name,
        group_slug,
        ai: { enabled: ai.enabled, image_enabled: ai.image_enabled },
      };
    }
  }
  throw new TypeError("GET /api/me answered with an unexpected shape");
}

export async function fetchMe(
  api: ApiClient,
  signal: AbortSignal,
): Promise<Me> {
  return parseMe(
    await api<unknown>("/api/me", { signal, timeoutMs: ME_TIMEOUT_MS }),
  );
}
