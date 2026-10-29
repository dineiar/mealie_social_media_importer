import { ApiError } from "../api/error";
import type { SessionEvent } from "./session";

function isMealieDown(error: unknown): boolean {
  return error instanceof ApiError && error.kind === "mealie_unreachable";
}

export function eventForFailure(error: unknown): SessionEvent {
  return {
    type: "unreachable",
    cause: isMealieDown(error) ? "mealie" : "backend",
  };
}
