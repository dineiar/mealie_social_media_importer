import { describe, expect, it } from "vitest";
import { ApiError } from "./error";
import { shouldRetry } from "./queryClient";

describe("shouldRetry", () => {
  it("does not retry a 4xx answer", () => {
    expect(shouldRetry(0, new ApiError(401, "unauthorized", "No"))).toBe(false);
  });

  it("retries a 5xx answer a limited number of times", () => {
    const error = new ApiError(503, "unavailable", "Down");
    expect(shouldRetry(0, error)).toBe(true);
    expect(shouldRetry(2, error)).toBe(false);
  });

  it("retries a network failure", () => {
    expect(shouldRetry(0, new TypeError("Failed to fetch"))).toBe(true);
  });
});
