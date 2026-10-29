import { describe, expect, it } from "vitest";
import { ApiError, UNKNOWN_ERROR_KIND } from "../api/error";
import { eventForFailure } from "./classifyFailure";

describe("eventForFailure", () => {
  it("blames Mealie when the backend says it can't reach it", () => {
    expect(
      eventForFailure(new ApiError(502, "mealie_unreachable", "Down")),
    ).toEqual({ type: "unreachable", cause: "mealie" });
  });

  it.each([
    [
      "nginx's HTML 502 for a down backend",
      new ApiError(502, UNKNOWN_ERROR_KIND, "Bad Gateway"),
    ],
    ["a backend timeout", new ApiError(504, "timeout", "Too slow")],
    ["a backend bug", new ApiError(500, UNKNOWN_ERROR_KIND, "Oops")],
    ["a 403", new ApiError(403, "forbidden", "No")],
    ["a 404", new ApiError(404, "not_found", "No")],
    ["a failed fetch", new TypeError("Failed to fetch")],
    ["a 200 that isn't JSON", new SyntaxError("Unexpected token <")],
    ["a 200 that isn't a user", new TypeError("unexpected shape")],
    [
      "a 401, which the client's onUnauthorized hook has already turned into a rejection",
      new ApiError(401, "unauthorized", "Key rejected"),
    ],
  ])("blames the importer for %s", (_name, error) => {
    expect(eventForFailure(error)).toEqual({
      type: "unreachable",
      cause: "backend",
    });
  });
});
