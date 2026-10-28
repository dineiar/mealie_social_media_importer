import { describe, expect, it } from "vitest";
import { KEY } from "../test/fixtures";
import { createKeyStorage } from "./keyStorage";

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
