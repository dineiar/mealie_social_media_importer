import { HttpResponse, http } from "msw";
import { describe, expect, it } from "vitest";
import { server, takeUnhandledRequests } from "./server";

describe("the MSW test server", () => {
  it("fails a request without a handler and records it for the test setup to fail on", async () => {
    const response = await fetch("/api/nothing-handles-this").catch(() => null);

    expect(response?.ok).not.toBe(true);
    const unhandled = takeUnhandledRequests();
    expect(unhandled).toHaveLength(1);
    expect(unhandled[0]).toContain("GET");
    expect(unhandled[0]).toContain("/api/nothing-handles-this");
  });

  it("records nothing for a request with a handler", async () => {
    server.use(http.get("/api/known", () => HttpResponse.json({ ok: true })));

    await fetch("/api/known");

    expect(takeUnhandledRequests()).toEqual([]);
  });
});
