import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse } from "msw";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "./App";
import { createQueryClient } from "./api/queryClient";
import { STORAGE_KEY } from "./auth/keyStorage";
import {
  aMe,
  anotherTab,
  flushEffects,
  holdMeAnswer,
  KEY_TEXT,
  mealieUnreachableResponse,
  meIs,
  meRequests,
  OTHER_KEY_TEXT,
  storedValues,
  unauthorizedResponse,
} from "./test/fixtures";

function renderApp() {
  render(
    <QueryClientProvider client={createQueryClient()}>
      <App />
    </QueryClientProvider>,
  );
}

const keyField = () => screen.getByLabelText("Mealie API key");
const connectButton = () => screen.getByRole("button", { name: /connect/i });
const rememberBox = () =>
  screen.getByRole("checkbox", { name: /remember me/i });

async function connectWith(key: string) {
  await userEvent.click(keyField());
  await userEvent.paste(key);
  await userEvent.click(connectButton());
}

async function replaceKeyWith(key: string) {
  await userEvent.clear(keyField());
  await connectWith(key);
}

describe("the login screen", () => {
  it("starts empty with Remember me checked", () => {
    renderApp();

    expect(
      screen.getByRole("heading", { name: "Connect to Mealie" }),
    ).toBeInTheDocument();
    expect(keyField()).toHaveValue("");
    expect(keyField()).toHaveAttribute("type", "password");
    expect(rememberBox()).toBeChecked();
    expect(connectButton()).toBeEnabled();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("posts the form if the browser ever submits it natively, so the key can't reach the address", () => {
    renderApp();

    const form = connectButton().closest("form");

    expect(form?.method).toBe("post");
  });

  it("shows and hides the key", async () => {
    renderApp();
    await userEvent.type(keyField(), KEY_TEXT);

    await userEvent.click(screen.getByRole("button", { name: "Show key" }));
    expect(keyField()).toHaveAttribute("type", "text");
    expect(keyField()).toHaveValue(KEY_TEXT);

    await userEvent.click(screen.getByRole("button", { name: "Hide key" }));
    expect(keyField()).toHaveAttribute("type", "password");
  });

  it("shows the checking state while Mealie answers", async () => {
    const { release } = holdMeAnswer();
    renderApp();

    await connectWith(KEY_TEXT);

    const submit = screen.getByRole("button", { name: "Checking your key…" });
    expect(submit).toBeDisabled();
    expect(submit.closest("form")).toHaveAttribute("aria-busy", "true");
    expect(keyField()).toHaveAttribute("readonly");
    expect(keyField()).toHaveValue(KEY_TEXT);

    release();
    expect(await screen.findByText("Ana Souza")).toBeInTheDocument();
  });

  it("announces the check to screen readers from a region that is not busy", async () => {
    const { release } = holdMeAnswer();
    renderApp();
    expect(screen.getByRole("status")).toBeEmptyDOMElement();

    await connectWith(KEY_TEXT);

    const announcement = screen.getByRole("status");
    expect(announcement).toHaveTextContent("Checking your key…");
    expect(announcement.closest("[aria-busy]")).toBeNull();

    release();
    await screen.findByText("Ana Souza");
  });

  it("keeps Remember me as chosen while the key is being checked", async () => {
    holdMeAnswer();
    renderApp();
    await connectWith(KEY_TEXT);

    await userEvent.click(rememberBox());

    expect(rememberBox()).toBeChecked();
  });

  it("shows the error and keeps the key in the field when Mealie rejects it", async () => {
    meRequests(unauthorizedResponse);
    renderApp();

    await connectWith(KEY_TEXT);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Mealie didn't accept this key.",
    );
    expect(keyField()).toHaveValue(KEY_TEXT);
    expect(keyField()).toHaveAttribute("aria-invalid", "true");
    expect(keyField()).toHaveAccessibleDescription(/didn't accept this key/);
    expect(keyField()).toHaveFocus();
    expect(keyField()).not.toHaveAttribute("readonly");
    expect(connectButton()).toBeEnabled();
  });

  it("lets the user try another key after a rejection", async () => {
    meRequests(unauthorizedResponse);
    renderApp();
    await connectWith("revoked-key");
    await screen.findByRole("alert");

    const authorizations = meIs();
    await replaceKeyWith(KEY_TEXT);

    expect(await screen.findByText("Ana Souza")).toBeInTheDocument();
    expect(authorizations).toEqual([`Bearer ${KEY_TEXT}`]);
  });

  it("sends a pasted key without the whitespace around it", async () => {
    const authorizations = meIs();
    renderApp();

    await connectWith(`  ${KEY_TEXT}  `);

    await screen.findByText("Ana Souza");
    expect(authorizations).toEqual([`Bearer ${KEY_TEXT}`]);
  });

  it("does not send a key with a character outside printable ASCII", async () => {
    renderApp();

    await connectWith(`${KEY_TEXT}${String.fromCharCode(0x200b)}`);

    expect(connectButton()).toBeEnabled();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});

describe("where the key is kept", () => {
  it("puts the key in localStorage when Remember me is checked", async () => {
    meIs();
    renderApp();

    await connectWith(KEY_TEXT);

    await screen.findByText("Ana Souza");
    await flushEffects();
    expect(storedValues()).toEqual({ local: [KEY_TEXT], tab: [] });
  });

  it("puts the key in sessionStorage when Remember me is unchecked", async () => {
    meIs();
    renderApp();
    await userEvent.click(rememberBox());

    await connectWith(KEY_TEXT);

    await screen.findByText("Ana Souza");
    await flushEffects();
    expect(storedValues()).toEqual({ local: [], tab: [KEY_TEXT] });
  });

  it("forgets the key from both storages on Log off", async () => {
    meIs();
    renderApp();
    await connectWith(KEY_TEXT);
    await screen.findByText("Ana Souza");

    await userEvent.click(screen.getByRole("button", { name: "Log off" }));

    expect(
      screen.getByRole("heading", { name: "Connect to Mealie" }),
    ).toBeInTheDocument();
    expect(keyField()).toHaveValue("");
    expect(storedValues()).toEqual({ local: [], tab: [] });
  });

  it("logs in on load with the stored key", async () => {
    localStorage.setItem(STORAGE_KEY, KEY_TEXT);
    meIs();

    renderApp();

    expect(await screen.findByText("Ana Souza")).toBeInTheDocument();
  });

  it("shows the error with the old key when Mealie rejects the stored one", async () => {
    sessionStorage.setItem(STORAGE_KEY, KEY_TEXT);
    meRequests(unauthorizedResponse);

    renderApp();

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Mealie didn't accept this key.",
    );
    expect(keyField()).toHaveValue(KEY_TEXT);
    await flushEffects();
    expect(storedValues()).toEqual({ local: [], tab: [] });
  });
});

describe("across tabs", () => {
  it("logs this tab off when another tab logs off, so Try again can't save the key back", async () => {
    const authorizations = meRequests(mealieUnreachableResponse);
    renderApp();
    await connectWith(KEY_TEXT);
    await screen.findByRole("alert");
    await flushEffects();
    expect(storedValues().local).toEqual([KEY_TEXT]);

    anotherTab.logsOff();
    await flushEffects();

    expect(
      screen.getByRole("heading", { name: "Connect to Mealie" }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
    expect(keyField()).toHaveValue("");
    expect(storedValues()).toEqual({ local: [], tab: [] });
    expect(authorizations).toHaveLength(1);
  });

  it("logs a logged in tab off when another tab logs off", async () => {
    meIs();
    renderApp();
    await connectWith(KEY_TEXT);
    await screen.findByText("Ana Souza");

    anotherTab.logsOff();

    expect(
      await screen.findByRole("heading", { name: "Connect to Mealie" }),
    ).toBeInTheDocument();
    await flushEffects();
    expect(storedValues()).toEqual({ local: [], tab: [] });
  });

  it("keeps a tab logged off when another tab logs off while its check runs", async () => {
    localStorage.setItem(STORAGE_KEY, KEY_TEXT);
    const { release } = holdMeAnswer();
    renderApp();
    await screen.findByRole("button", { name: "Checking your key…" });

    anotherTab.logsOff();
    release();
    await flushEffects();

    expect(connectButton()).toBeEnabled();
    expect(keyField()).toHaveValue("");
    expect(storedValues()).toEqual({ local: [], tab: [] });
  });

  it("drops the tab's own copy when another tab remembers the same key", async () => {
    meIs();
    renderApp();
    await userEvent.click(rememberBox());
    await connectWith(KEY_TEXT);
    await screen.findByText("Ana Souza");
    await flushEffects();
    expect(storedValues()).toEqual({ local: [], tab: [KEY_TEXT] });

    anotherTab.remembers(KEY_TEXT);
    await flushEffects();

    expect(storedValues()).toEqual({ local: [KEY_TEXT], tab: [] });
    expect(screen.getByText("Ana Souza")).toBeInTheDocument();
  });

  it("follows the remembered key afterwards: Try again keeps it in localStorage and a later log off reaches this tab", async () => {
    meRequests(mealieUnreachableResponse);
    renderApp();
    await userEvent.click(rememberBox());
    await connectWith(KEY_TEXT);
    await screen.findByRole("alert");
    anotherTab.remembers(KEY_TEXT);

    meIs();
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    await screen.findByText("Ana Souza");
    await flushEffects();
    expect(storedValues()).toEqual({ local: [KEY_TEXT], tab: [] });

    anotherTab.logsOff();
    expect(
      await screen.findByRole("heading", { name: "Connect to Mealie" }),
    ).toBeInTheDocument();
  });

  it("checks the key another tab remembered when it differs from this tab's key", async () => {
    meIs();
    renderApp();
    await userEvent.click(rememberBox());
    await connectWith(KEY_TEXT);
    await screen.findByText("Ana Souza");

    const authorizations = meIs(aMe({ full_name: "Bruno Lima" }));
    anotherTab.remembers(OTHER_KEY_TEXT);

    expect(await screen.findByText("Bruno Lima")).toBeInTheDocument();
    expect(authorizations).toEqual([`Bearer ${OTHER_KEY_TEXT}`]);
    await flushEffects();
    expect(storedValues()).toEqual({ local: [OTHER_KEY_TEXT], tab: [] });
  });

  it("leaves a tab alone whose key is not remembered when another tab logs off", async () => {
    meIs();
    renderApp();
    await userEvent.click(rememberBox());
    await connectWith(KEY_TEXT);
    await screen.findByText("Ana Souza");

    anotherTab.logsOff();
    await flushEffects();

    expect(screen.getByText("Ana Souza")).toBeInTheDocument();
    expect(storedValues()).toEqual({ local: [], tab: [KEY_TEXT] });
  });

  it("leaves a signed out tab alone when another tab remembers a key", async () => {
    renderApp();

    anotherTab.remembers(KEY_TEXT);
    await flushEffects();

    expect(connectButton()).toBeEnabled();
    expect(keyField()).toHaveValue("");
  });
});

describe("the header after login", () => {
  it("shows the avatar initial, full name, handle and Log off", async () => {
    meIs();
    renderApp();
    await connectWith(KEY_TEXT);

    const banner = await screen.findByRole("banner");

    expect(within(banner).getByText("A")).toBeInTheDocument();
    expect(within(banner).getByText("Ana Souza")).toBeInTheDocument();
    expect(within(banner).getByText("@ana")).toBeInTheDocument();
    expect(
      within(banner).getByRole("button", { name: "Log off" }),
    ).toBeInTheDocument();
    expect(screen.queryByLabelText("Mealie API key")).not.toBeInTheDocument();
  });
});

describe("the banners", () => {
  it("says nothing when Mealie's AI is on", async () => {
    meIs();
    renderApp();
    await connectWith(KEY_TEXT);
    await screen.findByText("Ana Souza");

    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("warns that recipes can't be created when Mealie's AI is off", async () => {
    meIs(aMe({ ai: { enabled: false, image_enabled: false } }));
    renderApp();
    await connectWith(KEY_TEXT);

    await screen.findByText("Ana Souza");
    const notice = screen.getByRole("status");

    expect(notice).toHaveTextContent("AI is off in Mealie");
    expect(notice).toHaveTextContent(
      "Recipes can't be created until AI is set up in Mealie.",
    );
  });

  it("says Mealie can't be reached and keeps the key", async () => {
    meRequests(mealieUnreachableResponse);
    renderApp();

    await connectWith(KEY_TEXT);

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Mealie can't be reached");
    expect(screen.getByRole("button", { name: "Log off" })).toBeInTheDocument();
    await flushEffects();
    expect(storedValues().local).toEqual([KEY_TEXT]);
  });

  it("says the importer isn't responding when nginx answers a down backend with HTML", async () => {
    meRequests(() =>
      HttpResponse.html("<h1>502 Bad Gateway</h1>", { status: 502 }),
    );
    renderApp();

    await connectWith(KEY_TEXT);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The importer isn't responding",
    );
    await flushEffects();
    expect(storedValues().local).toEqual([KEY_TEXT]);
  });

  it("says the importer isn't responding when /api/me answers 200 with something that isn't a user", async () => {
    meRequests(() => HttpResponse.json({}));
    renderApp();

    await connectWith(KEY_TEXT);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The importer isn't responding",
    );
    expect(screen.getByRole("button", { name: "Log off" })).toBeInTheDocument();
    await flushEffects();
    expect(storedValues().local).toEqual([KEY_TEXT]);
  });

  it("recovers on Try again once Mealie is back", async () => {
    meRequests(mealieUnreachableResponse);
    renderApp();
    await connectWith(KEY_TEXT);
    await screen.findByRole("alert");

    meIs();
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));

    expect(await screen.findByText("Ana Souza")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("keeps Remember me unchecked through Try again", async () => {
    meRequests(mealieUnreachableResponse);
    renderApp();
    await userEvent.click(rememberBox());
    await connectWith(KEY_TEXT);
    await screen.findByRole("alert");
    await flushEffects();
    expect(storedValues()).toEqual({ local: [], tab: [KEY_TEXT] });

    meIs();
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));

    await screen.findByText("Ana Souza");
    await flushEffects();
    expect(storedValues()).toEqual({ local: [], tab: [KEY_TEXT] });
  });

  it("logs off from an unreachable session", async () => {
    meRequests(mealieUnreachableResponse);
    renderApp();
    await connectWith(KEY_TEXT);
    await screen.findByRole("alert");

    await userEvent.click(screen.getByRole("button", { name: "Log off" }));

    expect(keyField()).toHaveValue("");
    expect(storedValues().local).toEqual([]);
  });
});

describe("the link to import", () => {
  afterEach(() => window.history.replaceState(null, "", "/"));

  const SEARCH = `?url=${encodeURIComponent("https://www.instagram.com/reel/C9xYz12AbCd/")}`;

  it("stays in the address through a rejection and a login", async () => {
    window.history.replaceState(null, "", `/${SEARCH}`);
    meRequests(unauthorizedResponse);
    renderApp();
    await connectWith("revoked-key");
    await screen.findByRole("alert");
    expect(window.location.search).toBe(SEARCH);

    meIs();
    await replaceKeyWith(KEY_TEXT);
    await screen.findByText("Ana Souza");

    expect(window.location.search).toBe(SEARCH);
  });
});
