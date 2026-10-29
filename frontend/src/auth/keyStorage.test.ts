import { describe, expect, it, vi } from "vitest";
import { KEY, KEY_TEXT } from "../test/fixtures";
import { createKeyStorage } from "./keyStorage";
import type { StoredKeyChange } from "./session";

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

  it("moves the key when the choice changes, so one storage holds it", () => {
    storage.save({ key: KEY, remember: true });
    storage.save({ key: KEY, remember: false });

    expect(valuesOf(localStorage)).toEqual([]);
    expect(valuesOf(sessionStorage)).toEqual([KEY]);
  });

  it("forgets the key from both storages", () => {
    storage.save({ key: KEY, remember: true });
    sessionStorage.setItem("mealie-importer.api-key", KEY);

    storage.clear();

    expect(valuesOf(localStorage)).toEqual([]);
    expect(valuesOf(sessionStorage)).toEqual([]);
    expect(storage.read()).toBeNull();
  });

  it.each(["", "   ", "has space"])("reads %j as no key", (stored) => {
    localStorage.setItem("mealie-importer.api-key", stored);

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
  });
});

describe("createKeyStorage subscribe", () => {
  function listen() {
    const changes: StoredKeyChange[] = [];
    const stop = storage.subscribe((change) => changes.push(change));
    return { changes, stop };
  }

  function storageEvent(init: StorageEventInit) {
    window.dispatchEvent(
      new StorageEvent("storage", { storageArea: localStorage, ...init }),
    );
  }

  it("reports a key another tab saved to localStorage", () => {
    const { changes, stop } = listen();

    storageEvent({ key: "mealie-importer.api-key", newValue: KEY_TEXT });

    expect(changes).toEqual([{ kind: "remembered", key: KEY }]);
    stop();
  });

  it.each([
    ["removes the key", { key: "mealie-importer.api-key", newValue: null }],
    ["clears localStorage", { key: null, newValue: null }],
    [
      "saves a value that isn't a key",
      { key: "mealie-importer.api-key", newValue: "has space" },
    ],
  ])("reports the key as forgotten when another tab %s", (_name, init) => {
    const { changes, stop } = listen();

    storageEvent(init);

    expect(changes).toEqual([{ kind: "forgotten" }]);
    stop();
  });

  it("ignores other keys and other storage areas", () => {
    const { changes, stop } = listen();

    storageEvent({ key: "theme", newValue: "dark" });
    storageEvent({
      key: "mealie-importer.api-key",
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
    storageEvent({ key: "mealie-importer.api-key", newValue: null });

    expect(listener).not.toHaveBeenCalled();
  });
});
