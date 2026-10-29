import { QueryClientProvider, useQuery } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import { describe, expect, it } from "vitest";
import { AuthProvider, useAuth } from "../auth/AuthProvider";
import { STORAGE_KEY } from "../auth/keyStorage";
import {
  anotherTab,
  deferred,
  flushEffects,
  KEY_TEXT,
  mealieUnreachableResponse,
  meIs,
  meRequests,
  OTHER_KEY_TEXT,
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
  it.each([
    ["logged in", () => meIs()],
    ["unreachable", () => meRequests(mealieUnreachableResponse)],
  ])("ends a %s session when the call gets a 401", async (_name, answerMe) => {
    localStorage.setItem(STORAGE_KEY, KEY_TEXT);
    answerMe();
    importsAnswer(unauthorizedResponse);

    const queryClient = renderImportScreen();

    expect(await screen.findByText("rejected")).toBeInTheDocument();
    await flushEffects();
    expect(storedValues()).toEqual({ local: [], tab: [] });
    expect(queryClient.getQueryCache().getAll()).toEqual([]);
  });

  it("ends only this tab's own key when a call gets a 401, not the key another tab remembered", async () => {
    sessionStorage.setItem(STORAGE_KEY, KEY_TEXT);
    localStorage.setItem(STORAGE_KEY, OTHER_KEY_TEXT);
    meIs();
    importsAnswer(unauthorizedResponse);

    renderImportScreen();

    expect(await screen.findByText("rejected")).toBeInTheDocument();
    await flushEffects();
    expect(storedValues()).toEqual({ local: [OTHER_KEY_TEXT], tab: [] });
  });

  it.each([403, 404, 500])(
    "keeps the session when the call gets a %i",
    async (status) => {
      localStorage.setItem(STORAGE_KEY, KEY_TEXT);
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
    localStorage.setItem(STORAGE_KEY, KEY_TEXT);
    meIs();
    const { released: oldKeyRefused, release: refuseOldKey } = deferred();
    importsAnswer(async (request) => {
      if (request.headers.get("Authorization") === `Bearer ${OTHER_KEY_TEXT}`) {
        return HttpResponse.json([]);
      }
      await oldKeyRefused;
      return unauthorizedResponse();
    });
    renderImportScreen();
    await screen.findByText("logged_in");

    anotherTab.remembers(OTHER_KEY_TEXT);
    await screen.findByText("checking");
    await screen.findByText("logged_in");
    refuseOldKey();
    await flushEffects();

    expect(screen.getByRole("status")).toHaveTextContent("logged_in");
    expect(storedValues().local).toEqual([OTHER_KEY_TEXT]);
  });
});
