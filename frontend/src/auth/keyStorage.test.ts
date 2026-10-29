import { describe, expect, it, vi } from "vitest";
import {
  dispatchStorageEvent,
  KEY,
  KEY_TEXT,
  OTHER_KEY_TEXT,
} from "../test/fixtures";
import type { ApiKey } from "./apiKey";
import { createKeyStorage, STORAGE_KEY } from "./keyStorage";

function valuesOf(storage: Storage): string[] {
  return Object.values(storage);
}

const storage = createKeyStorage(
  () => localStorage,
  () => sessionStorage,
);

describe("createKeyStorage", () => {
  it("puts a remembered key in localStorage only", () => {
    storage.save({ key: KEY, remember: true });

    expect(valuesOf(localStorage)).toEqual([KEY]);
    expect(valuesOf(sessionStorage)).toEqual([]);
    expect(storage.read()).toEqual({ key: KEY, remember: true });
  });

  it("puts a key that isn't remembered in sessionStorage only", () => {
    storage.save({ key: KEY, remember: false });

    expect(valuesOf(sessionStorage)).toEqual([KEY]);
    expect(valuesOf(localStorage)).toEqual([]);
    expect(storage.read()).toEqual({ key: KEY, remember: false });
  });

  it("moves a key into localStorage when the tab starts to remember it", () => {
    storage.save({ key: KEY, remember: false });
    storage.save({ key: KEY, remember: true });

    expect(valuesOf(localStorage)).toEqual([KEY]);
    expect(valuesOf(sessionStorage)).toEqual([]);
  });

  it("leaves the remembered key alone when a tab saves a key for itself, so no other tab is logged off", () => {
    localStorage.setItem(STORAGE_KEY, OTHER_KEY_TEXT);

    storage.save({ key: KEY, remember: false });

    expect(valuesOf(localStorage)).toEqual([OTHER_KEY_TEXT]);
    expect(valuesOf(sessionStorage)).toEqual([KEY]);
  });

  it("reads the key a tab holds for itself before the remembered one", () => {
    localStorage.setItem(STORAGE_KEY, OTHER_KEY_TEXT);
    sessionStorage.setItem(STORAGE_KEY, KEY_TEXT);

    expect(storage.read()).toEqual({ key: KEY, remember: false });
  });

  it("forgets the key from both storages", () => {
    storage.save({ key: KEY, remember: true });
    sessionStorage.setItem(STORAGE_KEY, KEY);

    storage.clear();

    expect(valuesOf(localStorage)).toEqual([]);
    expect(valuesOf(sessionStorage)).toEqual([]);
    expect(storage.read()).toBeNull();
  });

  it("discards a remembered key from localStorage and leaves the tab's own copy", () => {
    localStorage.setItem(STORAGE_KEY, KEY_TEXT);
    sessionStorage.setItem(STORAGE_KEY, OTHER_KEY_TEXT);

    storage.discard({ key: KEY, remember: true });

    expect(valuesOf(localStorage)).toEqual([]);
    expect(valuesOf(sessionStorage)).toEqual([OTHER_KEY_TEXT]);
  });

  it("discards a tab's own key from sessionStorage and leaves the remembered key", () => {
    localStorage.setItem(STORAGE_KEY, OTHER_KEY_TEXT);
    sessionStorage.setItem(STORAGE_KEY, KEY_TEXT);

    storage.discard({ key: KEY, remember: false });

    expect(valuesOf(localStorage)).toEqual([OTHER_KEY_TEXT]);
    expect(valuesOf(sessionStorage)).toEqual([]);
  });

  it("does not discard a newer key that took the place of the one refused", () => {
    localStorage.setItem(STORAGE_KEY, OTHER_KEY_TEXT);

    storage.discard({ key: KEY, remember: true });

    expect(valuesOf(localStorage)).toEqual([OTHER_KEY_TEXT]);
  });

  it.each(["", "   ", "has space"])("reads %j as no key", (stored) => {
    localStorage.setItem(STORAGE_KEY, stored);

    expect(storage.read()).toBeNull();
  });

  it("keeps working when the browser blocks storage", () => {
    const blocked = () => {
      throw new DOMException("blocked", "SecurityError");
    };
    const blockedStorage = createKeyStorage(blocked, blocked);

    expect(blockedStorage.read()).toBeNull();
    expect(() =>
      blockedStorage.save({ key: KEY, remember: true }),
    ).not.toThrow();
    expect(() => blockedStorage.clear()).not.toThrow();
    expect(() =>
      blockedStorage.discard({ key: KEY, remember: true }),
    ).not.toThrow();
  });
});

describe("createKeyStorage subscribe", () => {
  function listen() {
    const changes: (ApiKey | null)[] = [];
    const stop = storage.subscribe((key) => changes.push(key));
    return { changes, stop };
  }

  it("reports a key another tab saved to localStorage", () => {
    const { changes, stop } = listen();

    dispatchStorageEvent({ key: STORAGE_KEY, newValue: KEY_TEXT });

    expect(changes).toEqual([KEY]);
    stop();
  });

  it.each([
    ["removes the key", { key: STORAGE_KEY, newValue: null }],
    ["clears localStorage", { key: null, newValue: null }],
    [
      "saves a value that isn't a key",
      { key: STORAGE_KEY, newValue: "has space" },
    ],
  ])("reports the key as forgotten when another tab %s", (_name, init) => {
    const { changes, stop } = listen();

    dispatchStorageEvent(init);

    expect(changes).toEqual([null]);
    stop();
  });

  it("ignores other keys and other storage areas", () => {
    const { changes, stop } = listen();

    dispatchStorageEvent({ key: "theme", newValue: "dark" });
    dispatchStorageEvent({
      key: STORAGE_KEY,
      newValue: KEY_TEXT,
      storageArea: sessionStorage,
    });

    expect(changes).toEqual([]);
    stop();
  });

  it("stops reporting after unsubscribing", () => {
    const listener = vi.fn();
    const stop = storage.subscribe(listener);

    stop();
    dispatchStorageEvent({ key: STORAGE_KEY, newValue: null });

    expect(listener).not.toHaveBeenCalled();
  });
});
