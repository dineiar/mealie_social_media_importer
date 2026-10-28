import { ApiError } from "../api/error";
import type { SessionEvent } from "./session";

function isKeyRejection(error: unknown): boolean {
  return error instanceof ApiError && error.status === 401;
}

function isMealieDown(error: unknown): boolean {
  return error instanceof ApiError && error.kind === "mealie_unreachable";
}

export function eventForFailure(error: unknown): SessionEvent {
  if (isKeyRejection(error)) {
    return { type: "rejected" };
  }
  if (isMealieDown(error)) {
    return { type: "unreachable", cause: "mealie" };
  }
  return { type: "unreachable", cause: "backend" };
}
