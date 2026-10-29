import { type QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse } from "msw";
import {
  type ComponentType,
  Fragment,
  type ReactNode,
  StrictMode,
} from "react";
import { describe, expect, it, vi } from "vitest";
import { createQueryClient } from "../api/queryClient";
import {
  aMe,
  anotherTab,
  flushEffects,
  holdMeAnswer,
  KEY,
  KEY_TEXT,
  mealieUnreachableResponse,
  meIs,
  meRequests,
  neverAnswers,
  OTHER_KEY_TEXT,
  storedValues,
  unauthorizedResponse,
} from "../test/fixtures";
import { AuthProvider, useAuth } from "./AuthProvider";
import { STORAGE_KEY } from "./keyStorage";
import { listenForLogOff } from "./logOffChannel";
import type { Session } from "./session";

function describeSession(session: Session): string {
  return session.kind === "unreachable"
    ? `unreachable by ${session.cause}`
    : session.kind;
}

function SessionProbe() {
  const { session, check, logOff } = useAuth();
  return (
    <>
      <p role="status">{describeSession(session)}</p>
      <button type="button" onClick={() => check({ key: KEY, remember: true })}>
        Check and remember
      </button>
      <button type="button" onClick={logOff}>
        Log off
      </button>
    </>
  );
}

function renderProvider(
  queryClient: QueryClient = createQueryClient(),
  Wrapper: ComponentType<{ children: ReactNode }> = Fragment,
) {
  render(
    <Wrapper>
      <QueryClientProvider client={queryClient}>
        <AuthProvider>
          <SessionProbe />
        </AuthProvider>
      </QueryClientProvider>
    </Wrapper>,
  );
}

