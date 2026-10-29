import { QueryClientProvider, useQuery } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import { describe, expect, it } from "vitest";
import { AuthProvider, useAuth } from "../auth/AuthProvider";
import {
  anotherTab,
  flushEffects,
  KEY_TEXT,
  mealieUnreachableResponse,
  meIs,
  meRequests,
  storedValues,
  unauthorizedResponse,
} from "../test/fixtures";
import { server } from "../test/server";
import { createQueryClient } from "./queryClient";

function Imports() {
  const { api } = useAuth();
  const { data } = useQuery({
    queryKey: ["imports"],
    queryFn: () => api<string[]>("/api/imports"),
    retry: false,
  });
  return <p>{data ? `${data.length} imports` : "no imports yet"}</p>;
}

function ImportScreen() {
  const { session } = useAuth();
  const takesCalls =
    session.kind === "logged_in" || session.kind === "unreachable";
  return (
    <>
      <p role="status">{session.kind}</p>
      {takesCalls && <Imports />}
    </>
  );
}

function renderImportScreen() {
  const queryClient = createQueryClient();
  render(
    <QueryClientProvider client={queryClient}>
      <AuthProvider>
        <ImportScreen />
      </AuthProvider>
    </QueryClientProvider>,
  );
  return queryClient;
}

const importsAnswer = (
  respond: (request: Request) => Response | Promise<Response>,
) => server.use(http.get("/api/imports", ({ request }) => respond(request)));

describe("a call made through the session's API client", () => {
  it("ends a logged in session when the call gets a 401", async () => {
    localStorage.setItem("mealie-importer.api-key", KEY_TEXT);
    meIs();
    importsAnswer(unauthorizedResponse);

    const queryClient = renderImportScreen();

    expect(await screen.findByText("rejected")).toBeInTheDocument();
    await flushEffects();
    expect(storedValues()).toEqual({ local: [], tab: [] });
    expect(queryClient.getQueryCache().getAll()).toEqual([]);
  });

  it("ends an unreachable session when the call gets a 401", async () => {
    localStorage.setItem("mealie-importer.api-key", KEY_TEXT);
    meRequests(mealieUnreachableResponse);
    importsAnswer(unauthorizedResponse);

    renderImportScreen();

    expect(await screen.findByText("rejected")).toBeInTheDocument();
    await flushEffects();
    expect(storedValues()).toEqual({ local: [], tab: [] });
  });

  it.each([403, 404, 500])(
    "keeps the session when the call gets a %i",
    async (status) => {
      localStorage.setItem("mealie-importer.api-key", KEY_TEXT);
      meIs();
      importsAnswer(() => HttpResponse.json({}, { status }));

      renderImportScreen();

      await screen.findByText("no imports yet");
      await flushEffects();
      expect(screen.getByRole("status")).toHaveTextContent("logged_in");
      expect(storedValues().local).toEqual([KEY_TEXT]);
    },
  );

  it("ignores a 401 for a key the tab has since replaced", async () => {
    const otherKey = "another-key-9876543210";
    localStorage.setItem("mealie-importer.api-key", KEY_TEXT);
    meIs();
    let refuseOldKey: () => void = () => {};
    const oldKeyRefused = new Promise<void>((resolve) => {
      refuseOldKey = resolve;
    });
    importsAnswer(async (request) => {
      if (request.headers.get("Authorization") === `Bearer ${otherKey}`) {
        return HttpResponse.json([]);
      }
      await oldKeyRefused;
      return unauthorizedResponse();
    });
    renderImportScreen();
    await screen.findByText("logged_in");

    anotherTab.remembers(otherKey);
    await screen.findByText("checking");
    await screen.findByText("logged_in");
    refuseOldKey();
    await flushEffects();

    expect(screen.getByRole("status")).toHaveTextContent("logged_in");
    expect(storedValues().local).toEqual([otherKey]);
  });
});
