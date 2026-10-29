import { ApiError } from "../api/error";
import type { SessionEvent } from "./session";

export function isKeyRejection(error: unknown): boolean {
  return error instanceof ApiError && error.status === 401;
}

function isMealieDown(error: unknown): boolean {
  return error instanceof ApiError && error.kind === "mealie_unreachable";
}

export function unreachableFor(error: unknown): SessionEvent {
  return {
    type: "unreachable",
    cause: isMealieDown(error) ? "mealie" : "backend",
  };
}