describe("AuthProvider", () => {
  it("stays signed out and calls nothing when no key is stored", () => {
    renderProvider();

    expect(screen.getByRole("status")).toHaveTextContent("signed_out");
  });

  it("checks a typed key through /api/me as a bearer token", async () => {
    const { release, authorizations } = holdMeAnswer();
    renderProvider();

    await userEvent.click(
      screen.getByRole("button", { name: "Check and remember" }),
    );

    expect(screen.getByRole("status")).toHaveTextContent("checking");
    release();
    expect(await screen.findByText("logged_in")).toBeInTheDocument();
    expect(authorizations).toEqual([`Bearer ${KEY_TEXT}`]);
  });

  it("logs in under StrictMode, which aborts the first check before it answers", async () => {
    localStorage.setItem(STORAGE_KEY, KEY_TEXT);
    meIs();

    renderProvider(createQueryClient(), StrictMode);

    expect(await screen.findByText("logged_in")).toBeInTheDocument();
  });

  it("saves a typed key only once Mealie has accepted it", async () => {
    const { release } = holdMeAnswer();
    renderProvider();

    await userEvent.click(
      screen.getByRole("button", { name: "Check and remember" }),
    );
    await flushEffects();
    expect(screen.getByRole("status")).toHaveTextContent("checking");
    expect(storedValues()).toEqual({ local: [], tab: [] });

    release();
    await screen.findByText("logged_in");
    await flushEffects();
    expect(storedValues()).toEqual({ local: [KEY_TEXT], tab: [] });
  });

  it("stores nothing for a key Mealie rejects", async () => {
    meRequests(unauthorizedResponse);
    renderProvider();

    await userEvent.click(
      screen.getByRole("button", { name: "Check and remember" }),
    );

    expect(await screen.findByText("rejected")).toBeInTheDocument();
    await flushEffects();
    expect(storedValues()).toEqual({ local: [], tab: [] });
  });

  it.each([
    ["localStorage", () => localStorage],
    ["sessionStorage", () => sessionStorage],
  ])("logs in on load with the key from %s", async (_name, area) => {
    area().setItem(STORAGE_KEY, KEY_TEXT);
    const authorizations = meIs();

    renderProvider();

    expect(screen.getByRole("status")).toHaveTextContent("checking");
    expect(await screen.findByText("logged_in")).toBeInTheDocument();
    expect(authorizations).toEqual([`Bearer ${KEY_TEXT}`]);
  });

  it("clears a stored key that Mealie rejects on load", async () => {
    localStorage.setItem(STORAGE_KEY, KEY_TEXT);
    meRequests(unauthorizedResponse);

    renderProvider();

    expect(await screen.findByText("rejected")).toBeInTheDocument();
    await flushEffects();
    expect(storedValues()).toEqual({ local: [], tab: [] });
  });

  it("keeps the key and asks once when Mealie can't be reached", async () => {
    localStorage.setItem(STORAGE_KEY, KEY_TEXT);
    const authorizations = meRequests(mealieUnreachableResponse);

    renderProvider();

    expect(
      await screen.findByText("unreachable by mealie"),
    ).toBeInTheDocument();
    expect(authorizations).toHaveLength(1);
    await flushEffects();
    expect(storedValues()).toEqual({ local: [KEY_TEXT], tab: [] });
  });

  it.each([
    [
      "nginx answers 502 with HTML",
      () => HttpResponse.html("<h1>502 Bad Gateway</h1>", { status: 502 }),
    ],
    ["the connection fails", () => HttpResponse.error()],
    [
      "the page answers 200 with HTML instead of the API",
      () => HttpResponse.html("<!doctype html><title>App</title>"),
    ],
    ["the answer is 200 but not a user", () => HttpResponse.json({})],
    [
      "the answer is a 403",
      () =>
        HttpResponse.json(
          { error: { kind: "forbidden", message: "No" } },
          { status: 403 },
        ),
    ],
    [
      "the answer is a 404",
      () =>
        HttpResponse.json(
          { error: { kind: "not_found", message: "No" } },
          { status: 404 },
        ),
    ],
  ])(
    "keeps the key and blames the importer when %s",
    async (_name, respond) => {
      sessionStorage.setItem(STORAGE_KEY, KEY_TEXT);
      const authorizations = meRequests(respond);

      renderProvider();

      expect(
        await screen.findByText("unreachable by backend"),
      ).toBeInTheDocument();
      expect(authorizations).toHaveLength(1);
      await flushEffects();
      expect(storedValues()).toEqual({ local: [], tab: [KEY_TEXT] });
    },
  );

  it("gives up on a backend that never answers, and keeps the key", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    try {
      localStorage.setItem(STORAGE_KEY, KEY_TEXT);
      meRequests(neverAnswers);

      renderProvider();
      await act(() => vi.advanceTimersByTimeAsync(20_000));
      expect(screen.getByRole("status")).toHaveTextContent("checking");

      await act(() => vi.advanceTimersByTimeAsync(5_000));
      expect(screen.getByRole("status")).toHaveTextContent(
        "unreachable by backend",
      );
      expect(storedValues()).toEqual({ local: [KEY_TEXT], tab: [] });
    } finally {
      vi.useRealTimers();
    }
  });

  it("recovers when a check is repeated after the outage", async () => {
    localStorage.setItem(STORAGE_KEY, KEY_TEXT);
    let up = false;
    meRequests(() =>
      up ? HttpResponse.json(aMe()) : mealieUnreachableResponse(),
    );
    renderProvider();
    await screen.findByText("unreachable by mealie");

    up = true;
    await userEvent.click(
      screen.getByRole("button", { name: "Check and remember" }),
    );

    expect(await screen.findByText("logged_in")).toBeInTheDocument();
  });

  it("forgets the key from both storages and empties the query cache on log off", async () => {
    meIs();
    const queryClient = createQueryClient();
    renderProvider(queryClient);
    await userEvent.click(
      screen.getByRole("button", { name: "Check and remember" }),
    );
    await screen.findByText("logged_in");
    queryClient.setQueryData(["imports"], [{ id: "earlier-import" }]);
    await flushEffects();
    expect(storedValues().local).toEqual([KEY_TEXT]);

    await userEvent.click(screen.getByRole("button", { name: "Log off" }));

    expect(screen.getByRole("status")).toHaveTextContent("signed_out");
    expect(storedValues()).toEqual({ local: [], tab: [] });
    expect(queryClient.getQueryCache().getAll()).toEqual([]);
  });

  it("empties the query cache when it starts checking the key another tab remembered", async () => {
    meIs();
    const queryClient = createQueryClient();
    renderProvider(queryClient);
    await userEvent.click(
      screen.getByRole("button", { name: "Check and remember" }),
    );
    await screen.findByText("logged_in");
    queryClient.setQueryData(["imports"], [{ id: "earlier-import" }]);

    anotherTab.remembers(OTHER_KEY_TEXT);

    expect(screen.getByRole("status")).toHaveTextContent("checking");
    expect(queryClient.getQueryCache().getAll()).toEqual([]);
    await screen.findByText("logged_in");
  });

  describe("telling other tabs", () => {
    const settle = () => new Promise((resolve) => setTimeout(resolve, 20));

    it("tells them once when the user logs off", async () => {
      meIs();
      renderProvider();
      const heard = vi.fn();
      const stop = listenForLogOff(heard);
      await userEvent.click(
        screen.getByRole("button", { name: "Check and remember" }),
      );
      await screen.findByText("logged_in");

      await userEvent.click(screen.getByRole("button", { name: "Log off" }));

      await vi.waitFor(() => expect(heard).toHaveBeenCalled());
      await settle();
      expect(heard).toHaveBeenCalledTimes(1);
      stop();
    });

    it("does not announce a refused key, or a log off it heard from another tab", async () => {
      meRequests(unauthorizedResponse);
      const heard = vi.fn();
      const stop = listenForLogOff(heard);

      renderProvider();
      await userEvent.click(
        screen.getByRole("button", { name: "Check and remember" }),
      );
      await screen.findByText("rejected");
      anotherTab.logsOff();
      await screen.findByText("signed_out");
      await settle();

      expect(heard).toHaveBeenCalledTimes(1);
      stop();
    });
  });
});
