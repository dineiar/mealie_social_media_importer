import { HttpResponse, http } from "msw";
import { describe, expect, it } from "vitest";
import { expectNoUnhandledRequests, server } from "./server";

describe("the MSW test server", () => {
  it("fails a request without a handler and reports it", async () => {
    const response = await fetch("/api/nothing-handles-this").catch(() => null);

    expect(response?.ok).not.toBe(true);
    await expect(expectNoUnhandledRequests()).rejects.toThrow(
      "/api/nothing-handles-this",
    );
  });

  it("reports nothing for a request with a handler", async () => {
    server.use(http.get("/api/known", () => HttpResponse.json({ ok: true })));

    await fetch("/api/known");

    await expect(expectNoUnhandledRequests()).resolves.toBeUndefined();
  });

  it("reports a request the test fired and never awaited", async () => {
    fetch("/api/fired-and-forgotten").catch(() => undefined);

    await expect(expectNoUnhandledRequests()).rejects.toThrow(
      "/api/fired-and-forgotten",
    );
  });

  it("reports each request once, so the next test starts clean", async () => {
    await fetch("/api/seen-once").catch(() => undefined);
    await expect(expectNoUnhandledRequests()).rejects.toThrow();

    await expect(expectNoUnhandledRequests()).resolves.toBeUndefined();
  });
});
