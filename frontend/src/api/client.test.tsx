import { QueryClientProvider, useQuery } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import { describe, expect, it } from "vitest";
import { server } from "../test/server";
import { createApiClient, type KeySource } from "./client";
import { ApiError, UNKNOWN_ERROR_KIND } from "./error";
import { createQueryClient } from "./queryClient";

const KEY = "secret-key-123";
const withKey: KeySource = { getKey: () => KEY };
const withoutKey: KeySource = { getKey: () => null };

interface ConfigResponse {
  version: string;
}

async function failureOf(promise: Promise<unknown>): Promise<ApiError> {
  const error = await promise.then(
    () => null,
    (e: unknown) => e,
  );
  if (!(error instanceof ApiError)) {
    throw new Error("expected the request to fail with an ApiError");
  }
  return error;
}

describe("createApiClient", () => {
  it("sends the key from the key source as a bearer token", async () => {
    let authorization: string | null = null;
    server.use(
      http.get("/api/me", ({ request }) => {
        authorization = request.headers.get("Authorization");
        return HttpResponse.json({ username: "ana" });
      }),
    );

    const body = await createApiClient(withKey)<{ username: string }>(
      "/api/me",
    );

    expect(body).toEqual({ username: "ana" });
    expect(authorization).toBe(`Bearer ${KEY}`);
  });

  it("sends no Authorization header when the key source has no key", async () => {
    let authorization: string | null = "unset";
    server.use(
      http.get("/api/config", ({ request }) => {
        authorization = request.headers.get("Authorization");
        return HttpResponse.json({ version: "1.0.0" });
      }),
    );

    await createApiClient(withoutKey)("/api/config");

    expect(authorization).toBeNull();
  });

  it("sends a JSON body with its content type", async () => {
    let received: unknown = null;
    let contentType: string | null = null;
    server.use(
      http.post("/api/imports", async ({ request }) => {
        received = await request.json();
        contentType = request.headers.get("Content-Type");
        return HttpResponse.json({ id: "1" }, { status: 202 });
      }),
    );

    await createApiClient(withKey)("/api/imports", {
      method: "POST",
      json: { url: "https://www.instagram.com/reel/C9xYz12AbCd/" },
    });

    expect(received).toEqual({
      url: "https://www.instagram.com/reel/C9xYz12AbCd/",
    });
    expect(contentType).toBe("application/json");
  });

  it("turns an error response into an ApiError with the backend's kind and message", async () => {
    server.use(
      http.get("/api/me", () =>
        HttpResponse.json(
          { error: { kind: "unauthorized", message: "Key rejected" } },
          { status: 401 },
        ),
      ),
    );

    const error = await failureOf(createApiClient(withKey)("/api/me"));

    expect(error.status).toBe(401);
    expect(error.kind).toBe("unauthorized");
    expect(error.message).toBe("Key rejected");
  });

  it("falls back to the unknown kind when the error body is not JSON", async () => {
    server.use(
      http.get("/api/me", () =>
        HttpResponse.text("<h1>502 Bad Gateway</h1>", { status: 502 }),
      ),
    );

    const error = await failureOf(createApiClient(withKey)("/api/me"));

    expect(error.status).toBe(502);
    expect(error.kind).toBe(UNKNOWN_ERROR_KIND);
    expect(error.message).not.toContain(KEY);
  });

  it("falls back to the unknown kind when the error body has another shape", async () => {
    server.use(
      http.get("/api/me", () =>
        HttpResponse.json({ error: "nope" }, { status: 500 }),
      ),
    );

    const error = await failureOf(createApiClient(withKey)("/api/me"));

    expect(error.status).toBe(500);
    expect(error.kind).toBe(UNKNOWN_ERROR_KIND);
  });
});

const publicApi = createApiClient(withoutKey);

function BackendVersion() {
  const { data, error } = useQuery({
    queryKey: ["config"],
    queryFn: () => publicApi<ConfigResponse>("/api/config"),
  });
  if (error) {
    return <p role="alert">{error instanceof ApiError ? error.kind : "?"}</p>;
  }
  return <p>{data ? `Version ${data.version}` : "Loading"}</p>;
}

function renderBackendVersion() {
  render(
    <QueryClientProvider client={createQueryClient()}>
      <BackendVersion />
    </QueryClientProvider>,
  );
}

describe("a component using the client through TanStack Query", () => {
  it("shows data from the API", async () => {
    server.use(
      http.get("/api/config", () => HttpResponse.json({ version: "1.2.3" })),
    );

    renderBackendVersion();

    expect(screen.getByText("Loading")).toBeInTheDocument();
    expect(await screen.findByText("Version 1.2.3")).toBeInTheDocument();
  });

  it("shows the error kind when the API refuses", async () => {
    server.use(
      http.get("/api/config", () =>
        HttpResponse.json(
          { error: { kind: "forbidden", message: "No" } },
          { status: 403 },
        ),
      ),
    );

    renderBackendVersion();

    expect(await screen.findByRole("alert")).toHaveTextContent("forbidden");
  });
});
