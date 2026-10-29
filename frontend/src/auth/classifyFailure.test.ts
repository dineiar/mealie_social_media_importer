import { describe, expect, it } from "vitest";
import { ApiError, UNKNOWN_ERROR_KIND } from "../api/error";
import { isKeyRejection, unreachableFor } from "./classifyFailure";

describe("isKeyRejection", () => {
  it("is true for a 401", () => {
    expect(
      isKeyRejection(new ApiError(401, "unauthorized", "Key rejected")),
    ).toBe(true);
  });

  it.each([
    ["a 403", new ApiError(403, "forbidden", "No")],
    ["a 404", new ApiError(404, "not_found", "No")],
    ["a 502", new ApiError(502, "mealie_unreachable", "Down")],
    ["a failed fetch", new TypeError("Failed to fetch")],
  ])("is false for %s", (_name, error) => {
    expect(isKeyRejection(error)).toBe(false);
  });
});

describe("unreachableFor", () => {
  it("blames Mealie when the backend says it can't reach it", () => {
    expect(
      unreachableFor(new ApiError(502, "mealie_unreachable", "Down")),
    ).toEqual({ type: "unreachable", cause: "mealie" });
  });

  it.each([
    [
      "nginx's HTML 502 for a down backend",
      new ApiError(502, UNKNOWN_ERROR_KIND, "Bad Gateway"),
    ],
    ["a backend timeout", new ApiError(504, "timeout", "Too slow")],
    ["a backend bug", new ApiError(500, UNKNOWN_ERROR_KIND, "Oops")],
    ["a failed fetch", new TypeError("Failed to fetch")],
    ["a 200 that isn't JSON", new SyntaxError("Unexpected token <")],
    ["a 200 that isn't a user", new TypeError("unexpected shape")],
  ])("blames the importer for %s", (_name, error) => {
    expect(unreachableFor(error)).toEqual({
      type: "unreachable",
      cause: "backend",
    });
  });
});
