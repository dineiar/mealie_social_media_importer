import { act } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import type { Me } from "../api/types/Me";
import { type ApiKey, parseApiKey } from "../auth/apiKey";
import { STORAGE_KEY } from "../auth/keyStorage";
import { announceLogOff } from "../auth/logOffChannel";
import { server } from "./server";

export const KEY_TEXT = "mealie-key-0123456789";

export function requireApiKey(raw: string): ApiKey {
  const key = parseApiKey(raw);
  if (key === null) {
    throw new Error(`the fixture key ${JSON.stringify(raw)} must parse`);
  }
  return key;
}

export const KEY = requireApiKey(KEY_TEXT);

export const OTHER_KEY_TEXT = "another-key-9876543210";
export const OTHER_KEY = requireApiKey(OTHER_KEY_TEXT);

export function aMe(overrides: Partial<Me> = {}): Me {
  return {
    id: "7c1f2a9e-4d3b-4a52-8e61-0b9d7a4c5f10",
    username: "ana",
    full_name: "Ana Souza",
    group_slug: "home",
    ai: { enabled: true, image_enabled: true },
    ...overrides,
  };
}

export const unauthorizedResponse = () =>
  HttpResponse.json(
    { error: { kind: "unauthorized", message: "Key rejected" } },
    { status: 401 },
  );

export const mealieUnreachableResponse = () =>
  HttpResponse.json(
    { error: { kind: "mealie_unreachable", message: "Down" } },
    { status: 502 },
  );

export function meRequests(respond: () => Response | Promise<Response>) {
  const authorizations: (string | null)[] = [];
  server.use(
    http.get("/api/me", ({ request }) => {
      authorizations.push(request.headers.get("Authorization"));
      return respond();
    }),
  );
  return authorizations;
}

export function meIs(me: Me = aMe()) {
  return meRequests(() => HttpResponse.json(me));
}

export function deferred() {
  let release: () => void = () => {};
  const released = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { released, release };
}

export const neverAnswers = () => new Promise<Response>(() => {});

export function holdMeAnswer(me: Me = aMe()) {
  const { released, release } = deferred();
  const authorizations = meRequests(async () => {
    await released;
    return HttpResponse.json(me);
  });
  return { release, authorizations };
}

export function storedValues(): { local: string[]; tab: string[] } {
  return {
    local: Object.values(localStorage),
    tab: Object.values(sessionStorage),
  };
}

export async function flushEffects() {
  await act(async () => {});
}

export const settle = () =>
  new Promise<void>((resolve) => setTimeout(resolve, 20));

export function dispatchStorageEvent(init: StorageEventInit) {
  act(() => {
    window.dispatchEvent(
      new StorageEvent("storage", { storageArea: localStorage, ...init }),
    );
  });
}

export const anotherTab = {
  remembers(key: string = KEY_TEXT) {
    localStorage.setItem(STORAGE_KEY, key);
    dispatchStorageEvent({ key: STORAGE_KEY, newValue: key });
  },
  discardsRememberedKey() {
    localStorage.removeItem(STORAGE_KEY);
    dispatchStorageEvent({ key: STORAGE_KEY, newValue: null });
  },
  logsOff() {
    anotherTab.discardsRememberedKey();
    announceLogOff();
  },
};
